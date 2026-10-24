//! SMM1 UI sounds for the SMM1 layout screens: which sound each layout's (and each part's)
//! events play, as the game's own SLink database says, and the sounds themselves
//! (research/specs/smm1-sound.md).
//!
//! - `Pack/Static.pack` → `SLink2/SLink2DB.szs` (`mmk_formats::xlink_wiiu`, XLNK v0x15): one
//!   user per layout and per part pane, `UI_<layout>[_<part>[_<part>…]]` (the parts of a nested
//!   path joined with `_`), each with event keys (`Decide`, `In`, `Out`, `TouchSelect`,
//!   `Countdown1`, `CouseInWalk100`, …) → a sound or a container choosing among its children
//!   (switch on a property such as the game style or the number of Marios, blend, random), and
//!   action triggers: a state of the game's UI object (`State::cState_Walk`,
//!   `State::c1stDemoAnimB`, …) starts a key at a frame.
//! - `Pack/Sound.pack` → `Sound/Resource/<bank>.bars` (`mmk_formats::audio::bars_wiiu`: SARCs of
//!   AMTA + BFWAV): the waves, by name; a sound whose AMTA marks it a stream is
//!   `Sound/Resource/Stream/<name>.bfstm`. Both become 16-bit WAV (BFWAV / BFSTM → BWAV → PCM,
//!   the costume import's path).
//!
//! Output, next to the layouts: `ui/sound/sounds.json` and `ui/sound/<sound>.wav`.
//!
//! ```text
//! { "game": "SMM1",
//!   "users": { "UI_<layout>[_<part>]": {
//!       "keys": { "<key>": {"sound": "SE_…", "volume": 0.5, "delay": 32},
//!                 "<container key>": {"kind": "switch", "watch": "…", "children": [
//!                     {"key": "…", "if": {"cmp": ">=", "int": 80}, "sound": …}, …]} },
//!       "actions": { "State::cState_Walk": [{"key": "walk", "frame": 0, "end": 300}] } } },
//!   "sounds": ["SE_…", …] }
//! ```
//! A child's `if` is its switch condition (`enum` / `int` / `float` value), `weight` its random
//! weight. Action names are the database's, cut at 23 bytes.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use mmk_formats::archive::{self, Sarc};
use mmk_formats::audio::bars_wiiu::{self, SoundKind};
use mmk_formats::audio::{Bwav, Fwav, fstm};
use mmk_formats::xlink::{Call, CondValue, Condition, ContainerKind, Param};
use mmk_formats::xlink_wiiu::XlnkWiiU;
use serde_json::{Map, Value, json};

use crate::smm1::Overlay;
use crate::smm2_ui_sound::wav;
use crate::{Progress, Result, read, write};

/// The sound packs, searched in this order for a wave.
const PACKS: [&str; 2] = ["Pack/Sound.pack", "Pack/UI_Boot_Sound.pack"];

#[derive(Clone, Debug, Default)]
pub struct UiSoundReport {
    pub users: usize,
    pub sounds: usize,
    /// Sounds read from a stream (`.bfstm`).
    pub streams: usize,
    pub skipped: Vec<String>,
}

fn cmp_name(c: mmk_formats::xlink::Compare) -> &'static str {
    use mmk_formats::xlink::Compare::*;
    match c {
        Eq => "==",
        Gt => ">",
        Ge => ">=",
        Lt => "<",
        Le => "<=",
        Ne => "!=",
    }
}

fn kind_name(k: ContainerKind) -> String {
    match k {
        ContainerKind::Switch => "switch".into(),
        ContainerKind::Random => "random".into(),
        ContainerKind::Random2 => "random2".into(),
        ContainerKind::Blend => "blend".into(),
        ContainerKind::Sequence => "sequence".into(),
        ContainerKind::Other(o) => format!("kind{o}"),
    }
}

fn num(p: Option<&Param>) -> Option<f32> {
    p.and_then(Param::as_f32)
}

/// Call `i` as JSON: an asset (sound, volume, delay), or a container with its children.
fn node_json(calls: &[Call], i: usize, depth: usize) -> Option<Value> {
    let c = &calls[i];
    if !c.container {
        let sound = c.params.get("RuntimeAssetName")?.as_str()?;
        if sound.is_empty() {
            return None;
        }
        let mut v = json!({ "sound": sound });
        if let Some(vol) = num(c.params.get("Volume")).filter(|v| *v != 1.0) {
            v["volume"] = json!(vol);
        }
        if let Some(d) = num(c.params.get("Delay")).filter(|d| *d > 0.0) {
            v["delay"] = json!(d);
        }
        return Some(v);
    }
    if depth > 8 {
        return None;
    }
    let (a, b) = c.children?;
    let mut children = Vec::new();
    for k in a..=b {
        if calls[k].parent != Some(i) {
            continue;
        }
        let Some(mut child) = node_json(calls, k, depth + 1) else {
            continue;
        };
        child["key"] = json!(calls[k].key);
        match &calls[k].condition {
            Some(Condition::Switch { cmp, value, .. }) => {
                let mut w = json!({ "cmp": cmp_name(*cmp) });
                match value {
                    CondValue::Enum(s) => w["enum"] = json!(s),
                    CondValue::Int(n) => w["int"] = json!(n),
                    CondValue::Float(f) => w["float"] = json!(f),
                }
                child["if"] = w;
            }
            Some(Condition::Random { weight }) => child["weight"] = json!(weight),
            _ => {}
        }
        children.push(child);
    }
    if children.is_empty() {
        return None;
    }
    let mut v = json!({ "kind": kind_name(c.kind?), "children": children });
    if let Some((w, _)) = &c.watch {
        v["watch"] = json!(w);
    }
    Some(v)
}

fn collect_sounds(v: &Value, out: &mut BTreeSet<String>) {
    if let Some(s) = v["sound"].as_str() {
        out.insert(s.to_string());
    }
    for c in v["children"].as_array().into_iter().flatten() {
        collect_sounds(c, out);
    }
}

/// User `name` as JSON (None when it has nothing to play).
fn user_json(x: &XlnkWiiU, name: &str, sounds: &mut BTreeSet<String>) -> Option<Value> {
    let calls = x.user_calls(name).ok()??;
    let mut keys = Map::new();
    for (i, c) in calls.iter().enumerate() {
        if c.parent.is_none()
            && let Some(v) = node_json(&calls, i, 0)
        {
            collect_sounds(&v, sounds);
            keys.insert(c.key.clone(), v);
        }
    }
    if keys.is_empty() {
        return None;
    }
    let mut actions: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    for t in x.user_actions(name).ok().flatten().unwrap_or_default() {
        let key = &calls[t.call].key;
        if !keys.contains_key(key) {
            continue;
        }
        let mut v = json!({ "key": key, "frame": t.start_frame });
        if let Some(e) = t.end_frame {
            v["end"] = json!(e);
        }
        actions.entry(t.action.clone()).or_default().push(v);
    }
    let mut u = json!({ "keys": keys });
    if !actions.is_empty() {
        u["actions"] = json!(actions);
    }
    Some(u)
}

/// The database's bytes (`SLink2DB.bslnk`), update first.
fn database(ov: &Overlay) -> Option<Vec<u8>> {
    let szs = ov.pack_member("Pack/Static.pack", "SLink2/SLink2DB.szs")?;
    let sarc = Sarc::parse(szs).ok()?;
    sarc.files()
        .find(|(n, _)| n.ends_with(".bslnk"))
        .map(|(_, d)| d.to_vec())
}

/// A wave (BFWAV, or a BFSTM read the same way) as 16-bit WAV (at most two channels).
fn to_wav(w: &Fwav) -> std::result::Result<Vec<u8>, mmk_formats::Error> {
    let bwav = w.to_bwav();
    let b = Bwav::parse(&bwav)?;
    let chans = (0..b.channels.len().min(2))
        .map(|c| b.decode_channel(c))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(wav(b.sample_rate(), &chans))
}

/// Import the SMM1 UI sounds into `<ui>/sound/`. A dump without the database imports nothing
/// and says so in `skipped`.
pub(crate) fn import_ui_sounds(
    ov: &Overlay,
    ui: &Path,
    progress: Progress,
) -> Result<UiSoundReport> {
    let mut report = UiSoundReport::default();
    let Some(db) = database(ov) else {
        report
            .skipped
            .push("Pack/Static.pack: no SLink2/SLink2DB.szs".into());
        return Ok(report);
    };
    let x = match XlnkWiiU::parse(&db) {
        Ok(x) => x,
        Err(e) => {
            report.skipped.push(format!("SLink2DB: {e}"));
            return Ok(report);
        }
    };
    let names: Vec<&str> = x.user_names().filter(|n| n.starts_with("UI_")).collect();
    let mut users = Map::new();
    let mut wanted = BTreeSet::new();
    for (i, n) in names.iter().enumerate() {
        progress("ui sounds", i, names.len());
        if let Some(u) = user_json(&x, n, &mut wanted) {
            users.insert(n.to_string(), u);
        }
    }
    progress("ui sounds", names.len(), names.len());
    wanted.remove("@Blank");
    report.users = users.len();
    // the waves, from the banks (update first, then the base game's packs)
    let mut written = BTreeSet::new();
    let total = wanted.len();
    for pack in PACKS {
        for path in ov.files(pack) {
            let Ok(sarc) = read(&path).and_then(|d| {
                archive::unwrap(d)
                    .and_then(Sarc::parse)
                    .map_err(crate::fmt_err(&path))
            }) else {
                report.skipped.push(format!("{pack}: unreadable"));
                continue;
            };
            for (member, bank) in sarc.files() {
                if !member.starts_with("Sound/Resource/") || !member.ends_with(".bars") {
                    continue;
                }
                let Ok(bank) = archive::unwrap(bank.to_vec()).and_then(Sarc::parse) else {
                    continue;
                };
                for s in bars_wiiu::bank_sounds(&bank) {
                    if !wanted.contains(&s.name) || written.contains(&s.name) {
                        continue;
                    }
                    // a stream's bank entry holds only its AMTA: read below from Stream/
                    let Some(wave) = s.wave else { continue };
                    if s.amta.as_ref().is_some_and(|a| a.kind == SoundKind::Stream) {
                        continue;
                    }
                    match Fwav::parse(wave).and_then(|w| to_wav(&w)) {
                        Ok(b) => {
                            write(&ui.join(format!("sound/{}.wav", s.name)), &b)?;
                            written.insert(s.name);
                            report.sounds += 1;
                            progress("ui waves", written.len(), total);
                        }
                        Err(e) => report.skipped.push(format!("{member}/{}: {e}", s.name)),
                    }
                }
            }
        }
    }
    // streamed sounds (and any wanted sound no bank held)
    let left: Vec<String> = wanted.difference(&written).cloned().collect();
    for name in &left {
        let rel = format!("Sound/Resource/Stream/{name}.bfstm");
        let Some(path) = ov.file(&rel) else {
            report.skipped.push(format!("sound {name}: not found"));
            continue;
        };
        match read(&path).and_then(|d| {
            fstm::parse(&d)
                .and_then(|w| to_wav(&w))
                .map_err(crate::fmt_err(&path))
        }) {
            Ok(b) => {
                write(&ui.join(format!("sound/{name}.wav")), &b)?;
                written.insert(name.clone());
                report.sounds += 1;
                report.streams += 1;
            }
            Err(e) => report.skipped.push(format!("{rel}: {e}")),
        }
    }
    progress("ui waves", total, total);
    let doc = json!({ "game": "SMM1", "users": users, "sounds": written });
    write(
        &ui.join("sound/sounds.json"),
        serde_json::to_string(&doc).unwrap_or_default().as_bytes(),
    )?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::smm1::Smm1Config;
    use crate::smm1::tests::sarc;

    /// The database's users and keys → `sounds.json`; the bank's waves → WAV files; a sound no
    /// bank or stream holds is listed as skipped.
    #[test]
    fn imports_a_synthetic_database_and_bank() {
        let tmp = std::env::temp_dir().join(format!("mmk-smm1-snd-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        let base = tmp.join("content");
        fs::create_dir_all(base.join("Pack")).unwrap();
        let db = mmk_formats::fixtures::sample_xlnk_wiiu();
        let szs = sarc(&[("SLink2DB.bslnk", &db)]);
        fs::write(
            base.join("Pack/Static.pack"),
            sarc(&[("SLink2/SLink2DB.szs", &szs)]),
        )
        .unwrap();
        let amta = bars_wiiu::Amta::build("SE_Ok", 0, 1, 32000);
        let wave = mmk_formats::fixtures::sample_fwav_pcm(&[100, -100, 200]);
        let bank = sarc(&[("SE_Ok.bameta", &amta), ("SE_Ok.bfwav", &wave)]);
        fs::write(
            base.join("Pack/Sound.pack"),
            sarc(&[("Sound/Resource/UI_L.bars", &bank)]),
        )
        .unwrap();
        let ov = Overlay::new(&Smm1Config {
            base: base.clone(),
            update: None,
            out: tmp.join("out"),
            language: "USen".into(),
        });
        let ui = tmp.join("out/ui");
        let r = import_ui_sounds(&ov, &ui, &mut |_, _, _| {}).unwrap();
        assert_eq!((r.users, r.sounds), (1, 1));
        assert!(r.skipped.iter().any(|s| s.contains("SE_Many")));
        let doc: Value =
            serde_json::from_slice(&fs::read(ui.join("sound/sounds.json")).unwrap()).unwrap();
        assert_eq!(doc["game"], "SMM1");
        let u = &doc["users"]["UI_L"];
        assert_eq!(u["keys"]["Decide"]["sound"], "SE_Ok");
        let sw = &u["keys"]["Walk"]["children"][0];
        assert_eq!(u["keys"]["Walk"]["kind"], "blend");
        assert_eq!(sw["kind"], "switch");
        assert_eq!(sw["children"][0]["if"], json!({"cmp": ">=", "int": 50}));
        assert_eq!(sw["children"][1]["volume"], 0.5);
        assert_eq!(
            u["actions"]["State::cState_Walk"],
            json!([{"key": "Walk", "frame": 5}])
        );
        let w = fs::read(ui.join("sound/SE_Ok.wav")).unwrap();
        assert_eq!(&w[0..4], b"RIFF");
        assert_eq!(&w[44..50], &[100, 0, 0x9c, 0xff, 200, 0]);
        let _ = fs::remove_dir_all(&tmp);
    }
}
