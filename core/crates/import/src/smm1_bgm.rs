//! SMM1 menu music (importer 5): the streams the 100 Mario screens play (research
//! specs/smm1-100mario-screens.md §6.1, smm1-ending.md §2.6 / §4.6), as 16-bit WAVs next to the
//! UI sounds, each with its loop in a RIFF `smpl` chunk when the stream loops.
//!
//! - Course World's stream `BGM_NetworkMainScene` is six stereo tracks of which the code enables
//!   one (mask 1 << (value − 1), table 0x100d972b); the 100 Mario select uses 2 (Easy, Normal),
//!   3 (Expert) and 6 (Super Expert) [obs 0x02580cb0, 0x02581200]: each becomes
//!   `BGM_NetworkMainScene_<track>.wav`.
//! - The between-course maps' `BGM_100MarioMap[Expert|SuperExpert]` (0x0270864c), the staff
//!   rolls, game over's `M1_GameOver`, and the rescue scene's tunes by style (SLink
//!   `EndingPeachBGM` / `EndingKinopioBGM`): streams in `Sound/Resource/Stream/`, except SMW's
//!   egg-rescue tune, a wave of the `NetEndingDemoScene` bank [obs data].
//!
//! Output: `ui/sound/<name>.wav` (the same folder as the UI sounds: hosts play music cues from it),
//! and (importer 6) `ui/sound/bgm.json`: each tune's beat data from its AMTA — `{ "<name>": {
//! "rate", "loop": [start, end] | null, "marks": { "<marker>": sample } } }` (`beat:<n>`, `beatend`,
//! `loopstart`, `Koopa`, …: what the layouts' beat bounces run on, smm1-100mario-screens.md §3).

use std::path::Path;

use mmk_formats::archive::{self, Sarc};
use mmk_formats::audio::{Bwav, Fwav, bars_wiiu, fstm};
use serde_json::{Map, Value, json};

use crate::smm1::Overlay;
use crate::{Progress, Result, read, write};

/// Course World's stream and the tracks the select uses.
pub const COURSE_WORLD: &str = "BGM_NetworkMainScene";
pub const COURSE_WORLD_TRACKS: [usize; 3] = [2, 3, 6];

/// The other streams, by name (all stereo).
pub const STREAMS: [&str; 13] = [
    "BGM_100MarioMap",
    "BGM_100MarioMapExpert",
    "BGM_100MarioMapSuperExpert",
    "BGM_100mario_staffroll_easy",
    "BGM_100mario_staffroll_normal",
    "M1_GameOver",
    "M1_BGM_Ending",
    "M3_BGM_EndingDemo1",
    "M3_BGM_EndingDemo2",
    "MW_BGM_Ending2",
    "WU_STRM_BGM_LAST_BOSS2_HARP",
    "WU_STRM_BGM_LAST_BOSS_HARP_Dummy",
    "SuperMarioWorld_21_RescueEgg",
];

/// Where the waves that are not streams are (bank, sound).
const BANK_WAVES: [(&str, &str); 1] = [(
    "Sound/Resource/NetEndingDemoScene.bars",
    "SuperMarioWorld_21_RescueEgg",
)];

#[derive(Clone, Debug, Default)]
pub struct BgmReport {
    pub tracks: usize,
    pub skipped: Vec<String>,
}

/// A 16-bit PCM WAV of `channels` with the loop `[start, end)` (frames) in a `smpl` chunk.
pub fn wav_with_loop(rate: u32, channels: &[Vec<i16>], looping: Option<(u32, u32)>) -> Vec<u8> {
    let mut w = crate::smm2_ui_sound::wav(rate, channels);
    if let Some((start, end)) = looping {
        // smpl: manufacturer, product, sample period (ns), MIDI unity note, pitch fraction,
        // SMPTE format / offset, loop count, sampler data; one loop: id, type 0 (forward),
        // start, end (inclusive), fraction, play count 0 (forever)
        let mut c = Vec::with_capacity(60);
        let period = 1_000_000_000u32 / rate.max(1);
        for v in [
            0,
            0,
            period,
            60,
            0,
            0,
            0,
            1,
            0,
            0,
            0,
            start,
            end.saturating_sub(1),
            0,
            0,
        ] {
            c.extend_from_slice(&u32::to_le_bytes(v));
        }
        w.extend_from_slice(b"smpl");
        w.extend_from_slice(&(c.len() as u32).to_le_bytes());
        w.extend_from_slice(&c);
        let riff = (w.len() - 8) as u32;
        w[4..8].copy_from_slice(&riff.to_le_bytes());
    }
    w
}

/// Decode channels `pick` of `w` to 16-bit PCM.
fn decode(w: &Fwav, pick: &[usize]) -> std::result::Result<Vec<Vec<i16>>, mmk_formats::Error> {
    let mut sub = w.clone();
    sub.channels = pick
        .iter()
        .filter_map(|&c| w.channels.get(c).cloned())
        .collect();
    let bwav = sub.to_bwav();
    let b = Bwav::parse(&bwav)?;
    (0..b.channels.len()).map(|c| b.decode_channel(c)).collect()
}

/// The tunes' AMTAs (their markers), from every bank of `Pack/Sound.pack`.
fn tune_metas(ov: &Overlay) -> Vec<bars_wiiu::Amta> {
    let wanted: Vec<&str> = std::iter::once(COURSE_WORLD).chain(STREAMS).collect();
    let mut out: Vec<bars_wiiu::Amta> = Vec::new();
    for path in ov.files("Pack/Sound.pack") {
        let Some(sarc) = read(&path)
            .ok()
            .and_then(|d| archive::unwrap(d).ok())
            .and_then(|d| Sarc::parse(d).ok())
        else {
            continue;
        };
        for (_, bank) in sarc.files() {
            let Some(b) = archive::unwrap(bank.to_vec())
                .ok()
                .and_then(|d| Sarc::parse(d).ok())
            else {
                continue;
            };
            for s in bars_wiiu::bank_sounds(&b) {
                if let Some(a) = s.amta
                    && wanted.contains(&a.name.as_str())
                    && !out.iter().any(|o| o.name == a.name)
                {
                    out.push(a);
                }
            }
        }
        // the update's pack first: the base game's adds what it lacks
    }
    out
}

/// Import the menu music into `ui` (`<out>/ui`).
pub(crate) fn import_bgm(ov: &Overlay, ui: &Path, progress: Progress) -> Result<BgmReport> {
    let mut report = BgmReport::default();
    let mut beats = Map::new();
    let mut meta = |name: &str, tune: &str, w: &Fwav, metas: &[bars_wiiu::Amta]| {
        let marks: Map<String, Value> = metas
            .iter()
            .find(|a| a.name == tune)
            .map(|a| a.marks.iter().map(|(n, p)| (n.clone(), json!(p))).collect())
            .unwrap_or_default();
        beats.insert(
            name.to_string(),
            json!({ "rate": w.sample_rate, "loop": w.loop_range, "marks": marks }),
        );
    };
    let metas = tune_metas(ov);
    let total = STREAMS.len() + 1;
    let stream = |name: &str| -> Option<Fwav> {
        let path = ov.file(&format!("Sound/Resource/Stream/{name}.bfstm"))?;
        fstm::parse(&read(&path).ok()?).ok()
    };
    progress("menu music", 0, total);
    match stream(COURSE_WORLD) {
        Some(w) => {
            for t in COURSE_WORLD_TRACKS {
                let pick = [2 * (t - 1), 2 * (t - 1) + 1];
                if pick[1] >= w.channels.len() {
                    report.skipped.push(format!(
                        "{COURSE_WORLD}: no track {t} ({} channels)",
                        w.channels.len()
                    ));
                    continue;
                }
                meta(&format!("{COURSE_WORLD}_{t}"), COURSE_WORLD, &w, &metas);
                match decode(&w, &pick) {
                    Ok(ch) => {
                        write(
                            &ui.join(format!("sound/{COURSE_WORLD}_{t}.wav")),
                            &wav_with_loop(w.sample_rate, &ch, w.loop_range),
                        )?;
                        report.tracks += 1;
                    }
                    Err(e) => report
                        .skipped
                        .push(format!("{COURSE_WORLD} track {t}: {e}")),
                }
            }
        }
        None => report.skipped.push(format!("{COURSE_WORLD}: not found")),
    }
    for (i, name) in STREAMS.iter().enumerate() {
        progress("menu music", i + 1, total);
        let wave = stream(name).or_else(|| {
            let bank = BANK_WAVES.iter().find(|b| b.1 == *name)?.0;
            let data = ov.pack_member("Pack/Sound.pack", bank)?;
            let sarc = Sarc::parse(archive::unwrap(data).ok()?).ok()?;
            let s = bars_wiiu::bank_sounds(&sarc)
                .into_iter()
                .find(|s| s.name == *name)?;
            Fwav::parse(s.wave?).ok()
        });
        let Some(w) = wave else {
            report.skipped.push(format!("{name}: not found"));
            continue;
        };
        meta(name, name, &w, &metas);
        let pick: Vec<usize> = (0..w.channels.len().min(2)).collect();
        match decode(&w, &pick) {
            Ok(ch) => {
                write(
                    &ui.join(format!("sound/{name}.wav")),
                    &wav_with_loop(w.sample_rate, &ch, w.loop_range),
                )?;
                report.tracks += 1;
            }
            Err(e) => report.skipped.push(format!("{name}: {e}")),
        }
    }
    progress("menu music", total, total);
    write(
        &ui.join("sound/bgm.json"),
        serde_json::to_string(&Value::Object(beats))
            .unwrap()
            .as_bytes(),
    )?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_loop_goes_into_the_smpl_chunk() {
        let w = wav_with_loop(32000, &[vec![1, 2, 3, 4], vec![5, 6, 7, 8]], Some((1, 4)));
        let riff = u32::from_le_bytes(w[4..8].try_into().unwrap()) as usize;
        assert_eq!(riff + 8, w.len());
        let at = w.windows(4).position(|x| x == b"smpl").unwrap();
        let word =
            |k: usize| u32::from_le_bytes(w[at + 8 + 4 * k..at + 12 + 4 * k].try_into().unwrap());
        assert_eq!((word(7), word(11), word(12)), (1, 1, 3));
        assert!(
            !wav_with_loop(32000, &[vec![0; 4]], None)
                .windows(4)
                .any(|x| x == b"smpl")
        );
    }

    /// The owner's dump (skipped without it): every tune is there.
    #[test]
    fn owner_dump_menu_music() {
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
        let tmp = std::env::temp_dir().join(format!("mmk-bgm-{}", std::process::id()));
        let r = import_bgm(&ov, &tmp, &mut |_, _, _| {}).unwrap();
        assert!(r.skipped.is_empty(), "{:?}", r.skipped);
        assert_eq!(r.tracks, 3 + STREAMS.len());
        let cw = std::fs::read(tmp.join("sound/BGM_NetworkMainScene_6.wav")).unwrap();
        assert!(cw.windows(4).any(|x| x == b"smpl"), "Course World loops");
        // the beat markers (smm1-100mario-screens.md §3): Course World's 56 beats ending at its
        // loop end; the between-course map's count without an end or a loop
        let beats: Value =
            serde_json::from_slice(&std::fs::read(tmp.join("sound/bgm.json")).unwrap()).unwrap();
        let cw = &beats["BGM_NetworkMainScene_2"];
        assert_eq!(cw["marks"]["beat:56"], 0);
        assert_eq!(cw["marks"]["beatend"], 1054117);
        assert_eq!(cw["rate"], 32000);
        let map = &beats["BGM_100MarioMap"];
        assert!(map["marks"].get("beat:56").is_some());
        assert!(map["marks"].get("beatend").is_none() && map["loop"].is_null());
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
