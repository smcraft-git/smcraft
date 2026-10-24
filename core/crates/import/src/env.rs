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
//!
//! And its colour correction (`.baglccr`, `agl::pfx::ColorCorrection`: the 8³ colour table the
//! game's last pass looks every pixel up in) as `color_correction`: the same name's, else the
//! folder's `ef_normal*.baglccr` [obs: 3D World's snow field, which has none, draws with
//! `ef_normal3w`'s saturation 1.125 — the table's parameters read from the recomp's draw of it].
//!
//! 3D World draws with its environment palette (`Palette/Palette3W.benvpal`) instead of the
//! `.baglenv` files' lights and fogs [obs: the recomp's environment uniform blocks of the 3D World
//! ground, snow, water and ghost house courses hold the palette's values, not the files']: per
//! theme a `SceneCommon_Palette_<n>` (light directions), a `DV_Palette_<n>_<v>` (the background:
//! main / fill light, hemisphere, four fogs) and a `MAIN_Palette_<n>_<v>` (the play field's
//! lights). Those values replace the lights and fogs of the theme's `<theme>3w` / `dv_<theme>3w`.

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

/// A colour-correction file (`.baglccr`) → JSON: hue, saturation, brightness, gamma (every
/// SMM2 table has hue 0, brightness 1, gamma 1 and identity `toycam_*` / `level` settings; only
/// the saturation differs: 3D World 0.85 … 1.2, NSMBU 1).
pub fn ccr_json(root: &List) -> Option<Value> {
    let o = root.object("color_correction")?;
    Some(json!({
        "enable": o.bool("enable").unwrap_or(true),
        "hue": o.f32("hue").unwrap_or(0.0),
        "saturation": o.f32("saturation").unwrap_or(1.0),
        "brightness": o.f32("brightness").unwrap_or(1.0),
        "gamma": o.f32("gamma").unwrap_or(1.0),
    }))
}

/// The colour correction an environment file goes with: `<dir>/<base>.baglccr`, else the
/// folder's `ef_normal*.baglccr`.
fn ccr_for<'a>(files: &'a [(String, Vec<u8>)], env: &str) -> Option<&'a [u8]> {
    let stem = env.strip_suffix(".baglenv")?;
    let dir = stem.rsplit_once('/').map_or("", |(d, _)| d);
    let own = format!("{stem}.baglccr");
    files
        .iter()
        .find(|(n, _)| *n == own)
        .or_else(|| {
            files.iter().find(|(n, _)| {
                let (d, f) = n.rsplit_once('/').unwrap_or(("", n));
                d == dir && f.starts_with("ef_normal") && f.ends_with(".baglccr")
            })
        })
        .map(|(_, d)| d.as_slice())
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

/// The palette's themes in its order (palette `n` = 2 × this index; the odd `n` repeat them with
/// a few fogs changed [?: which courses use them]).
const PALETTE_THEMES: [&str; 10] = [
    "plain",
    "underground",
    "water",
    "hauntedhouse",
    "castle",
    "airship",
    "snow",
    "desert",
    "athletic",
    "woods",
];

/// The palette variant a course is played with (`_0` / `_1` differ in some fogs and the hemisphere
/// [?: the editor's]) [obs: the played courses' environment blocks match `_2`].
const PALETTE_VARIANT: usize = 2;

/// A palette direction (azimuth, elevation in radians) as the direction the light travels.
fn palette_dir(a: &[f32]) -> [f32; 3] {
    let (az, el) = (
        a.first().copied().unwrap_or(0.0),
        a.get(1).copied().unwrap_or(0.0),
    );
    [-el.cos() * az.sin(), -el.sin(), -el.cos() * az.cos()]
}

/// Patch one environment JSON with a palette object's lights (`p` the key prefix, `DV` or `MAIN`)
/// and, for the background, its fogs.
fn patch_from_palette(env: &mut Value, pal: &Object, common: Option<&Object>, p: &str) {
    let rgba = |k: &str| {
        json!(
            pal.vec(&format!("{p}_{k}"))
                .map(|v| v.to_vec())
                .unwrap_or_else(|| vec![0.0; 4])
        )
    };
    let num = |k: &str, d: f32| pal.f32(&format!("{p}_{k}")).unwrap_or(d);
    let dir = |k: &str, d: [f32; 3]| {
        common
            .and_then(|c| c.vec(&format!("{p}_{k}")))
            .map_or(d, palette_dir)
            .to_vec()
    };
    let light = |name: &str, pre: &str, dir: Vec<f32>| {
        json!({
            "name": name,
            "enable": true,
            "diffuse": rgba(&format!("{pre}DifColor")),
            "specular": rgba(&format!("{pre}SpeColor")),
            "backside": rgba(&format!("{pre}BackColor")),
            "intensity": num(&format!("{pre}Intencity"), 1.0),
            "direction": dir,
            "view": false,
        })
    };
    let mut dirs: Vec<Value> = vec![
        light("Main", "", dir("Dir", [0.0, -1.0, 0.0])),
        light("Fill", "Fill", dir("FillDir", [0.0, 1.0, 0.0])),
    ];
    // the files' other directional lights (3D World's "Depth") stay
    if let Some(old) = env["directional"].as_array() {
        dirs.extend(
            old.iter()
                .filter(|o| !matches!(o["name"].as_str(), Some("Main" | "Fill")))
                .cloned(),
        );
    }
    env["directional"] = json!(dirs);
    env["hemisphere"] = json!([{
        "name": "Hemi",
        "enable": true,
        "sky": rgba("SkyColor"),
        "ground": rgba("GrdColor"),
        "intensity": num("BgIntencity", 1.0),
        "direction": [0.0, 1.0, 0.0],
    }]);
    if p == "DV" {
        let fog = |name: &str, dir: [f32; 3]| {
            json!({
                "name": name,
                "enable": true,
                "start": num(&format!("{name}Start"), 0.0),
                "end": num(&format!("{name}End"), 1.0),
                "damp": 1.0,
                "color": rgba(&format!("{name}Color")),
                "direction": dir,
            })
        };
        // `DV_FogPat` 1 leaves the second fogs out [obs: 3D World snow (1) has no FogZ1 / FogY1
        // in its environment block, ground (4), underground and forest (0) have the palette's]
        let mut fogs = vec![fog("FogZ", [0.0, 0.0, -1.0]), fog("FogY", [0.0, -1.0, 0.0])];
        if num("FogPat", 0.0) != 1.0 {
            fogs.push(fog("FogZ1", [0.0, 0.0, -1.0]));
            fogs.push(fog("FogY1", [0.0, -1.0, 0.0]));
        }
        env["fog"] = json!(fogs);
    }
    env["palette"] = json!(true);
}

/// Apply 3D World's environment palette to the written `env/*.json`; returns how many themes.
fn apply_palette(romfs: &Path, out: &Path) -> Result<usize> {
    let Ok(data) = read(&romfs.join("Palette/Palette3W.benvpal")) else {
        return Ok(0);
    };
    let Ok((_, root)) = aamp::parse(&data) else {
        return Ok(0);
    };
    let mut count = 0;
    for (k, theme) in PALETTE_THEMES.iter().enumerate() {
        let n = 2 * k;
        let common = root.object(&format!("SceneCommon_Palette_{n}"));
        for (file, p) in [
            (format!("dv_{theme}3w"), "DV"),
            (format!("{theme}3w"), "MAIN"),
        ] {
            let Some(pal) = root.object(&format!("{p}_Palette_{n}_{PALETTE_VARIANT}")) else {
                continue;
            };
            let path = out.join(format!("env/{file}.json"));
            let Some(mut env) = std::fs::read(&path)
                .ok()
                .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
            else {
                continue;
            };
            patch_from_palette(&mut env, pal, common, p);
            write(&path, serde_json::to_string(&env).unwrap().as_bytes())?;
        }
        count += 1;
    }
    Ok(count)
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
                if let Some(ccr) = ccr_for(&files, name)
                    .and_then(|d| aamp::parse(d).ok())
                    .and_then(|(_, r)| ccr_json(&r))
                {
                    env["color_correction"] = ccr;
                }
                write(
                    &out.join(format!("env/{base}.json")),
                    serde_json::to_string(&env).unwrap().as_bytes(),
                )?;
                count += 1;
            }
        }
    }
    apply_palette(romfs, out)?;
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_directions_point_where_the_light_travels() {
        // azimuth 30°, elevation 60°: down and away, a little to the left (3D World's ground
        // field light, as the game's environment block holds it)
        let d = palette_dir(&[30f32.to_radians(), 60f32.to_radians()]);
        for (a, b) in d.iter().zip([-0.25, -0.866_025, -0.433_013]) {
            assert!((a - b).abs() < 1e-5, "{d:?}");
        }
        // elevation −15°, azimuth 90°: sideways and a little up
        let d = palette_dir(&[90f32.to_radians(), (-15f32).to_radians()]);
        assert!(
            (d[0] + 0.965_926).abs() < 1e-5 && (d[1] - 0.258_819).abs() < 1e-5,
            "{d:?}"
        );
    }
}
