//! Environment stage: the per-theme lights and fogs of the 3D styles (`Env/Cmn.szs` for the
//! play field, `Env/Cmn_DV.szs` for the background scenes) → `env/<name>.json`.
//!
//! Each `.baglenv` (AAMP) holds directional, hemisphere and ambient lights and fogs. The names
//! are the files' base names: `plain`, `castle_n` (NSMBU field), `plain3w` (3D World field),
//! `dv_plain`, `dv_plain_d` (NSMBU background, day / night), `dv_plain3w` (3D World background);
//! the style's SceneDB names the set a theme uses (`Env_Model`, `Env_DV`).
//!
//! Each environment also gets its light map's `Diffuse` map (`.bagllmap`, the same name's or the
//! folder's common `cmn*.bagllmap` [inf]) as `lightmap`: the lights it is made of, named, with
//! their `effect` (a scale) and `pow` (the exponent on the light's LUT index), and its rim.

use std::path::Path;

use mmk_formats::aamp::{self, List, Object};
use mmk_formats::archive::{self, Sarc};
use serde_json::{Value, json};

use crate::{Result, read, write};

const ARCHIVES: &[&str] = &["Env/Cmn.szs", "Env/Cmn_DV.szs"];

fn rgba(o: &Object, k: &str) -> Value {
    json!(o.vec(k).map(|v| v.to_vec()).unwrap_or_else(|| vec![0.0; 4]))
}

fn objects<'a>(root: &'a List, list: &str) -> impl Iterator<Item = &'a Object> {
    root.list(list).into_iter().flat_map(|l| l.objects.iter())
}

/// One environment file → JSON (lights and fogs).
pub fn env_json(root: &List) -> Value {
    let common = |o: &Object| {
        json!({
            "name": o.str("name").unwrap_or(""),
            "enable": o.bool("enable").unwrap_or(true),
        })
    };
    let with = |mut base: Value, extra: Value| {
        if let (Some(b), Some(e)) = (base.as_object_mut(), extra.as_object()) {
            for (k, v) in e {
                b.insert(k.clone(), v.clone());
            }
        }
        base
    };
    let directional: Vec<Value> = objects(root, "DirectionalLight")
        .map(|o| {
            with(
                common(o),
                json!({
                    "diffuse": rgba(o, "DiffuseColor"),
                    "specular": rgba(o, "SpecularColor"),
                    "backside": rgba(o, "BacksideColor"),
                    "intensity": o.f32("Intensity").unwrap_or(1.0),
                    "direction": o.vec("Direction").map(|v| v.to_vec()).unwrap_or(vec![0.0, -1.0, 0.0]),
                    "view": o.bool("ViewCoordinate").unwrap_or(false),
                }),
            )
        })
        .collect();
    let hemisphere: Vec<Value> = objects(root, "HemisphereLight")
        .map(|o| {
            with(
                common(o),
                json!({
                    "sky": rgba(o, "SkyColor"),
                    "ground": rgba(o, "GroundColor"),
                    "intensity": o.f32("Intensity").unwrap_or(1.0),
                    "direction": o.vec("Direction").map(|v| v.to_vec()).unwrap_or(vec![0.0, 1.0, 0.0]),
                }),
            )
        })
        .collect();
    let ambient: Vec<Value> = objects(root, "AmbientLight")
        .map(|o| {
            with(
                common(o),
                json!({
                    "color": rgba(o, "Color"),
                    "intensity": o.f32("Intensity").unwrap_or(1.0),
                }),
            )
        })
        .collect();
    let fog: Vec<Value> = objects(root, "Fog")
        .map(|o| {
            with(
                common(o),
                json!({
                    "start": o.f32("Start").unwrap_or(0.0),
                    "end": o.f32("End").unwrap_or(1.0),
                    "damp": o.f32("Damp").unwrap_or(1.0),
                    "color": rgba(o, "Color"),
                    "direction": o.vec("Direction").map(|v| v.to_vec()).unwrap_or(vec![0.0, 0.0, -1.0]),
                }),
            )
        })
        .collect();
    json!({
        "directional": directional,
        "hemisphere": hemisphere,
        "ambient": ambient,
        "fog": fog,
    })
}

/// The `Diffuse` map of a light-map file (`.bagllmap`) → JSON: `lights` (type, name, effect,
/// pow of each environment object it is made of; an empty name refers to none) and `rim`.
pub fn lightmap_json(root: &List) -> Option<Value> {
    let is_diffuse = |o: &Object| o.str("name") == Some("Diffuse");
    let map = root
        .lists
        .iter()
        .find(|l| l.objects.iter().any(is_diffuse))?;
    let head = map.objects.iter().find(|o| is_diffuse(o))?;
    let lights: Vec<Value> = map
        .list("env_obj_ref_array")
        .into_iter()
        .flat_map(|l| l.objects.iter())
        .map(|o| {
            json!({
                "type": o.str("type").unwrap_or(""),
                "name": o.str("name").unwrap_or(""),
                "effect": o.f32("effect").unwrap_or(1.0),
                "pow": o.f32("pow").unwrap_or(1.0),
            })
        })
        .collect();
    Some(json!({
        "lights": lights,
        "rim": {
            "enable": head.bool("rim_enable").unwrap_or(false),
            "light": head.str("rim_light_ref").unwrap_or(""),
            "effect": head.f32("rim_effect").unwrap_or(0.0),
            "width": head.f32("rim_width").unwrap_or(1.0),
            "angle": head.f32("rim_angle").unwrap_or(1.0),
            "pow": head.f32("rim_pow").unwrap_or(1.0),
        },
    }))
}

/// The light map an environment file uses: `<dir>/<base>.bagllmap`, else the folder's common
/// one (`cmn.bagllmap`, `cmn3w.bagllmap`) [inf: the env sets name no light map; the per-theme
/// files carry the theme's name].
fn lightmap_for<'a>(files: &'a [(String, Vec<u8>)], env: &str) -> Option<&'a [u8]> {
    let stem = env.strip_suffix(".baglenv")?;
    let dir = stem.rsplit_once('/').map_or("", |(d, _)| d);
    let own = format!("{stem}.bagllmap");
    files
        .iter()
        .find(|(n, _)| *n == own)
        .or_else(|| {
            files.iter().find(|(n, _)| {
                let (d, f) = n.rsplit_once('/').unwrap_or(("", n));
                d == dir && f.starts_with("cmn") && f.ends_with(".bagllmap")
            })
        })
        .map(|(_, d)| d.as_slice())
}

/// Write every `.baglenv` of the environment archives; returns how many.
pub fn export(romfs: &Path, out: &Path) -> Result<usize> {
    let mut count = 0;
    for rel in ARCHIVES {
        let path = romfs.join(rel);
        let Ok(data) = read(&path) else { continue };
        let outer = Sarc::parse(archive::unwrap(data).map_err(crate::fmt_err(&path))?)
            .map_err(crate::fmt_err(&path))?;
        for (_, genvb) in outer.files() {
            let Ok(inner) = Sarc::parse(genvb.to_vec()) else {
                continue;
            };
            let files: Vec<(String, Vec<u8>)> = inner
                .files()
                .map(|(n, f)| (n.to_string(), f.to_vec()))
                .collect();
            for (name, file) in &files {
                let Some(base) = name.strip_suffix(".baglenv") else {
                    continue;
                };
                let base = base.rsplit('/').next().unwrap_or(base);
                let Ok((_, root)) = aamp::parse(file) else {
                    continue;
                };
                let mut env = env_json(&root);
                if let Some(map) = lightmap_for(&files, name)
                    .and_then(|d| aamp::parse(d).ok())
                    .and_then(|(_, r)| lightmap_json(&r))
                {
                    env["lightmap"] = map;
                }
                write(
                    &out.join(format!("env/{base}.json")),
                    serde_json::to_string(&env).unwrap().as_bytes(),
                )?;
                count += 1;
            }
        }
    }
    Ok(count)
}
