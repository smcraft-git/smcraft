//! SMM1 ending import (importer 5): what the 100 Mario rescue scene (`NetEndingDemoScene`,
//! research/specs/smm1-ending.md §2) draws in the 2D styles — the stage (`field_model`), Peach
//! and Toad from `Model/<S>_Object_peach.szs`, and the demo Mario's player models with the
//! texture patterns he uses there (`run`, `wait`, the shoe's and the cape's) — baked into the UI
//! import as flat triangles, the way the Super World map is (DECISIONS.md "Super World map: flat
//! quads in the UI pack"): every model of these archives is camera-facing quads [obs data].
//!
//! Output (UI import):
//!
//! ```text
//! ui/ending/ending.json   { "version", "archives": { "<archive>": {
//!                             "models": { "<model>": [ { "material", "texture", "wrap": [s, t],
//!                                         "v": [[x, y, z, u, v] × 3 per triangle] } ] },
//!                             "patterns": { "<anim>": { "frames", "loop", "textures": [...],
//!                                         "tracks": { "<material>": { "base", "keys": [[f, i]] } } } } } } }
//! ui/textures/<archive>/<texture>.mtex
//! ```
//! Vertices are model space with the shapes' bones applied (scale, rotation, translation down
//! the parent chain) and UVs through the material's `tex_srt0` (the Maya-mode matrix).

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use mmk_formats::archive::{self, Sarc};
use mmk_formats::bfres_wiiu::{self, Bone, Material, Model};
use serde_json::{Map, Value, json};

use crate::smm1::{Overlay, to_mtex};
use crate::{Progress, Result, write};

/// Version of `ending.json`.
pub const ENDING_VERSION: u32 = 1;

/// The archives of `Pack/StaticSkin.pack` the scene uses, by style: the stage with Peach and
/// Toad, then the player models of the forms SMM1 has in that style (`M1`: small, Super, Fire,
/// the Big Mushroom's, Lanky (Weird Mushroom), the costume rig; `M3`: + the Super Leaf's; `MW`: +
/// the cape over Super Mario), and the style's shared player animations (`CommonAnm`: their
/// patterns name the textures of whichever player model plays them) [obs: the archives' names;
/// which form uses which, research smm1-extras-*.md].
pub const ARCHIVES: [(&str, &[&str]); 3] = [
    (
        "M1",
        &[
            "M1_Object_peach",
            "M1_Player_MarioMdl",
            "M1_Player_SpMarioMdl",
            "M1_Player_FireMarioMdl",
            "M1_Player_MegaMarioMdl",
            "M1_Player_SlimMarioMdl",
            "M1_Player_CharaMdl",
            "M1_Player_CommonAnm",
        ],
    ),
    (
        "M3",
        &[
            "M3_Object_peach",
            "M3_Player_MarioMdl",
            "M3_Player_SpMarioMdl",
            "M3_Player_FireMarioMdl",
            "M3_Player_TnkMarioMdl",
            "M3_Player_CommonAnm",
        ],
    ),
    (
        "MW",
        &[
            "MW_Object_peach",
            "MW_Player_MarioMdl",
            "MW_Player_SpMarioMdl",
            "MW_Player_FireMarioMdl",
            "MW_Player_Mant",
            "MW_Player_CommonAnm",
        ],
    ),
];

/// The demo Mario's texture patterns (0x025ee7e8, 0x025eed54, 0x025eeebc): the walk (`run`),
/// standing (`wait`), the Kuribo's Shoe hops, the cape's [obs]. The stage archives keep all.
const PLAYER_PATTERNS: [&str; 8] = [
    "run",
    "wait",
    "shoes_jump",
    "shoes_jumpup",
    "shoes_wait",
    "mt_walk",
    "mt_down",
    "mt_wait",
];

#[derive(Clone, Debug, Default)]
pub struct EndingReport {
    pub archives: usize,
    pub textures: usize,
    pub skipped: Vec<String>,
}

/// Rotate `v` by the quaternion `q` (x, y, z, w).
fn rotate(q: [f32; 4], v: [f32; 3]) -> [f32; 3] {
    let [x, y, z, w] = q;
    let u = [x, y, z];
    let cross = |a: [f32; 3], b: [f32; 3]| {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    };
    let t = cross(u, v).map(|c| c * 2.0);
    let c = cross(u, t);
    [
        v[0] + w * t[0] + c[0],
        v[1] + w * t[1] + c[1],
        v[2] + w * t[2] + c[2],
    ]
}

/// A point of bone `b`'s space in model space (its rest pose and its parents').
fn to_model(bones: &[Bone], mut b: usize, mut p: [f32; 3]) -> [f32; 3] {
    for _ in 0..bones.len() {
        let Some(bone) = bones.get(b) else { break };
        let s = [
            p[0] * bone.scale[0],
            p[1] * bone.scale[1],
            p[2] * bone.scale[2],
        ];
        let r = if bone.rotate == [0.0, 0.0, 0.0, 1.0] {
            s
        } else {
            rotate(bone.rotate, s)
        };
        p = [
            r[0] + bone.translate[0],
            r[1] + bone.translate[1],
            r[2] + bone.translate[2],
        ];
        if bone.parent < 0 {
            break;
        }
        b = bone.parent as usize;
    }
    p
}

/// A UV through `tex_srt0` (mode, scale, rotation, translation; the Maya-mode matrix).
fn tex_srt(m: &Material, uv: [f32; 2]) -> [f32; 2] {
    let Some([_, sx, sy, r, tx, ty]) = m.tex_srt else {
        return uv;
    };
    let (sn, cs) = r.sin_cos();
    let m02 = sx * ((-0.5 * cs - 0.5 * sn + 0.5) - tx);
    let m12 = sy * ((0.5 * sn - 0.5 * cs + 0.5) + ty);
    [
        sx * cs * uv[0] + sx * sn * uv[1] + m02,
        -sy * sn * uv[0] + sy * cs * uv[1] + m12,
    ]
}

/// One model's shapes as triangles.
pub fn bake_model(m: &Model) -> Value {
    let mut shapes = Vec::new();
    for s in &m.shapes {
        let mut v = Vec::with_capacity(s.indices.len());
        for &i in &s.indices {
            let i = i as usize;
            let (Some(&p), Some(&uv)) = (s.positions.get(i), s.uvs.get(i)) else {
                continue;
            };
            let p = to_model(&m.bones, s.bone as usize, p);
            let uv = tex_srt(&s.material, uv);
            v.push(json!([p[0], p[1], p[2], uv[0], uv[1]]));
        }
        shapes.push(json!({
            "material": s.material.name,
            "texture": s.material.texture,
            "wrap": s.material.wrap,
            "v": v,
        }));
    }
    Value::Array(shapes)
}

fn sarc_member(data: Vec<u8>, name: &str) -> Option<Vec<u8>> {
    let sarc = Sarc::parse(archive::unwrap(data).ok()?).ok()?;
    let found = sarc
        .files()
        .find(|(n, d)| n.ends_with(name) || d.starts_with(b"FRES"))
        .map(|(_, d)| d.to_vec())?;
    archive::unwrap(found).ok()
}

/// Import the ending's models into `ui` (`<out>/ui`).
pub(crate) fn import_ending(ov: &Overlay, ui: &Path, progress: Progress) -> Result<EndingReport> {
    let mut report = EndingReport::default();
    let mut archives = Map::new();
    // the textures the styles' shared animations name (a model without its own patterns plays
    // those with its own textures)
    let mut common: BTreeMap<&str, BTreeSet<String>> = BTreeMap::new();
    for (style, names) in ARCHIVES {
        for name in names.iter().filter(|n| n.ends_with("_CommonAnm")) {
            let pats = ov
                .pack_member("Pack/StaticSkin.pack", &format!("Model/{name}.szs"))
                .and_then(|szs| sarc_member(szs, ".bfres"))
                .and_then(|f| bfres_wiiu::tex_patterns(&f).ok())
                .unwrap_or_default();
            let set = common.entry(style).or_default();
            for p in pats
                .iter()
                .filter(|p| PLAYER_PATTERNS.contains(&p.name.as_str()))
            {
                set.extend(p.textures.iter().cloned());
            }
        }
    }
    let total: usize = ARCHIVES.iter().map(|a| a.1.len()).sum();
    let mut done = 0;
    for (style, names) in ARCHIVES {
        for name in names {
            progress("ending", done, total);
            done += 1;
            let Some(szs) = ov.pack_member("Pack/StaticSkin.pack", &format!("Model/{name}.szs"))
            else {
                report
                    .skipped
                    .push(format!("StaticSkin.pack/Model/{name}.szs: not found"));
                continue;
            };
            let Some(fres) = sarc_member(szs, ".bfres") else {
                report.skipped.push(format!("{name}: no BFRES"));
                continue;
            };
            let parsed = bfres_wiiu::models(&fres)
                .and_then(|m| Ok((m, bfres_wiiu::tex_patterns(&fres)?)))
                .and_then(|(m, p)| Ok((m, p, bfres_wiiu::textures(&fres)?)));
            let (models, patterns, textures) = match parsed {
                Ok(x) => x,
                Err(e) => {
                    report.skipped.push(format!("{name}: {e}"));
                    continue;
                }
            };
            let stage = name.ends_with("_Object_peach");
            let mut wanted: BTreeSet<String> = if stage {
                BTreeSet::new()
            } else {
                common.get(style).cloned().unwrap_or_default()
            };
            let mut out_models = Map::new();
            for m in &models {
                for s in &m.shapes {
                    if let Some(t) = &s.material.texture {
                        wanted.insert(t.clone());
                    }
                }
                out_models.insert(m.name.clone(), bake_model(m));
            }
            let mut out_patterns = Map::new();
            for p in &patterns {
                if !stage && !PLAYER_PATTERNS.contains(&p.name.as_str()) {
                    continue;
                }
                wanted.extend(p.textures.iter().cloned());
                let tracks: BTreeMap<String, Value> = p
                    .materials
                    .iter()
                    .map(|t| {
                        (
                            t.material.clone(),
                            json!({ "base": t.base, "keys": t.keys }),
                        )
                    })
                    .collect();
                out_patterns.insert(
                    p.name.clone(),
                    json!({
                        "frames": p.frames, "loop": p.looping, "textures": p.textures,
                        "tracks": tracks,
                    }),
                );
            }
            for t in &textures {
                if !wanted.contains(&t.name) || t.name.contains(['/', '\\']) {
                    continue;
                }
                match t.decode() {
                    Ok(img) => {
                        write(
                            &ui.join(format!("textures/{name}/{}.mtex", t.name)),
                            &to_mtex(&img).to_bytes(),
                        )?;
                        report.textures += 1;
                    }
                    Err(e) => report.skipped.push(format!("{name}/{}: {e}", t.name)),
                }
            }
            archives.insert(
                name.to_string(),
                json!({ "models": out_models, "patterns": out_patterns }),
            );
            report.archives += 1;
        }
    }
    progress("ending", total, total);
    write(
        &ui.join("ending/ending.json"),
        serde_json::to_string(&json!({ "version": ENDING_VERSION, "archives": archives }))
            .unwrap()
            .as_bytes(),
    )?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The owner's dump (skipped without it): the scenes' stages, Peach and Toad, the player
    /// models, as the binary places them (smm1-ending.md §2.3).
    #[test]
    fn owner_dump_ending_models() {
        let home = std::env::var_os("HOME")
            .map(std::path::PathBuf::from)
            .unwrap_or_default();
        let base = home.join("smm-work/extracted/smm1-disc/GM000500001018DC00000000000100/content");
        let upd = home.join("smm-work/extracted/smm1-update-v208/content");
        if !base.is_dir() {
            eprintln!("SMM1 dump not present; skipping");
            return;
        }
        let ov = Overlay::new(&crate::smm1::Smm1Config {
            base,
            update: upd.is_dir().then_some(upd),
            out: Default::default(),
            language: "USen".into(),
        });
        let tmp = std::env::temp_dir().join(format!("mmk-ending-{}", std::process::id()));
        let r = import_ending(&ov, &tmp, &mut |_, _, _| {}).unwrap();
        assert!(r.skipped.is_empty(), "{:?}", r.skipped);
        assert_eq!(r.archives, 20);
        let doc: Value =
            serde_json::from_slice(&std::fs::read(tmp.join("ending/ending.json")).unwrap())
                .unwrap();
        let m1 = &doc["archives"]["M1_Object_peach"];
        // the ground: 2 quads of 384 × 48 with the field texture 12 times across each
        let body = &m1["models"]["field_model"][0];
        assert_eq!(body["texture"], "field");
        assert_eq!(body["v"].as_array().unwrap().len(), 12);
        let xs: Vec<f64> = body["v"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v[0].as_f64().unwrap())
            .collect();
        assert_eq!(xs.iter().cloned().fold(f64::MAX, f64::min), 0.0);
        assert_eq!(xs.iter().cloned().fold(0.0, f64::max), 768.0);
        let us: Vec<f64> = body["v"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v[3].as_f64().unwrap())
            .collect();
        assert_eq!(us.iter().cloned().fold(0.0, f64::max), 12.0);
        // MW's Peach blinks
        let keys = &m1["patterns"]["wait_peach"]["tracks"]["M1_body"]["keys"];
        assert!(keys.as_array().unwrap().is_empty(), "M1's Peach does not");
        let mw = &doc["archives"]["MW_Object_peach"]["patterns"]["wait_peach"];
        assert_eq!(mw["textures"], json!(["wait.0", "wait.1"]));
        // the small Mario's run: walk.0 / .1 / .2, five frames each
        let run = &doc["archives"]["M1_Player_MarioMdl"]["patterns"]["run"];
        assert_eq!(run["frames"], 15);
        assert_eq!(run["textures"], json!(["walk.0", "walk.1", "walk.2"]));
        assert!(
            tmp.join("textures/M1_Player_MarioMdl/walk.1.mtex")
                .is_file()
        );
        assert!(
            tmp.join("textures/M1_Object_peach/M1_DV_sky.mtex")
                .is_file()
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn bones_and_texture_srt_bake_into_the_vertices() {
        let bones = vec![
            Bone {
                name: "body".into(),
                parent: -1,
                scale: [1.0; 3],
                rotate: [0.0, 0.0, 0.0, 1.0],
                translate: [0.0, 0.0, 0.0],
            },
            Bone {
                name: "up".into(),
                parent: 0,
                scale: [12.0, 1.0, 1.0],
                rotate: [0.0, 0.0, 0.0, 1.0],
                translate: [0.0, 144.0, 0.0],
            },
        ];
        assert_eq!(to_model(&bones, 1, [64.0, 128.0, 0.0]), [768.0, 272.0, 0.0]);
        // a quarter turn about z
        let h = std::f32::consts::FRAC_1_SQRT_2;
        let r = rotate([0.0, 0.0, h, h], [1.0, 0.0, 0.0]);
        assert!((r[0]).abs() < 1e-6 && (r[1] - 1.0).abs() < 1e-6);
        let m = Material {
            tex_srt: Some([0.0, 12.0, 1.0, 0.0, 0.0, 0.0]),
            ..Default::default()
        };
        assert_eq!(tex_srt(&m, [1.0, 1.0]), [12.0, 1.0]);
        assert_eq!(tex_srt(&Material::default(), [0.5, 0.25]), [0.5, 0.25]);
    }
}
