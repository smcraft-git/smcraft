//! Wii U BFRES (FRES version 3, big-endian): the embedded textures (FTEX), and — for SMM1's
//! flat 2D models (the ending's stage, Peach and Toad, the 2D styles' player models, all quads
//! facing the camera) — the models' bones, shapes (positions, first UV set, triangles) with
//! their material's texture, texture SRT and wrap modes, and the texture-pattern animations
//! (FTXP).
//!
//! Layout (from the public Wii U BFRES descriptions): the header holds 12 relative offsets to
//! index groups (`0x20 + 4·i`) and their entry counts (u16 at `0x50 + 2·i`); group 1 lists the
//! textures. An index group is `u32 size, i32 count`, then `count + 1` 16-byte nodes (the first
//! is the root): `u32 search, u16 left, u16 right, i32 name, i32 data`, the last two relative to
//! their own field. An FTEX starts with its magic and a GX2 surface descriptor (dimension, width,
//! height, depth, mip count, format, AA, use, image size, image pointer, mip size, mip pointer,
//! tile mode, swizzle, alignment, pitch, 13 mip offsets), then the view (first mip, mip count,
//! first slice, slice count), the four channel selectors, 5 registers, a handle, the array
//! length, and relative offsets to the name, the path, the image data and the mip data.

use crate::bflim::{FlimFormat, IDENTITY, Image, decode_surface};
use crate::error::{Error, Result};

fn bad(detail: impl Into<String>) -> Error {
    Error::Format {
        what: "BFRES (Wii U)",
        detail: detail.into(),
    }
}

/// One FTEX: the GX2 surface fields needed to decode mip 0.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Texture {
    pub name: String,
    pub width: u32,
    pub height: u32,
    /// `GX2SurfaceFormat`.
    pub format: u32,
    pub tile_mode: u32,
    pub swizzle: u32,
    /// Channel selectors (GX2: 0 R, 1 G, 2 B, 3 A, 4 zero, 5 one).
    pub comp_sel: [u8; 4],
    /// Mip 0, still tiled.
    pub data: Vec<u8>,
}

/// GX2 surface format → the BFLIM format with the same element layout.
fn flim_format(gx2: u32) -> Option<FlimFormat> {
    use FlimFormat::*;
    Some(match gx2 {
        0x001 => L8,
        0x007 => La8,
        0x008 => Rgb565,
        0x00a => Rgb5a1,
        0x00b => Rgba4,
        0x01a => Rgba8,
        0x41a => Rgba8Srgb,
        0x031 => Bc1,
        0x431 => Bc1Srgb,
        0x032 => Bc2,
        0x432 => Bc2Srgb,
        0x033 => Bc3,
        0x433 => Bc3Srgb,
        0x034 => Bc4L,
        0x035 => Bc5La,
        _ => return None,
    })
}

impl Texture {
    /// Untile and convert mip 0. The channel selectors are applied through
    /// `Image::channel_select` (BC4/BC5 keep their luminance mapping when the selectors are the
    /// identity).
    pub fn decode(&self) -> Result<Image> {
        let fmt = flim_format(self.format)
            .ok_or_else(|| bad(format!("{}: GX2 format {:#x}", self.name, self.format)))?;
        let mut img = decode_surface(
            &self.data,
            self.width,
            self.height,
            fmt,
            self.tile_mode,
            self.swizzle,
        )?;
        if self.comp_sel != [0, 1, 2, 3] {
            // GX2 selector → .mtex selector (0 zero, 1 one, 2 R, 3 G, 4 B, 5 A)
            let sel = self.comp_sel.map(|c| match c {
                0..=3 => c + 2,
                4 => 0,
                _ => 1,
            });
            img.channel_select = if img.channel_select == IDENTITY {
                sel
            } else {
                // compose: output channel i reads the decoded channel `sel[i]`
                sel.map(|s| match s {
                    2..=5 => img.channel_select[(s - 2) as usize],
                    v => v,
                })
            };
        }
        Ok(img)
    }
}

/// A model (FMDL): its bones and shapes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Model {
    pub name: String,
    pub bones: Vec<Bone>,
    pub shapes: Vec<Shape>,
}

/// A bone of the model's skeleton (FSKL), with its rest pose relative to the parent.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Bone {
    pub name: String,
    /// −1 for the root.
    pub parent: i16,
    pub scale: [f32; 3],
    /// Quaternion (x, y, z, w) or Euler angles, as stored.
    pub rotate: [f32; 4],
    pub translate: [f32; 3],
}

/// A shape (FSHP): one LOD's triangles of one vertex buffer, bound to one bone, drawn with one
/// material.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Shape {
    pub name: String,
    pub bone: u16,
    pub material: Material,
    pub positions: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
}

/// What of a material (FMAT) a flat model needs: its first texture, the `tex_srt0` shader
/// parameter (mode, scale x, scale y, rotation, translate x, translate y) and the first
/// sampler's wrap modes (GX2 clamp: 0 wrap, 1 mirror, 2 clamp, …).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Material {
    pub name: String,
    pub texture: Option<String>,
    pub tex_srt: Option<[f32; 6]>,
    pub wrap: [u8; 2],
}

/// A texture-pattern animation (FTXP): for each animated material, the texture index over the
/// frames; indices point into `textures`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TexPattern {
    pub name: String,
    pub frames: i32,
    pub looping: bool,
    pub textures: Vec<String>,
    pub materials: Vec<PatternTrack>,
}

/// One material's pattern: a constant `base` index, or step keys (frame, index).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PatternTrack {
    pub material: String,
    pub base: u32,
    pub keys: Vec<(f32, u32)>,
}

impl PatternTrack {
    /// The texture index at `frame` (step keys: the last key at or before it).
    pub fn at(&self, frame: f32) -> u32 {
        self.keys
            .iter()
            .take_while(|k| k.0 <= frame)
            .last()
            .map_or(self.base, |k| k.1)
    }
}

struct R<'a>(&'a [u8]);

impl R<'_> {
    fn u32(&self, o: usize) -> Result<u32> {
        self.0
            .get(o..o + 4)
            .map(|b| u32::from_be_bytes(b.try_into().unwrap()))
            .ok_or_else(|| bad(format!("read past end at {o:#x}")))
    }
    fn u16(&self, o: usize) -> Result<u16> {
        self.0
            .get(o..o + 2)
            .map(|b| u16::from_be_bytes(b.try_into().unwrap()))
            .ok_or_else(|| bad(format!("read past end at {o:#x}")))
    }
    /// A self-relative offset (0 = none).
    fn rel(&self, o: usize) -> Result<Option<usize>> {
        let v = self.u32(o)? as i32;
        Ok((v != 0).then(|| (o as i64 + v as i64) as usize))
    }
    fn cstr(&self, o: usize) -> Result<String> {
        let tail = self.0.get(o..).ok_or_else(|| bad("string outside file"))?;
        let n = tail.iter().position(|&b| b == 0).unwrap_or(tail.len());
        Ok(String::from_utf8_lossy(&tail[..n]).into_owned())
    }
    fn f32(&self, o: usize) -> Result<f32> {
        Ok(f32::from_bits(self.u32(o)?))
    }
    fn u8(&self, o: usize) -> Result<u8> {
        self.0
            .get(o)
            .copied()
            .ok_or_else(|| bad(format!("read past end at {o:#x}")))
    }
    /// A self-relative offset that must be there.
    fn to(&self, o: usize, what: &str) -> Result<usize> {
        self.rel(o)?
            .ok_or_else(|| bad(format!("no {what} at {o:#x}")))
    }
    /// The name at the self-relative offset `o` ("" when none).
    fn name(&self, o: usize) -> Result<String> {
        match self.rel(o)? {
            Some(p) => self.cstr(p),
            None => Ok(String::new()),
        }
    }
    /// An index group's entries: (name, data offset).
    fn group(&self, at: usize) -> Result<Vec<(String, usize)>> {
        let n = self.u32(at + 4)? as usize;
        let mut out = Vec::with_capacity(n);
        for i in 1..=n {
            let node = at + 8 + 16 * i;
            if let Some(d) = self.rel(node + 12)? {
                out.push((self.name(node + 8)?, d));
            }
        }
        Ok(out)
    }
    /// The index group of header slot `slot` (0 models, 1 textures, … 6 texture patterns).
    fn header_group(&self, slot: usize) -> Result<Vec<(String, usize)>> {
        match self.rel(0x20 + 4 * slot)? {
            Some(g) => self.group(g),
            None => Ok(Vec::new()),
        }
    }
    /// One vertex attribute value as floats (the GX2 formats the flat models use).
    fn attrib(&self, o: usize, format: u32) -> Result<Vec<f32>> {
        let half = |b: u16| half_to_f32(b);
        Ok(match format {
            0x806 => vec![self.f32(o)?],
            0x80d => vec![self.f32(o)?, self.f32(o + 4)?],
            0x811 => vec![self.f32(o)?, self.f32(o + 4)?, self.f32(o + 8)?],
            0x813 => (0..4).map(|k| self.f32(o + 4 * k)).collect::<Result<_>>()?,
            0x808 => vec![half(self.u16(o)?), half(self.u16(o + 2)?)],
            0x80f => (0..4)
                .map(|k| self.u16(o + 2 * k).map(half))
                .collect::<Result<_>>()?,
            0x004 => vec![self.u8(o)? as f32 / 255.0, self.u8(o + 1)? as f32 / 255.0],
            0x007 => vec![
                self.u16(o)? as f32 / 65535.0,
                self.u16(o + 2)? as f32 / 65535.0,
            ],
            0x20a | 0x207 => vec![
                (self.u16(o)? as i16 as f32 / 32767.0).max(-1.0),
                (self.u16(o + 2)? as i16 as f32 / 32767.0).max(-1.0),
            ],
            f => return Err(bad(format!("vertex format {f:#x}"))),
        })
    }
}

/// IEEE half → f32.
fn half_to_f32(h: u16) -> f32 {
    let s = ((h >> 15) & 1) as u32;
    let e = ((h >> 10) & 0x1f) as i32;
    let m = (h & 0x3ff) as u32;
    let v = if e == 0 {
        (m as f32) * 2f32.powi(-24)
    } else if e == 31 {
        if m == 0 { f32::INFINITY } else { f32::NAN }
    } else {
        (1.0 + m as f32 / 1024.0) * 2f32.powi(e - 15)
    };
    if s == 1 { -v } else { v }
}

fn check_fres(file: &[u8]) -> Result<()> {
    if file.get(..4) != Some(b"FRES") {
        return Err(bad("missing FRES magic"));
    }
    if file.get(4) != Some(&3) {
        return Err(bad(format!("unsupported version {:?}", file.get(4..8))));
    }
    Ok(())
}

/// The models of a Wii U BFRES (FRES 3.x): bones, and each shape's first LOD as triangles with
/// positions and the first UV set (layouts: FMDL `+0x0C` FSKL, `+0x10` FVTX array, `+0x14` FSHP
/// group, `+0x18` FMAT group; FSKL `+0x10` bone group; bone `+0x06` parent, `+0x14` scale,
/// `+0x20` rotation, `+0x30` translation; FVTX (0x20 bytes) `+0x04` attribute / buffer counts,
/// `+0x10` attributes (12 bytes: name, buffer, offset, format), `+0x18` buffers (0x18 bytes:
/// size, stride `+0x0C`, data `+0x14`); FSHP `+0x0E` material, `+0x10` bone, `+0x12` vertex
/// buffer, `+0x24` LODs (primitive, index format, count, …, index buffer `+0x14`, first vertex
/// `+0x18`); FMAT `+0x10` texture / sampler / parameter counts, `+0x28` texture references (name,
/// FTEX), `+0x2C` samplers (GX2 sampler words), `+0x34` shader parameters (0x14 bytes: type,
/// size, data offset, …, name `+0x10`), `+0x3C` their data) [obs: the SMM1 files decode this
/// way; the public v3 descriptions].
pub fn models(file: &[u8]) -> Result<Vec<Model>> {
    check_fres(file)?;
    let r = R(file);
    let mut out = Vec::new();
    for (name, fm) in r.header_group(0)? {
        if file.get(fm..fm + 4) != Some(b"FMDL") {
            return Err(bad(format!("{name}: not an FMDL")));
        }
        let mut model = Model {
            name,
            ..Default::default()
        };
        if let Some(fskl) = r.rel(fm + 0x0C)?
            && let Some(bg) = r.rel(fskl + 0x10)?
        {
            for (bname, bo) in r.group(bg)? {
                let v = |k: usize| r.f32(bo + 0x14 + 4 * k);
                model.bones.push(Bone {
                    name: bname,
                    parent: r.u16(bo + 6)? as i16,
                    scale: [v(0)?, v(1)?, v(2)?],
                    rotate: [v(3)?, v(4)?, v(5)?, v(6)?],
                    translate: [v(7)?, v(8)?, v(9)?],
                });
            }
        }
        let mats = match r.rel(fm + 0x18)? {
            Some(g) => r.group(g)?,
            None => Vec::new(),
        };
        let mut materials = Vec::new();
        for (mname, mo) in &mats {
            materials.push(material(&r, mname, *mo)?);
        }
        let fvtx = r.to(fm + 0x10, "vertex buffers")?;
        let shapes = match r.rel(fm + 0x14)? {
            Some(g) => r.group(g)?,
            None => Vec::new(),
        };
        for (sname, so) in shapes {
            let mat = r.u16(so + 0x0E)? as usize;
            let bone = r.u16(so + 0x10)?;
            let vb = fvtx + 0x20 * r.u16(so + 0x12)? as usize;
            let (na, nb) = (r.u8(vb + 4)? as usize, r.u8(vb + 5)? as usize);
            let nv = r.u32(vb + 8)? as usize;
            let attrs = r.to(vb + 0x10, "attributes")?;
            let bufs = r.to(vb + 0x18, "buffers")?;
            let mut buffers = Vec::with_capacity(nb);
            for b in 0..nb {
                let bo = bufs + 0x18 * b;
                buffers.push((r.to(bo + 0x14, "buffer data")?, r.u16(bo + 0x0C)? as usize));
            }
            let mut shape = Shape {
                name: sname,
                bone,
                material: materials.get(mat).cloned().unwrap_or_default(),
                ..Default::default()
            };
            for a in 0..na {
                let ao = attrs + 12 * a;
                let aname = r.name(ao)?;
                if aname != "_p0" && aname != "_u0" {
                    continue;
                }
                let (bi, eo, fmt) = (
                    r.u8(ao + 4)? as usize,
                    r.u16(ao + 6)? as usize,
                    r.u32(ao + 8)?,
                );
                let &(data, stride) = buffers
                    .get(bi)
                    .ok_or_else(|| bad(format!("{}: buffer {bi}", shape.name)))?;
                for v in 0..nv {
                    let x = r.attrib(data + v * stride + eo, fmt)?;
                    if aname == "_p0" {
                        shape.positions.push([
                            x[0],
                            x.get(1).copied().unwrap_or(0.0),
                            x.get(2).copied().unwrap_or(0.0),
                        ]);
                    } else {
                        shape.uvs.push([x[0], x.get(1).copied().unwrap_or(0.0)]);
                    }
                }
            }
            let lod = r.to(so + 0x24, "LODs")?;
            let (prim, ifmt, count) = (r.u32(lod)?, r.u32(lod + 4)?, r.u32(lod + 8)? as usize);
            if prim != 4 {
                return Err(bad(format!("{}: primitive type {prim}", shape.name)));
            }
            let ib = r.to(lod + 0x14, "index buffer")?;
            let idata = r.to(ib + 0x14, "index data")?;
            let first = r.u32(lod + 0x18)?;
            for k in 0..count {
                let i = match ifmt {
                    0 | 4 => r.u16(idata + 2 * k)? as u32,
                    _ => r.u32(idata + 4 * k)?,
                };
                shape.indices.push(i + first);
            }
            model.shapes.push(shape);
        }
        out.push(model);
    }
    Ok(out)
}

fn material(r: &R<'_>, name: &str, mo: usize) -> Result<Material> {
    let mut m = Material {
        name: name.to_string(),
        ..Default::default()
    };
    let (ntex, nsamp) = (r.u8(mo + 0x10)?, r.u8(mo + 0x11)?);
    if ntex > 0
        && let Some(t) = r.rel(mo + 0x28)?
    {
        m.texture = Some(r.name(t)?);
    }
    if nsamp > 0
        && let Some(s) = r.rel(mo + 0x2C)?
    {
        let w = r.u32(s)?;
        m.wrap = [(w & 7) as u8, ((w >> 3) & 7) as u8];
    }
    let nparam = r.u16(mo + 0x12)? as usize;
    if let (Some(params), Some(data)) = (r.rel(mo + 0x34)?, r.rel(mo + 0x3C)?) {
        for i in 0..nparam {
            let e = params + 0x14 * i;
            if r.name(e + 0x10)? != "tex_srt0" {
                continue;
            }
            let (size, at) = (r.u8(e + 1)? as usize, r.u16(e + 2)? as usize);
            if size >= 24 {
                let v = |k: usize| r.f32(data + at + 4 * k);
                m.tex_srt = Some([v(0)?, v(1)?, v(2)?, v(3)?, v(4)?, v(5)?]);
            }
        }
    }
    Ok(m)
}

/// The texture-pattern animations (FTXP) of a Wii U BFRES (FTXP `+0x0C` flags (bit 2 loop),
/// `+0x10` frames, `+0x14` texture / material counts, `+0x2C` material animations (0x1C bytes:
/// pattern / curve counts, name `+0x0C`, pattern infos `+0x10` (8 bytes: curve index, …, name),
/// curves `+0x14` (0x24 bytes: flags (frame type bits 0–1, key type bits 2–3), key count, …,
/// offset `+0x14` as an integer, frames `+0x1C`, keys `+0x20`), base values `+0x18`), `+0x30` the
/// texture index group) [obs: the SMM1 files].
pub fn tex_patterns(file: &[u8]) -> Result<Vec<TexPattern>> {
    check_fres(file)?;
    let r = R(file);
    let mut out = Vec::new();
    for (name, to) in r.header_group(6)? {
        if file.get(to..to + 4) != Some(b"FTXP") {
            return Err(bad(format!("{name}: not an FTXP")));
        }
        let mut p = TexPattern {
            name,
            frames: r.u32(to + 0x10)? as i32,
            looping: r.u16(to + 0x0C)? & 4 != 0,
            ..Default::default()
        };
        if let Some(g) = r.rel(to + 0x30)? {
            p.textures = r.group(g)?.into_iter().map(|e| e.0).collect();
        }
        let nmat = r.u16(to + 0x16)? as usize;
        let mats = r.to(to + 0x2C, "material animations")?;
        for m in 0..nmat {
            let mo = mats + 0x1C * m;
            let npat = r.u16(mo)? as usize;
            let mut track = PatternTrack {
                material: r.name(mo + 0x0C)?,
                ..Default::default()
            };
            if npat == 0 {
                p.materials.push(track);
                continue;
            }
            let info = r.to(mo + 0x10, "pattern infos")?;
            if let Some(base) = r.rel(mo + 0x18)? {
                track.base = r.u16(base)? as u32;
            }
            let curve = r.u8(info)? as i8;
            if curve >= 0 {
                let co = r.to(mo + 0x14, "curves")? + 0x24 * curve as usize;
                let flags = r.u16(co)?;
                let nk = r.u16(co + 2)? as usize;
                let offset = r.u32(co + 0x14)? as i32;
                let (fa, ka) = (r.to(co + 0x1C, "frames")?, r.to(co + 0x20, "keys")?);
                for k in 0..nk {
                    let f = match flags & 3 {
                        0 => r.f32(fa + 4 * k)?,
                        1 => r.u16(fa + 2 * k)? as i16 as f32 / 32.0,
                        _ => r.u8(fa + k)? as f32,
                    };
                    let v = match (flags >> 2) & 3 {
                        0 => r.u32(ka + 4 * k)? as i32,
                        1 => r.u16(ka + 2 * k)? as i16 as i32,
                        _ => r.u8(ka + k)? as i8 as i32,
                    };
                    track.keys.push((f, (v + offset).max(0) as u32));
                }
            }
            p.materials.push(track);
        }
        out.push(p);
    }
    Ok(out)
}

/// The textures of a Wii U BFRES, in index-group order.
pub fn textures(file: &[u8]) -> Result<Vec<Texture>> {
    let r = R(file);
    if file.get(..4) != Some(b"FRES") {
        return Err(bad("missing FRES magic"));
    }
    if file.get(4) != Some(&3) {
        return Err(bad(format!("unsupported version {:?}", file.get(4..8))));
    }
    let count = r.u16(0x50 + 2)? as usize;
    let Some(group) = r.rel(0x20 + 4)? else {
        return Ok(Vec::new());
    };
    let n = (r.u32(group + 4)? as usize).min(count.max(r.u32(group + 4)? as usize));
    let mut out = Vec::with_capacity(n);
    for i in 1..=n {
        let node = group + 8 + 16 * i;
        let Some(o) = r.rel(node + 12)? else {
            continue;
        };
        if file.get(o..o + 4) != Some(b"FTEX") {
            return Err(bad(format!("entry {i}: not an FTEX")));
        }
        let f = |k: usize| r.u32(o + 4 + 4 * k);
        let (width, height, format) = (f(1)?, f(2)?, f(5)?);
        let (size, tile_mode, swizzle) = (f(8)? as usize, f(12)?, f(13)?);
        let comp = file
            .get(o + 0x88..o + 0x8c)
            .ok_or_else(|| bad("truncated FTEX"))?;
        let name = match r.rel(o + 0xA8)? {
            Some(p) => r.cstr(p)?,
            None => match r.rel(node + 8)? {
                Some(p) => r.cstr(p)?,
                None => format!("texture{i}"),
            },
        };
        let data_at = r
            .rel(o + 0xB0)?
            .ok_or_else(|| bad(format!("{name}: no image data")))?;
        let data = file
            .get(data_at..data_at + size)
            .ok_or_else(|| bad(format!("{name}: image data outside file")))?
            .to_vec();
        out.push(Texture {
            name,
            width,
            height,
            format,
            tile_mode,
            swizzle,
            comp_sel: [comp[0], comp[1], comp[2], comp[3]],
            data,
        });
    }
    Ok(out)
}

/// Build a texture-only Wii U BFRES (tests and synthetic fixtures): `(name, width, height,
/// RGBA8 pixels)` per texture, stored linear-aligned (tile mode 1).
pub fn build_rgba(textures: &[(&str, u32, u32, Vec<u8>)]) -> Vec<u8> {
    let mut f = vec![0u8; 0x6C];
    f[..4].copy_from_slice(b"FRES");
    f[4..8].copy_from_slice(&[3, 5, 0, 3]);
    f[8..10].copy_from_slice(&[0xFE, 0xFF]);
    f[10..12].copy_from_slice(&0x10u16.to_be_bytes());
    f[0x52..0x54].copy_from_slice(&(textures.len() as u16).to_be_bytes());
    let group = f.len();
    let put = |f: &mut Vec<u8>, at: usize, v: u32| f[at..at + 4].copy_from_slice(&v.to_be_bytes());
    let rel = |at: usize, to: usize| (to as i64 - at as i64) as i32 as u32;
    f.resize(group + 8 + 16 * (textures.len() + 1), 0);
    put(&mut f, 0x24, rel(0x24, group));
    put(&mut f, group, (8 + 16 * (textures.len() + 1)) as u32);
    put(&mut f, group + 4, textures.len() as u32);
    for (i, (name, w, h, px)) in textures.iter().enumerate() {
        let px =
            crate::gx2::tile(px, *w, *h, 32, crate::gx2::TileMode::LinearAligned, 0).expect("tile");
        let o = f.len();
        f.resize(o + 0xC0, 0);
        f[o..o + 4].copy_from_slice(b"FTEX");
        let fields = [
            1,
            *w,
            *h,
            1,
            1,
            0x1a,
            0,
            1,
            px.len() as u32,
            0,
            0,
            0,
            1,
            0,
            1,
            *w,
        ];
        for (k, v) in fields.iter().enumerate() {
            put(&mut f, o + 4 + 4 * k, *v);
        }
        f[o + 0x88..o + 0x8c].copy_from_slice(&[0, 1, 2, 3]);
        let name_at = f.len();
        f.extend_from_slice(name.as_bytes());
        f.push(0);
        while !f.len().is_multiple_of(0x100) {
            f.push(0);
        }
        let data_at = f.len();
        f.extend_from_slice(&px);
        put(&mut f, o + 0xA8, rel(o + 0xA8, name_at));
        put(&mut f, o + 0xB0, rel(o + 0xB0, data_at));
        let node = group + 8 + 16 * (i + 1);
        put(&mut f, node + 8, rel(node + 8, name_at));
        put(&mut f, node + 12, rel(node + 12, o));
    }
    let len = f.len() as u32;
    put(&mut f, 0x0C, len);
    f
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synthetic_textures_round_trip() {
        let px: Vec<u8> = (0..8 * 4 * 4).map(|i| (i * 7 % 251) as u8).collect();
        let file = build_rgba(&[("wait.0", 8, 4, px.clone()), ("walk.1", 8, 4, px.clone())]);
        let t = textures(&file).unwrap();
        assert_eq!(t.len(), 2);
        assert_eq!(t[0].name, "wait.0");
        assert_eq!(t[1].name, "walk.1");
        assert_eq!((t[1].width, t[1].height, t[1].format), (8, 4, 0x1a));
        let img = t[0].decode().unwrap();
        assert_eq!(img.data, px);
    }

    #[test]
    fn rejects_other_files() {
        assert!(textures(b"FLIM....").is_err());
        let mut f = build_rgba(&[]);
        f[4] = 4;
        assert!(textures(&f).is_err());
        assert!(textures(&build_rgba(&[])).unwrap().is_empty());
    }

    #[test]
    fn selectors_compose() {
        let px = vec![10, 20, 30, 40];
        let mut t = textures(&build_rgba(&[("a", 1, 1, px)])).unwrap().remove(0);
        t.comp_sel = [0, 0, 0, 5];
        assert_eq!(t.decode().unwrap().channel_select, [2, 2, 2, 1]);
    }
}
