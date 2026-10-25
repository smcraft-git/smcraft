//! The Course Maker's sound effects (research/specs/sound-effect-parts.md, private): the palette
//! bar's sound button (`L_PaletteSet_00/L_SoundBtn_00`) turns the palette into the effects'
//! (their pictures `Lyt_E_P_SE_<label>`, names from `SoundIconName`), the search button opens
//! the effect window (the ring with the five sound categories, `L_Category_01`: Feelings,
//! Stingers, Reactions, Animations, Music, one page each), and a press puts the effect on the
//! tile: on Mario, on a part that can hold one, else on the empty tile (`mmk_editor::
//! sound_effects`). The eraser (or the back button) takes it off again.
//!
//! The course shows every effect with its icon from the sheet `Otoasobi_CategoryPanel`: a
//! spot's big icon on its tile, a part's (and Mario's) small framed icon at the part's top
//! right [inf: which size goes where; capture `maker-sound-icons`].
//!
//! Each effect sounds its Course Maker user's `cue` (`EditOtoasobi<id>`) when it is chosen (the
//! sound palette's choice, 0x71018e6710 [obs v3.0.3]) or put down [inf: 0x7100f0eaa0, the icon
//! object's virtual, its moment not read]; the maker's calls run the effect without its picture.
//! The sound button's balloon names the palette it switches to (`Mode_Disp_Sound` /
//! `Mode_Disp_Parts`, 0x71018e2fe0) [obs v3.0.3].

use mmk_editor::EditError;
use mmk_editor::sound_effects::SoundTarget;
use mmk_formats::smm2::sound_effects::{
    CATEGORY_KEYS, ICON_IDS, PAGES, label, page_of, sheet_cell,
};
use mmk_ui::{Quad, TEXTURE_NONE};

use super::view::{self, quad};
use super::{Buttons, MakerFlow, Mode, PAGE, RING, Ring, SLOTS, hit, keys, slot_part};
use crate::sound::Event;

/// The course icons' sheet (in the import's `EditModel` group).
pub const SHEET: &str = "Otoasobi_CategoryPanel";
const SHEET_W: f32 = 1536.0;
const SHEET_H: f32 = 1290.0;
const CELL: f32 = 128.0;
/// The small framed icons: five rows lower, an 88-px picture 20 px into its cell.
const SMALL_ROW: usize = 5;
const SMALL_INSET: f32 = 20.0;
const SMALL: f32 = 88.0;

/// The maker's effect tools.
#[derive(Clone, Debug)]
pub struct SoundTools {
    /// The palette shows the effects (the sound button is on).
    pub on: bool,
    /// The palette's effects (course ids), most recent first, and the selected slot.
    pub palette: Vec<u8>,
    pub slot: usize,
    pub(super) ring: Ring,
    /// Effects to sound this tick (their `EditOtoasobi<id>` cue).
    pub previews: Vec<u8>,
}

impl Default for SoundTools {
    /// The palette starts with the first twelve effects of the Feelings and Stingers pages
    /// [inf].
    fn default() -> Self {
        Self {
            on: false,
            palette: PAGES[0]
                .iter()
                .chain(PAGES[1])
                .take(SLOTS)
                .map(|&i| ICON_IDS[i])
                .collect(),
            slot: 0,
            ring: Ring::default(),
            previews: Vec::new(),
        }
    }
}

/// The palette picture of an effect.
pub fn picture(id: u8) -> Option<String> {
    label(id).map(|l| format!("Lyt_E_P_SE_{l}"))
}

/// The Course Maker's sound user of an effect.
pub fn edit_user(id: u8) -> String {
    format!("EditOtoasobi{id:02}")
}

impl MakerFlow {
    /// The selected effect.
    pub fn sound_effect(&self) -> Option<u8> {
        self.se.palette.get(self.se.slot).copied()
    }

    /// The sound button: the palette between the parts and the effects.
    pub(super) fn toggle_sound_mode(&mut self) {
        self.se.on = !self.se.on;
        self.eraser = false;
        if let Some(p) = &mut self.page {
            let b = "L_PaletteSet_00/L_SoundBtn_00";
            p.play(&format!("{b}/Decide"), self.tick);
            p.hold(&format!("{b}/Toggle"), if self.se.on { 1.0 } else { 0.0 });
            if self.se.on {
                p.play(&format!("{b}/ButtonLoop"), self.tick);
            } else {
                p.stop(&format!("{b}/ButtonLoop"));
            }
            p.hold("L_RightParts_00/L_EraserBtn_00/Toggle", 0.0);
        }
        self.ui
            .sound_button(PAGE, "L_PaletteSet_00/L_SoundBtn_00", Event::Decide);
        if self.se.on
            && let Some(id) = self.sound_effect()
        {
            self.se.previews.push(id);
        }
        self.update_page();
    }

    /// The palette slots' effect pictures (from `update_page` while the sound button is on).
    pub(super) fn layout_sound_slots(&mut self) {
        let (palette, slot, eraser) = (self.se.palette.clone(), self.se.slot, self.eraser);
        let Some(p) = &mut self.page else { return };
        for i in 0..SLOTS {
            let part = format!("L_PaletteSet_00/{}", slot_part(i));
            let icon = format!("{part}/P_Icon_00");
            match palette.get(i).and_then(|&id| picture(id)) {
                Some(name) => {
                    p.show(&part, true);
                    p.hold(&format!("{part}/Invisible"), 0.0);
                    p.hold(
                        &format!("{part}/Check"),
                        if i == slot && !eraser { 1.0 } else { 0.0 },
                    );
                    p.hold(&format!("{part}/Select"), if i == slot { 2.0 } else { 0.0 });
                    p.show(&icon, true);
                    p.image(&icon, view::GROUP, &name);
                }
                None => {
                    p.hold(&format!("{part}/Invisible"), 1.0);
                    p.show(&icon, false);
                }
            }
        }
    }

    pub(super) fn select_sound_slot(&mut self, i: usize) {
        if i >= self.se.palette.len() {
            return;
        }
        if i != self.se.slot {
            self.ui.sound_button(
                PAGE,
                &format!("L_PaletteSet_00/{}", slot_part(i)),
                Event::Select,
            );
        }
        self.se.slot = i;
        self.eraser = false;
        if let Some(id) = self.sound_effect() {
            self.se.previews.push(id);
        }
        if let Some(p) = &mut self.page {
            p.hold("L_RightParts_00/L_EraserBtn_00/Toggle", 0.0);
        }
        self.update_page();
    }

    /// The effect's name (`SoundIconName`).
    pub fn sound_effect_name(&self, id: u8) -> String {
        label(id)
            .and_then(|l| self.ui.message("SoundIconName", l))
            .unwrap_or_else(|| label(id).unwrap_or("?").to_string())
    }

    /// A press on the course in sound mode: the effect onto the tile.
    pub(super) fn press_place_sound(&mut self, cx: &mut crate::Cx) {
        let (tx, ty) = self.cur;
        let Some(id) = self.sound_effect() else {
            return;
        };
        self.editor.checkpoint();
        match self.editor.put_sound_effect(id, tx, ty) {
            Ok(_) => {
                self.se.previews.push(id);
                self.changed();
            }
            Err(e) => {
                self.editor.discard_checkpoint();
                self.refuse_sound(cx, &e);
            }
        }
    }

    fn refuse_sound(&mut self, cx: &mut crate::Cx, e: &EditError) {
        super::refuse(cx, &self.ui, e);
        let part = match self.editor.object_at(self.cur.0, self.cur.1) {
            Some(i) => self.editor.object(i).map_or(7, |o| o.id),
            None => 7,
        };
        self.sound(part, "invalid");
    }

    /// The eraser in sound mode: the effect off the tile.
    pub(super) fn erase_sound_here(&mut self) -> bool {
        let (tx, ty) = self.cur;
        let Some((_, at)) = self.editor.sound_effect_at(tx, ty) else {
            return false;
        };
        let part = match at {
            SoundTarget::Part(i) => self.editor.object(i).map_or(7, |o| o.id),
            _ => 7,
        };
        // (the erase press keeps one checkpoint, as `erase_here`)
        if !self.editor.erase_sound_effect(tx, ty) {
            return false;
        }
        self.sound(part, "eraser");
        if let Some(super::Drag::Erase { changed }) = &mut self.drag {
            *changed = true;
        }
        self.changed();
        true
    }

    // ------------------------------------------------------------ the effect window

    pub(super) fn open_sound_ring(&mut self) {
        let mut ring = Ring::default();
        if let Some((pg, at)) = self.sound_effect().and_then(page_of) {
            (ring.category, ring.sel) = (pg, at);
        }
        self.se.ring = ring;
        self.mode = Mode::Ring;
        self.ui
            .sound_button(PAGE, "L_PaletteSet_00/L_SearchBtn_00", Event::Decide);
        if let Some(p) = &mut self.page {
            p.show("L_RingMenu_00", true);
            p.show("L_RingMenu_00/PF_Base_00", false);
            p.stop("L_RingMenu_00/Out");
            p.play("L_RingMenu_00/In", self.tick);
            p.stop("L_PaletteSet_00/In");
            p.play("L_PaletteSet_00/Out", self.tick);
        }
        self.layout_sound_ring(true);
    }

    /// Show sound category `c` in the open window (captures).
    pub(super) fn se_ring_category(&mut self, c: usize) {
        self.se.ring = Ring {
            category: c.min(PAGES.len() - 1),
            ..Default::default()
        };
        self.layout_sound_ring(true);
    }

    /// The ring with the current sound category: its effects' pictures, the selection, the
    /// five-tab category bar (`L_Category_01`) [obs data].
    pub(super) fn layout_sound_ring(&mut self, change: bool) {
        let cat = self.se.ring.category.min(PAGES.len() - 1);
        let page: Vec<u8> = PAGES[cat].iter().map(|&i| ICON_IDS[i]).collect();
        let n = page.len().max(1);
        self.se.ring.sel = self.se.ring.sel.min(n - 1);
        let sel = self.se.ring.sel;
        let name = page.get(sel).map(|&id| self.sound_effect_name(id));
        let title = self
            .ui
            .message(PAGE, &format!("CategoryName_{}", CATEGORY_KEYS[cat]));
        let tick = self.tick;
        let Some(p) = &mut self.page else { return };
        let set = "L_RingMenu_00/L_RingIconSet_00";
        p.hold(&format!("{set}/Type"), (n.max(2) - 1) as f32);
        p.hold(&format!("{set}/Select"), sel as f32);
        p.hold(&format!("{set}/Category"), cat as f32);
        for k in 0..RING {
            let icon = format!("{set}/L_RingIcon_{:02}", k + 1);
            match page.get(k).and_then(|&id| picture(id)) {
                Some(name) => {
                    p.visible(&icon, None);
                    let pic = format!("{icon}/P_Dummy_00");
                    p.show(&pic, true);
                    p.image(&pic, view::GROUP, &name);
                    p.show(&format!("{icon}/L_NewIcon_00"), false);
                }
                None => p.show(&icon, false),
            }
        }
        let centre = format!("{set}/L_RingIcon_00");
        p.show(&format!("{centre}/L_NewIcon_00"), false);
        match page.get(sel).and_then(|&id| picture(id)) {
            Some(name) => {
                p.show(&format!("{centre}/P_Dummy_00"), true);
                p.image(&format!("{centre}/P_Dummy_00"), view::GROUP, &name);
            }
            None => p.show(&format!("{centre}/P_Dummy_00"), false),
        }
        p.show("L_RingMenu_00/L_Category_00", false);
        p.show("L_RingMenu_00/L_Category_01", true);
        let bar = "L_RingMenu_00/L_Category_01";
        for c in 0..PAGES.len() {
            let a = format!("{bar}/ChangeCategory{c:02}");
            if c == cat {
                if change {
                    p.play(&a, tick);
                } else {
                    p.hold_end(&a);
                }
            } else {
                p.stop(&a);
            }
        }
        p.hold(&format!("{bar}/PageVolume"), 0.0);
        for d in 0..6 {
            p.show(&format!("{bar}/P_Page_{d:02}"), false);
        }
        if let Some(t) = title {
            p.text(&format!("{bar}/T_Category_00"), &t);
        }
        let balloon = "L_RingMenu_00/L_Balloon_00";
        p.show(&format!("{balloon}/T_Balloon_01"), false);
        if let Some(n) = name {
            p.text(&format!("{balloon}/T_Balloon_00"), &n);
        }
    }

    pub(super) fn sound_ring_tick(&mut self, pressed: Buttons, keys: u16, click: Option<[f32; 2]>) {
        let n = PAGES[self.se.ring.category.min(PAGES.len() - 1)]
            .len()
            .max(1);
        if keys & keys::PARTS != 0 || pressed.has(Buttons::BACK) {
            self.close_ring();
            return;
        }
        if let Some(p) = click {
            self.sound_ring_click(p);
            return;
        }
        if pressed.has(Buttons::RIGHT) || pressed.has(Buttons::LEFT) {
            let step = if pressed.has(Buttons::RIGHT) {
                1
            } else {
                n - 1
            };
            self.se.ring.sel = (self.se.ring.sel + step) % n;
            self.ui.sound_key(PAGE, "p_MoveRightRingMenu", &[]);
            self.layout_sound_ring(false);
        } else if pressed.has(Buttons::UP) || pressed.has(Buttons::DOWN) {
            // one page a category: up / down go through the categories
            let k = PAGES.len();
            let c = self.se.ring.category;
            self.se.ring.category = if pressed.has(Buttons::DOWN) {
                (c + 1) % k
            } else {
                (c + k - 1) % k
            };
            self.se.ring.sel = 0;
            self.ui.sound_key(PAGE, "p_MoveLeftRingMenu", &[]);
            self.layout_sound_ring(true);
        } else if pressed.has(Buttons::CONFIRM) {
            self.choose_sound_ring();
        }
    }

    fn sound_ring_click(&mut self, p: [f32; 2]) {
        let tick = self.tick;
        let Some(page) = &mut self.page else { return };
        let set = "L_RingMenu_00/L_RingIconSet_00";
        let icons: Vec<String> = (0..RING)
            .map(|k| format!("{set}/L_RingIcon_{:02}/W_Square_00", k + 1))
            .collect();
        if let Some(k) = hit(page, tick, &icons, p) {
            self.se.ring.sel = k;
            self.layout_sound_ring(false);
            self.choose_sound_ring();
            return;
        }
        let tabs: Vec<String> = (0..PAGES.len())
            .map(|c| format!("L_RingMenu_00/L_Category_01/L_BdBase_{c:02}"))
            .collect();
        if let Some(c) = hit(page, tick, &tabs, p) {
            if c != self.se.ring.category {
                self.se.ring = Ring {
                    category: c,
                    ..Default::default()
                };
                self.layout_sound_ring(true);
            }
            return;
        }
        let close = ["L_RingMenu_00/L_CloseBtn_00".to_string()];
        if hit(page, tick, &close, p).is_some() {
            self.close_ring();
        }
    }

    /// The chosen effect goes to the front of the palette (a history, as the parts') and is
    /// selected.
    fn choose_sound_ring(&mut self) {
        let cat = self.se.ring.category.min(PAGES.len() - 1);
        let Some(&icon) = PAGES[cat].get(self.se.ring.sel) else {
            return;
        };
        let id = ICON_IDS[icon];
        self.se.palette.retain(|&x| x != id);
        self.se.palette.insert(0, id);
        self.se.palette.truncate(SLOTS);
        self.se.slot = 0;
        self.se.previews.push(id);
        self.eraser = false;
        if let Some(p) = &mut self.page {
            p.play("L_PaletteSet_00/Sort", self.tick);
            p.hold("L_RightParts_00/L_EraserBtn_00/Toggle", 0.0);
        }
        self.ui
            .sound_button(PAGE, "L_PaletteSet_00/L_PaletteBtn_", Event::Decide);
        self.close_ring();
        self.update_page();
    }

    // ------------------------------------------------------------ on the course

    /// The course's effects as layout quads: their icons from the sheet (a flat square where
    /// the import lacks it).
    pub fn sound_effect_quads(&self, q: &mut Vec<Quad>) {
        let tex = self.model_texture(SHEET);
        for e in self.editor.placed_effects() {
            let Some((col, row)) = sheet_cell(e.id) else {
                continue;
            };
            let (x, y) = (e.tile.0 as f32 * 16.0, e.tile.1 as f32 * 16.0);
            let (r, uv) = match e.target {
                SoundTarget::Spot(_) => {
                    let [x0, y0] = self.to_layout(x, y);
                    let [x1, y1] = self.to_layout(x + 16.0, y + 16.0);
                    let (u, v) = (col as f32 * CELL, row as f32 * CELL);
                    ([x0, y0, x1, y1], [u, v, u + CELL, v + CELL])
                }
                // a part's and Mario's: the small framed icon over the tile's top-right corner
                _ => {
                    let [x0, y0] = self.to_layout(x + 8.0, y + 8.0);
                    let [x1, y1] = self.to_layout(x + 20.0, y + 20.0);
                    let u = col as f32 * CELL + SMALL_INSET;
                    let v = (row + SMALL_ROW) as f32 * CELL + SMALL_INSET;
                    ([x0, y0, x1, y1], [u, v, u + SMALL, v + SMALL])
                }
            };
            if r[2] < -640.0 || r[0] > 640.0 || r[3] < -360.0 || r[1] > 360.0 {
                continue;
            }
            match tex {
                Some(t) => q.push(quad(
                    r,
                    t,
                    [
                        uv[0] / SHEET_W,
                        uv[1] / SHEET_H,
                        uv[2] / SHEET_W,
                        uv[3] / SHEET_H,
                    ],
                    [255; 4],
                )),
                None => q.push(quad(
                    r,
                    TEXTURE_NONE,
                    [0.0, 0.0, 1.0, 1.0],
                    [255, 220, 60, 200],
                )),
            }
        }
    }

    /// The tick's effect sounds, by their Course Maker users.
    pub(super) fn sound_effect_previews(&mut self, host: &mut dyn crate::host::Host) {
        let style = self.style();
        for id in std::mem::take(&mut self.se.previews) {
            host.edit_sound(&style, &edit_user(id), "cue", None);
        }
    }
}
