//! Glyphs drawn at run time from the import's scalable fonts.
//!
//! SMM2's fonts are scalable (see `mmk_formats::bfcpx`). The importer bakes the characters the
//! game's own messages need into bitmap fonts, and keeps the plain font files
//! (`ui/scalable/…`, [`scalable_path`]) and each baked font's recipe (its combination, extra
//! fonts and metrics: [`Face`], the `scalable` object of `fonts/<Name>.json`). A character the
//! sheets lack (kana, kanji and hangul in course and maker names) is rasterised here on first
//! use, at the baked font's metrics, into a shared set of glyph sheets ([`GlyphAtlas`]).
//!
//! Drawing is `&self` (the layout player), so the atlas places glyphs behind a lock and records
//! the pixels each sheet needs; the cache copies them into its textures once a frame
//! ([`crate::UiCache::update_glyphs`]), where the renderers find them: a sheet is a texture of
//! the cache whose pixels change ([`crate::TextureEntry::version`]). The cache always keeps one
//! empty sheet ready, so a glyph never waits for a texture id; with [`MAX_SHEETS`] in use, the
//! sheet drawn longest ago is emptied for new glyphs.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};

use ab_glyph::{Font as _, FontVec, PxScale, ScaleFont, point};
use mmk_formats::bfcpx::{Combination, RangeMode, SubFont};
use mmk_formats::bffnt::CharWidth;
use serde_json::{Value, json};

use crate::TEXTURE_NONE;
use crate::files::{Fetch, UiFiles};
use crate::font::Glyph;

/// Width and height of a run-time glyph sheet (RGBA8: white, coverage in alpha).
pub const SHEET_SIZE: u32 = 1024;
/// Sheets kept at most (4 MiB each); then the one drawn longest ago is reused.
pub const MAX_SHEETS: usize = 4;

/// A glyph's coverage, `w`×`h` (row-major; empty for a blank glyph), and its metrics.
#[derive(Clone, Debug, PartialEq)]
pub struct Raster {
    pub px: Vec<u8>,
    pub w: u32,
    pub h: u32,
    pub width: CharWidth,
}

fn px_scale(f: &FontVec, px_per_em: f32) -> PxScale {
    let upem = f.units_per_em().unwrap_or(1000.0);
    PxScale::from(px_per_em * f.height_unscaled() / upem)
}

/// Rasterise `c` from `f` at `px_per_em`, baseline `baseline` px below the cell top, clipped to
/// `cell_h` rows. None when the font has no glyph for it (or an empty one for a character that
/// is not a space: MARIOMAKER maps kana it doesn't draw to empty glyphs).
pub fn raster(f: &FontVec, c: char, px_per_em: f32, baseline: f32, cell_h: u32) -> Option<Raster> {
    let id = f.glyph_id(c);
    if id.0 == 0 {
        return None;
    }
    let scale = px_scale(f, px_per_em);
    let advance = f.as_scaled(scale).h_advance(id);
    let glyph = id.with_scale_and_position(scale, point(0.0, baseline));
    let Some(outline) = f.outline_glyph(glyph) else {
        if !c.is_whitespace() && !c.is_control() && !matches!(c, '\u{200B}'..='\u{200D}') {
            return None;
        }
        // blank glyph (space): advance only
        return Some(Raster {
            px: Vec::new(),
            w: 0,
            h: cell_h,
            width: CharWidth {
                left: 0,
                glyph_width: 0,
                char_width: advance.round().clamp(0.0, 255.0) as u8,
            },
        });
    };
    let b = outline.px_bounds();
    let left = b.min.x.floor();
    let w = (b.max.x.ceil() - left).max(1.0) as u32;
    let mut px = vec![0u8; (w * cell_h) as usize];
    outline.draw(|x, y, cov| {
        let gx = x as i64 + (b.min.x - left).round() as i64;
        let gy = y as i64 + b.min.y.round() as i64;
        if gx >= 0 && (gx as u32) < w && gy >= 0 && (gy as u32) < cell_h {
            let i = (gy as u32 * w + gx as u32) as usize;
            px[i] = px[i].max((cov * 255.0).round().clamp(0.0, 255.0) as u8);
        }
    });
    Some(Raster {
        px,
        w,
        h: cell_h,
        width: CharWidth {
            left: left.clamp(-128.0, 127.0) as i8,
            glyph_width: w.min(255) as u8,
            char_width: advance.round().clamp(0.0, 255.0) as u8,
        },
    })
}

/// Where the importer keeps the plain (decrypted) font file `file` of `Font/ScalableFont.szs`:
/// `ui/scalable/<stem>.otf` for a `.bfotf`, `.ttf` otherwise.
pub fn scalable_path(file: &str) -> String {
    let (stem, ext) = file.rsplit_once('.').unwrap_or((file, ""));
    let ext = if ext.eq_ignore_ascii_case("bfotf") {
        "otf"
    } else {
        "ttf"
    };
    format!("ui/scalable/{stem}.{ext}")
}

/// A font to try after a combination's own: file, glyph scale, baseline shift (px, up).
#[derive(Clone, Debug, PartialEq)]
pub struct Source {
    pub file: String,
    pub scale: f32,
    pub shift: i16,
}

impl Source {
    pub fn new(file: &str, scale: f32, shift: i16) -> Self {
        Self {
            file: file.to_string(),
            scale,
            shift,
        }
    }
}

/// How a baked font draws a character: its combination's entries that list it (later entries
/// first), then every entry (last resort), then the extra fonts in order.
#[derive(Clone, Debug, PartialEq)]
pub struct Face {
    pub comb: Combination,
    pub extra: Vec<Source>,
    /// The combination's authored size (px per em).
    pub size: f32,
    /// Baseline, px below the cell top, and cell (line) height of the baked font.
    pub baseline: f32,
    pub cell_h: u32,
}

fn mode_name(m: RangeMode) -> &'static str {
    match m {
        RangeMode::All => "all",
        RangeMode::Only => "only",
        RangeMode::Except => "except",
    }
}

impl Face {
    /// The sources to try for `c`, in order: (file, scale, baseline shift).
    pub fn sources(&self, c: char) -> impl Iterator<Item = (&str, f32, i16)> + '_ {
        let comb = &self.comb;
        comb.candidates(c as u32)
            .chain((0..comb.fonts.len()).rev())
            .map(|i| {
                let s = &comb.fonts[i];
                (s.file.as_str(), s.scale, s.baseline_shift)
            })
            .chain(
                self.extra
                    .iter()
                    .map(|s| (s.file.as_str(), s.scale, s.shift)),
            )
    }

    /// Rasterise `c` from the first source that has it (`fonts`: a source font by file name):
    /// (that file, the glyph).
    pub fn raster<F>(&self, c: char, mut fonts: F) -> Option<(&str, Raster)>
    where
        F: FnMut(&str) -> Option<Arc<FontVec>>,
    {
        for (file, scale, shift) in self.sources(c) {
            let Some(f) = fonts(file) else {
                continue;
            };
            let em = self.size * if scale > 0.0 { scale } else { 1.0 };
            if let Some(r) = raster(&f, c, em, self.baseline - shift as f32, self.cell_h) {
                return Some((file, r));
            }
        }
        None
    }

    /// Every font file the face may read.
    pub fn files(&self) -> impl Iterator<Item = &str> {
        self.comb
            .fonts
            .iter()
            .map(|f| f.file.as_str())
            .chain(self.extra.iter().map(|s| s.file.as_str()))
    }

    /// The `scalable` object of `fonts/<Name>.json`.
    pub fn to_json(&self) -> Value {
        json!({
            "size": self.size,
            "baseline": self.baseline,
            "cell_height": self.cell_h,
            "fonts": self.comb.fonts.iter().map(|f| json!({
                "file": f.file,
                "mode": mode_name(f.mode),
                "ranges": f.ranges.iter().map(|&(a, b)| json!([a, b])).collect::<Vec<_>>(),
                "scale": f.scale,
                "shift": f.baseline_shift,
            })).collect::<Vec<_>>(),
            "extra": self.extra.iter().map(|s| json!({
                "file": s.file, "scale": s.scale, "shift": s.shift,
            })).collect::<Vec<_>>(),
        })
    }

    pub fn from_json(v: &Value) -> Option<Self> {
        let f32_of = |v: &Value| v.as_f64().map(|x| x as f32);
        let mut comb = Combination {
            size: f32_of(&v["size"])?,
            ..Default::default()
        };
        for e in v["fonts"].as_array()? {
            comb.fonts.push(SubFont {
                file: e["file"].as_str()?.to_string(),
                mode: match e["mode"].as_str()? {
                    "only" => RangeMode::Only,
                    "except" => RangeMode::Except,
                    _ => RangeMode::All,
                },
                ranges: e["ranges"]
                    .as_array()?
                    .iter()
                    .filter_map(|r| Some((r[0].as_u64()? as u32, r[1].as_u64()? as u32)))
                    .collect(),
                scale: f32_of(&e["scale"])?,
                baseline_shift: e["shift"].as_i64()? as i16,
                ..Default::default()
            });
        }
        let extra = v["extra"]
            .as_array()?
            .iter()
            .filter_map(|e| {
                Some(Source {
                    file: e["file"].as_str()?.to_string(),
                    scale: f32_of(&e["scale"])?,
                    shift: e["shift"].as_i64()? as i16,
                })
            })
            .collect();
        Some(Self {
            size: comb.size,
            comb,
            extra,
            baseline: f32_of(&v["baseline"])?,
            cell_h: v["cell_height"].as_u64()?.clamp(1, 255) as u32,
        })
    }
}

/// Where a run-time glyph is.
#[derive(Clone, Copy, Debug)]
enum Slot {
    /// No source font has the character.
    Missing,
    /// Advance only (a space).
    Blank(CharWidth),
    At {
        sheet: usize,
        x: u32,
        y: u32,
        h: u32,
        width: CharWidth,
    },
}

#[derive(Clone, Copy, Debug)]
struct Shelf {
    y: u32,
    h: u32,
    /// Next free column.
    x: u32,
}

#[derive(Debug)]
struct Sheet {
    texture: u32,
    shelves: Vec<Shelf>,
    /// First row below the shelves.
    bottom: u32,
    /// Glyphs placed (since the last reuse).
    glyphs: usize,
    /// The last frame one of its glyphs was looked up.
    used: u64,
    /// Emptied for reuse: the texture is cleared before `blits`.
    clear: bool,
    /// Glyphs placed since the cache last copied them: (x, y, coverage).
    blits: Vec<(u32, u32, Raster)>,
}

impl Sheet {
    fn new(texture: u32) -> Self {
        Self {
            texture,
            shelves: Vec::new(),
            bottom: 1,
            glyphs: 0,
            used: 0,
            clear: false,
            blits: Vec::new(),
        }
    }

    /// Room for a `w`×`h` glyph (1-px gutters): its top-left corner.
    fn alloc(&mut self, w: u32, h: u32) -> Option<(u32, u32)> {
        if let Some(s) = self
            .shelves
            .iter_mut()
            .find(|s| s.h == h && s.x + w < SHEET_SIZE)
        {
            let x = s.x;
            s.x += w + 1;
            return Some((x, s.y));
        }
        if self.bottom + h < SHEET_SIZE && 1 + w < SHEET_SIZE {
            let y = self.bottom;
            self.shelves.push(Shelf { y, h, x: 2 + w });
            self.bottom += h + 1;
            return Some((1, y));
        }
        None
    }

    fn reset(&mut self) {
        self.shelves.clear();
        self.bottom = 1;
        self.glyphs = 0;
        self.blits.clear();
        self.clear = true;
    }
}

#[derive(Default)]
struct State {
    faces: Vec<Face>,
    /// Source fonts by file (None: not in the import).
    fonts: HashMap<String, Option<Arc<FontVec>>>,
    glyphs: HashMap<(u32, char), Slot>,
    sheets: Vec<Sheet>,
    frame: u64,
    /// A glyph found no room this frame: the cache adds a sheet.
    short: bool,
}

/// Pixels for one sheet texture, from [`GlyphAtlas::take_updates`].
#[derive(Debug)]
pub struct SheetUpdate {
    pub texture: u32,
    /// Empty the texture first (the sheet was reused).
    pub clear: bool,
    /// (x, y, coverage) to write.
    pub blits: Vec<(u32, u32, Raster)>,
}

/// The run-time glyphs of one UI import: faces, source fonts, sheets (see the module notes).
pub struct GlyphAtlas {
    files: Option<Arc<dyn UiFiles>>,
    state: Mutex<State>,
}

impl std::fmt::Debug for GlyphAtlas {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let s = self.lock();
        write!(
            f,
            "GlyphAtlas({} faces, {} glyphs, {} sheets)",
            s.faces.len(),
            s.glyphs.len(),
            s.sheets.len()
        )
    }
}

impl GlyphAtlas {
    /// Source fonts are read from `files` ([`scalable_path`]) when first needed.
    pub fn new(files: Option<Arc<dyn UiFiles>>) -> Self {
        Self {
            files,
            state: Mutex::new(State::default()),
        }
    }

    /// With source fonts given here (tests, tools): file name → font.
    pub fn with_fonts(fonts: HashMap<String, FontVec>) -> Self {
        let a = Self::new(None);
        a.lock().fonts = fonts
            .into_iter()
            .map(|(k, f)| (k, Some(Arc::new(f))))
            .collect();
        a
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn add_face(&self, face: Face) -> u32 {
        let mut s = self.lock();
        s.faces.push(face);
        (s.faces.len() - 1) as u32
    }

    /// Give the atlas another (empty) sheet: texture `texture` of the cache.
    pub fn add_sheet(&self, texture: u32) {
        self.lock().sheets.push(Sheet::new(texture));
    }

    pub fn sheet_count(&self) -> usize {
        self.lock().sheets.len()
    }

    /// Whether the cache should add a sheet now: none yet, a glyph found no room, or the newest
    /// one is in use (one empty sheet is kept ready), below [`MAX_SHEETS`].
    pub fn wants_sheet(&self) -> bool {
        let s = self.lock();
        s.sheets.len() < MAX_SHEETS && (s.short || s.sheets.last().is_none_or(|sh| sh.glyphs > 0))
    }

    /// The pixels placed since the last call, per sheet; starts the next frame.
    pub fn take_updates(&self) -> Vec<SheetUpdate> {
        let mut s = self.lock();
        s.frame += 1;
        s.short = false;
        s.sheets
            .iter_mut()
            .filter(|sh| sh.clear || !sh.blits.is_empty())
            .map(|sh| SheetUpdate {
                texture: sh.texture,
                clear: std::mem::take(&mut sh.clear),
                blits: std::mem::take(&mut sh.blits),
            })
            .collect()
    }

    /// Whether face `face` has a source font with `c` (rasterises it).
    pub fn covers(&self, face: u32, c: char) -> bool {
        self.glyph(face, c).is_some()
    }

    /// The glyph of `c` in face `face`: rasterised and placed on first use. None when no source
    /// font has it, or (rarely) when the sheets have no room this frame.
    pub fn glyph(&self, face: u32, c: char) -> Option<Glyph> {
        let mut guard = self.lock();
        let s = &mut *guard;
        let slot = match s.glyphs.get(&(face, c)) {
            Some(&slot) => slot,
            None => {
                let f = s.faces.get(face as usize)?;
                let files = self.files.as_deref();
                let fonts = &mut s.fonts;
                let got = f.raster(c, |file| {
                    fonts
                        .entry(file.to_string())
                        .or_insert_with(|| load_font(files, file))
                        .clone()
                });
                match got {
                    None => {
                        s.glyphs.insert((face, c), Slot::Missing);
                        return None;
                    }
                    Some((_, r)) if r.w == 0 || r.px.is_empty() => {
                        let slot = Slot::Blank(r.width);
                        s.glyphs.insert((face, c), slot);
                        slot
                    }
                    Some((_, r)) => {
                        let slot = place(s, r)?;
                        s.glyphs.insert((face, c), slot);
                        slot
                    }
                }
            }
        };
        let full = SHEET_SIZE as f32;
        match slot {
            Slot::Missing => None,
            Slot::Blank(width) => Some(Glyph {
                texture: TEXTURE_NONE,
                u0: 0.0,
                v0: 0.0,
                u1: 0.0,
                v1: 0.0,
                width,
            }),
            Slot::At {
                sheet,
                x,
                y,
                h,
                width,
            } => {
                let sh = &mut s.sheets[sheet];
                sh.used = s.frame;
                Some(Glyph {
                    texture: sh.texture,
                    u0: x as f32 / full,
                    v0: y as f32 / full,
                    u1: (x + width.glyph_width as u32) as f32 / full,
                    v1: (y + h) as f32 / full,
                    width,
                })
            }
        }
    }
}

fn load_font(files: Option<&dyn UiFiles>, file: &str) -> Option<Arc<FontVec>> {
    match files?.read(&scalable_path(file)) {
        Fetch::Ready(b) => FontVec::try_from_vec(b).ok().map(Arc::new),
        _ => None,
    }
}

/// Put `r` on a sheet: the first with room; else (all sheets in use) the one drawn longest ago
/// before this frame, emptied. None (and a note for the cache) when none can take it now.
fn place(s: &mut State, r: Raster) -> Option<Slot> {
    let (w, h) = (r.w.min(SHEET_SIZE - 2), r.h.min(SHEET_SIZE - 2));
    let mut spot = s
        .sheets
        .iter_mut()
        .enumerate()
        .find_map(|(i, sh)| sh.alloc(w, h).map(|p| (i, p)));
    if spot.is_none() && s.sheets.len() >= MAX_SHEETS {
        let frame = s.frame;
        if let Some(i) = s
            .sheets
            .iter()
            .enumerate()
            .filter(|(_, sh)| sh.used < frame)
            .min_by_key(|(_, sh)| sh.used)
            .map(|(i, _)| i)
        {
            s.glyphs
                .retain(|_, slot| !matches!(slot, Slot::At { sheet, .. } if *sheet == i));
            s.sheets[i].reset();
            spot = s.sheets[i].alloc(w, h).map(|p| (i, p));
        }
    }
    let Some((i, (x, y))) = spot else {
        s.short = true;
        return None;
    };
    let sh = &mut s.sheets[i];
    sh.glyphs += 1;
    let width = r.width;
    sh.blits.push((x, y, r));
    Some(Slot::At {
        sheet: i,
        x,
        y,
        h,
        width,
    })
}

/// Write `blits` into an RGBA8 sheet's pixels (`data`: [`SHEET_SIZE`]², white).
pub fn blit(data: &mut [u8], blits: &[(u32, u32, Raster)]) {
    for (x0, y0, r) in blits {
        for y in 0..r.h.min(SHEET_SIZE - y0) {
            for x in 0..r.w.min(SHEET_SIZE - x0) {
                let a = r.px.get((y * r.w + x) as usize).copied().unwrap_or(0);
                data[(((y0 + y) * SHEET_SIZE + x0 + x) * 4 + 3) as usize] = a;
            }
        }
    }
}

/// A transparent white sheet.
pub fn empty_sheet() -> Vec<u8> {
    [255u8, 255, 255, 0].repeat((SHEET_SIZE * SHEET_SIZE) as usize)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn glyph_raster(w: u32, h: u32) -> Raster {
        Raster {
            px: vec![200; (w * h) as usize],
            w,
            h,
            width: CharWidth {
                left: 1,
                glyph_width: w as u8,
                char_width: w as u8 + 2,
            },
        }
    }

    #[test]
    fn shelves_pack_and_keep_gutters() {
        let mut sh = Sheet::new(0);
        assert_eq!(sh.alloc(10, 20), Some((1, 1)));
        assert_eq!(sh.alloc(10, 20), Some((12, 1)));
        // another height: a shelf of its own below
        assert_eq!(sh.alloc(5, 30), Some((1, 22)));
        assert_eq!(sh.alloc(10, 20), Some((23, 1)));
        // a full row moves on to a new shelf
        let mut sh = Sheet::new(0);
        let per_row = (SHEET_SIZE - 1) / 101;
        for _ in 0..per_row {
            assert_eq!(sh.alloc(100, 50).unwrap().1, 1);
        }
        assert_eq!(sh.alloc(100, 50), Some((1, 52)));
    }

    #[test]
    fn full_sheets_are_reused_least_recently_drawn_first() {
        let mut s = State::default();
        for t in 0..MAX_SHEETS as u32 {
            s.sheets.push(Sheet::new(10 + t));
        }
        let big = SHEET_SIZE - 3; // one glyph fills a sheet
        for i in 0..MAX_SHEETS {
            let slot = place(&mut s, glyph_raster(big, big)).unwrap();
            assert!(matches!(slot, Slot::At { sheet, .. } if sheet == i));
            s.glyphs.insert((0, char::from(b'a' + i as u8)), slot);
            s.sheets[i].used = 5 + i as u64;
        }
        // all full, all drawn this frame: no room now
        s.frame = 5 + MAX_SHEETS as u64 - 1;
        s.sheets.iter_mut().for_each(|sh| sh.used = s.frame);
        assert!(place(&mut s, glyph_raster(big, big)).is_none());
        assert!(s.short);
        // next frame, sheet 2 drawn longest ago: emptied and reused, its glyph forgotten
        s.frame += 1;
        s.sheets[2].used = 1;
        let slot = place(&mut s, glyph_raster(big, big)).unwrap();
        assert!(matches!(slot, Slot::At { sheet: 2, .. }));
        assert!(s.sheets[2].clear);
        assert!(!s.glyphs.contains_key(&(0, 'c')));
        assert!(s.glyphs.contains_key(&(0, 'a')));
    }

    #[test]
    fn faces_round_trip_through_json() {
        let face = Face {
            comb: Combination {
                size: 46.0,
                fonts: vec![SubFont {
                    file: "A.bfttf".into(),
                    mode: RangeMode::Except,
                    ranges: vec![(0, 0x303F)],
                    scale: 1.5,
                    baseline_shift: -5,
                    ..Default::default()
                }],
                ..Default::default()
            },
            extra: vec![Source::new("B.bfotf", 1.0, 3)],
            size: 46.0,
            baseline: 52.0,
            cell_h: 69,
        };
        assert_eq!(Face::from_json(&face.to_json()), Some(face.clone()));
        assert_eq!(
            face.sources('A').map(|s| s.0).collect::<Vec<_>>(),
            ["A.bfttf", "B.bfotf"]
        );
        // 'A' is in the entry's excepted range (only the last-resort pass); kana are not
        assert_eq!(
            face.sources('あ').map(|s| s.0).collect::<Vec<_>>(),
            ["A.bfttf", "A.bfttf", "B.bfotf"]
        );
        assert_eq!(scalable_path("B.bfotf"), "ui/scalable/B.otf");
        assert_eq!(scalable_path("A.bfttf"), "ui/scalable/A.ttf");
    }

    #[test]
    fn blits_write_coverage_into_alpha() {
        let mut data = empty_sheet();
        blit(&mut data, &[(3, 2, glyph_raster(2, 2))]);
        let a = |x: u32, y: u32| data[((y * SHEET_SIZE + x) * 4 + 3) as usize];
        assert_eq!((a(3, 2), a(4, 3), a(5, 2), a(2, 2)), (200, 200, 0, 0));
        assert_eq!(&data[0..3], &[255, 255, 255]);
    }
}
