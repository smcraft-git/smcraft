//! SMM2 UI import stage "myworld map": what the Super World map is drawn with (private spec
//! `research/specs/super-worlds.md` §7), into the UI cache so every front end draws it as layout
//! quads (DECISIONS.md "Super World map: flat quads in the UI pack").
//!
//! ```text
//! ui/textures/<Model>/<name>.mtex   every texture of every `Pack/MyWorld_Model.pack` model (the
//!                                   field atlases `MyWorld_Field_<skin>`, roads, points, the
//!                                   player models, the backdrops `MyWorld_DV_<skin>`, …)
//! ui/myworld/map.json               { version, models: {<Model>: model}, tables: {<Mush table>:
//!                                   rows} }
//! ```
//! The game's map models are flat textured quads at small depths (z 0 … 2) seen from straight
//! above [obs: every shape of the pack lies in one z plane]; a model here keeps per shape the
//! rest-pose vertices `[x, y, z, u, v]` in model units (world px), the triangle list, its
//! material and the bone it rides on, and per animation the texture pattern of each material,
//! the bones' skinning matrices (2D part, baked per frame where they change) and the bones'
//! visibility. The Mush tables are the theme rows (`SceneDB_MyWorld`) and the animation choice
//! tables (`WM_PlayerAnime`, `WM_PointPartsAnime`, `WM_GhostAnime`).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use mmk_assets::mesh::{Binding, MeshModel, Pose, SkelAnim};
use mmk_formats::archive::{self, Sarc};
use mmk_formats::bfres;
use mmk_formats::bntx;
use serde_json::{Map, Value, json};

use crate::{ImportError, Result, read, write};

/// The pack holding every map model (v3.0.0+).
pub const PACK: &str = "Pack/MyWorld_Model.pack";
/// Where the model description goes (relative to the UI cache root).
pub const MAP_FILE: &str = "ui/myworld/map.json";
/// Version of `map.json`'s form.
pub const MAP_VERSION: u32 = 1;
/// The Mush tables copied next to the models.
pub const TABLES: [&str; 4] = [
    "SceneDB_MyWorld",
    "WM_PlayerAnime",
    "WM_PointPartsAnime",
    "WM_GhostAnime",
];

/// What the stage wrote.
#[derive(Clone, Debug, Default)]
pub struct MyWorldReport {
    pub models: usize,
    pub textures: usize,
    pub tables: usize,
    pub skipped: Vec<String>,
}

fn fmt_err(path: &Path) -> impl Fn(mmk_formats::Error) -> ImportError + '_ {
    move |source| ImportError::Format {
        path: path.to_path_buf(),
        source,
    }
}

/// Model name from a member path (`Model/MyWorld_Point_normal.Nin_NX_NVN.zs`).
fn model_name(path: &str) -> String {
    let file = path.rsplit('/').next().unwrap_or(path);
    file.split('.').next().unwrap_or(file).to_string()
}

/// Round to 1/1000 (the JSON stays small; positions are whole or half px, matrices are smooth).
fn r3(v: f32) -> Value {
    let x = (v as f64 * 1000.0).round() / 1000.0;
    json!(if x == 0.0 { 0.0 } else { x })
}

/// The 2D part of a 3×4 affine: a, b (row x), c, d (row y), tx, ty.
fn affine2d(m: &bfres::Affine) -> [f32; 6] {
    [m[0][0], m[0][1], m[1][0], m[1][1], m[0][3], m[1][3]]
}

fn shapes_json(m: &MeshModel) -> Vec<Value> {
    m.shapes
        .iter()
        .map(|s| {
            let range = s.first_index as usize..(s.first_index + s.index_count) as usize;
            let idx = &m.indices[range];
            // the shape's own vertices, renumbered from 0
            let mut verts: Vec<u32> = idx.to_vec();
            verts.sort_unstable();
            verts.dedup();
            let local = |i: u32| verts.binary_search(&i).unwrap_or(0) as u32;
            let mut v = Vec::with_capacity(verts.len() * 5);
            let mut joints = Vec::with_capacity(verts.len());
            for &i in &verts {
                let vx = &m.vertices[i as usize];
                v.extend([
                    r3(vx.position[0]),
                    r3(vx.position[1]),
                    r3(vx.position[2]),
                    r3(vx.uv[0]),
                    r3(vx.uv[1]),
                ]);
                joints.push(vx.joints[0]);
            }
            let mat = m.materials.get(s.material);
            let mut o = json!({
                "mat": mat.map(|x| x.name.clone()).unwrap_or_default(),
                "tex": mat.and_then(|x| x.albedo.clone()),
                "blend": mat.map_or(0, |x| x.blend),
                "bone": s.bone,
                "v": v,
                "i": idx.iter().map(|&i| local(i)).collect::<Vec<_>>(),
            });
            // the albedo's texture transform (scale u, v, rotation, translate u, v) and wrap
            // modes (0 repeat, 1 mirror, 2 clamp), where they are not the plain ones
            if let Some(m) = mat {
                if let Some(srt) = m.param("tex_srt0").filter(|p| p.len() >= 6)
                    && srt[1..6] != [1.0, 1.0, 0.0, 0.0, 0.0]
                {
                    o["srt"] = json!(srt[1..6].iter().map(|&x| r3(x)).collect::<Vec<_>>());
                }
                if m.wrap != [0, 0] {
                    o["wrap"] = json!(m.wrap);
                }
            }
            // rigid shapes ride on one bone; list the joints only when they differ
            if joints.iter().any(|&j| j != joints[0]) {
                o["joints"] = json!(joints);
            } else {
                o["bone"] = json!(joints.first().copied().unwrap_or(0));
            }
            o
        })
        .collect()
}

/// One animation: texture patterns, baked skinning matrices of the bones that move, bone
/// visibility.
fn anim_json(m: &MeshModel, a: &SkelAnim) -> Value {
    let mut tex = Map::new();
    for t in &a.textures {
        tex.insert(
            t.material.clone(),
            json!(
                t.keys
                    .iter()
                    .map(|(f, n)| json!([f, n]))
                    .collect::<Vec<_>>()
            ),
        );
    }
    let mut o = json!({ "frames": a.frames, "loop": a.looping, "tex": tex });
    if !a.bones.is_empty() {
        let binding = Binding::new(m, a);
        let mut pose = Pose::default();
        let mut rest = Pose::default();
        m.pose(None, 0.0, &mut rest);
        let n = m.bones.len();
        let mut tracks: Vec<Vec<Value>> = vec![Vec::new(); n];
        let mut last: Vec<Option<[f32; 6]>> = vec![None; n];
        for f in 0..=a.frames.max(1) {
            m.pose(Some((a, &binding)), f as f32, &mut pose);
            for b in 0..n {
                let k = affine2d(&pose.skin[b]);
                let rounded = k.map(|x| (x * 1000.0).round() / 1000.0);
                if last[b] != Some(rounded) {
                    last[b] = Some(rounded);
                    let mut e = vec![json!(f)];
                    e.extend(k.iter().map(|&x| r3(x)));
                    tracks[b].push(Value::Array(e));
                }
            }
        }
        let identity = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
        let mut skin = Map::new();
        for (b, t) in tracks.into_iter().enumerate() {
            // a bone that stays at rest all along needs nothing
            let still = t.len() == 1 && last[b] == Some(identity);
            if !still {
                skin.insert(b.to_string(), Value::Array(t));
            }
        }
        if !skin.is_empty() {
            o["skin"] = Value::Object(skin);
        }
    }
    let mut vis = Map::new();
    for t in &a.visibility {
        let Some(b) = m.bones.iter().position(|x| x.name == t.bone) else {
            continue;
        };
        let mut keys = Vec::new();
        let mut prev = None;
        for f in 0..=a.frames.max(1) {
            let on = t.curve.as_ref().map_or(t.base, |c| c.eval(f as f32) >= 0.5);
            if prev != Some(on) {
                prev = Some(on);
                keys.push(json!([f, on as u8]));
            }
        }
        vis.insert(b.to_string(), Value::Array(keys));
    }
    if !vis.is_empty() {
        o["vis"] = Value::Object(vis);
    }
    o
}

/// The model description of one BFRES file (its first model; the map files hold one each).
pub fn model_json(f: &bfres::Bfres) -> Value {
    let Some(file) = crate::meshes::convert_file(f) else {
        // a texture-only file (the field atlases)
        return json!({ "bones": [], "shapes": [], "anims": {} });
    };
    let m = &file.models[0];
    let mut anims = Map::new();
    for a in crate::meshes::convert_anims(f) {
        anims.insert(a.name.clone(), anim_json(m, &a));
    }
    json!({
        "bones": m.bones.iter().map(|b| b.name.clone()).collect::<Vec<_>>(),
        "shapes": shapes_json(m),
        "anims": anims,
    })
}

/// The map's model files: the pack's members, plus loose `Model/MyWorld_*.zs` the pack lacks.
fn collect(romfs: &Path) -> Result<Vec<(String, Vec<u8>)>> {
    let mut out: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let pack = romfs.join(PACK);
    if crate::is_file(&pack) {
        let data = archive::unwrap(read(&pack)?).map_err(fmt_err(&pack))?;
        let sarc = Sarc::parse(data).map_err(fmt_err(&pack))?;
        for (name, d) in sarc.files() {
            if name.ends_with(".zs") {
                out.insert(model_name(name), d.to_vec());
            }
        }
    }
    if let Ok(paths) = crate::read_dir(&romfs.join("Model")) {
        for p in paths {
            let file = p
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let name = model_name(&file);
            if name.starts_with("MyWorld_") && file.ends_with(".zs") && !out.contains_key(&name) {
                out.insert(name, read(&p)?);
            }
        }
    }
    Ok(out.into_iter().collect())
}

/// The animation tables' column names, in English: scene skin, animation, kind (points,
/// ghosts), route (the player).
const COLUMNS: [(&str, &str); 4] = [
    ("\u{30b7}\u{30fc}\u{30f3}\u{30b9}\u{30ad}\u{30f3}", "skin"),
    ("\u{30a2}\u{30cb}\u{30e1}", "anim"),
    ("\u{7a2e}\u{985e}", "kind"),
    ("\u{30eb}\u{30fc}\u{30c8}", "route"),
];

/// A table's rows with the animation tables' columns renamed ([`COLUMNS`]).
fn rename_columns(v: Value) -> Value {
    match v {
        Value::Array(rows) => Value::Array(rows.into_iter().map(rename_columns).collect()),
        Value::Object(row) => Value::Object(
            row.into_iter()
                .map(|(k, v)| {
                    let k = COLUMNS
                        .iter()
                        .find(|(from, _)| *from == k)
                        .map_or(k, |(_, to)| to.to_string());
                    (k, v)
                })
                .collect(),
        ),
        other => other,
    }
}

/// The Mush tables of [`TABLES`] from `Pack/Static.pack`.
fn tables(romfs: &Path) -> Result<Map<String, Value>> {
    let mut out = Map::new();
    let path = romfs.join("Pack/Static.pack");
    if !crate::is_file(&path) {
        return Ok(out);
    }
    let sarc = Sarc::parse(archive::unwrap(read(&path)?).map_err(fmt_err(&path))?)
        .map_err(fmt_err(&path))?;
    for (name, data) in sarc.files() {
        let stem = model_name(name);
        if name.starts_with("Mush/") && TABLES.contains(&stem.as_str()) {
            let v = mmk_formats::byml::parse(data).map_err(fmt_err(&path.join(name)))?;
            out.insert(
                stem,
                rename_columns(serde_json::to_value(&v).unwrap_or(Value::Null)),
            );
        }
    }
    Ok(out)
}

/// Run the stage into the UI cache folder `out` (the folder holding `ui/`). Ok with zero models
/// for a dump without Super Worlds (before 3.0.0).
pub fn run(romfs: &Path, out: &Path) -> Result<MyWorldReport> {
    let mut report = MyWorldReport::default();
    let files = collect(romfs)?;
    let mut models = Map::new();
    for (name, data) in files {
        let ctx = PathBuf::from(format!("{PACK}/{name}"));
        let sarc = match archive::unwrap(data).and_then(Sarc::parse) {
            Ok(s) => s,
            Err(e) => {
                report.skipped.push(format!("{}: {e}", ctx.display()));
                continue;
            }
        };
        let mut model = None;
        for (_, member) in sarc.files() {
            if member.starts_with(b"FRES") {
                match bfres::parse(member) {
                    Ok(f) => model = Some(model_json(&f)),
                    Err(e) => report.skipped.push(format!("{}: {e}", ctx.display())),
                }
            }
            let Some(b) = bntx::find_embedded(member) else {
                continue;
            };
            let texs = match bntx::parse(b) {
                Ok(t) => t,
                Err(e) => {
                    report.skipped.push(format!("{}: {e}", ctx.display()));
                    continue;
                }
            };
            for t in &texs {
                let Some(m) = crate::smm2_ui::bntx_to_mtex(t) else {
                    report
                        .skipped
                        .push(format!("{name}/{}: format {:?}", t.name, t.format));
                    continue;
                };
                write(
                    &out.join(format!("ui/textures/{name}/{}.mtex", t.name)),
                    &m.to_bytes(),
                )?;
                report.textures += 1;
            }
        }
        if let Some(m) = model {
            models.insert(name, m);
            report.models += 1;
        }
    }
    if models.is_empty() {
        return Ok(report);
    }
    let tables = tables(romfs)?;
    report.tables = tables.len();
    let doc = json!({ "version": MAP_VERSION, "models": models, "tables": tables });
    write(
        &out.join(MAP_FILE),
        serde_json::to_string(&doc).unwrap().as_bytes(),
    )?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn affine_and_rounding() {
        let m: bfres::Affine = [
            [1.0, 0.0, 0.0, 5.0],
            [0.0, 2.0, 0.0, -3.0],
            [0.0, 0.0, 1.0, 9.0],
        ];
        assert_eq!(affine2d(&m), [1.0, 0.0, 0.0, 2.0, 5.0, -3.0]);
        assert_eq!(r3(0.12345), json!(0.123));
        assert_eq!(r3(-0.0001), json!(0.0));
        assert_eq!(
            model_name("Model/MyWorld_Point_normal.Nin_NX_NVN.zs"),
            "MyWorld_Point_normal"
        );
        let row = json!([{ COLUMNS[0].0: "plain", COLUMNS[1].0: "walk", "Other": 1 }]);
        assert_eq!(
            rename_columns(row),
            json!([{ "skin": "plain", "anim": "walk", "Other": 1 }])
        );
    }

    /// The owner's dump, when present: every map model converts, the field atlases come out as
    /// textures only, and the course point has its states.
    #[test]
    fn owner_dump_map_models() {
        let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
            return;
        };
        let romfs = home.join("smm-work/extracted/smm2-v303-romfs");
        if !romfs.join(PACK).is_file() {
            return;
        }
        let dir = std::env::temp_dir().join(format!("mmk-myworld-{}", std::process::id()));
        let r = run(&romfs, &dir).unwrap();
        assert!(r.models > 100, "{r:?}");
        assert_eq!(r.tables, 4);
        let doc: Value =
            serde_json::from_slice(&std::fs::read(dir.join(MAP_FILE)).unwrap()).unwrap();
        let point = &doc["models"]["MyWorld_Point_normal"];
        for state in ["wait", "tocleared", "cleared", "passable"] {
            assert!(point["anims"][state].is_object(), "{state}");
        }
        assert_eq!(doc["models"]["MyWorld_Field_plain"]["shapes"], json!([]));
        assert!(dir.join("ui/textures/MyWorld_Field_plain").is_dir());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
