//! The SMM1 screens' sounds (`smm1_sound` + the hooks in `smm1`): a synthetic import (stand-in
//! layouts, a small `sounds.json`) checks which cues each screen event queues and when; the
//! owner's import, when present, checks the real data's keys on the real layouts.

use mmk_formats::fixtures::{sample_bflan, sample_bflyt};
use mmk_formats::lyt::{self, Animation, Layout as Lyt};
use mmk_ui::{MemFiles, UiCache};
use serde_json::{Map, json};

use crate::menu::{Buttons, UiInput};
use crate::smm1::{ChallengeMenuScene, Entry, MapIntroScene, RunCardInfo, RunOverlay, RunQuery};
use crate::smm1_sound::Smm1Sounds;
use crate::testing::{press, smm1};
use crate::{Ui, UiKind};

const SOUNDS: &str = r#"{"game": "SMM1", "users": {
  "UI_Cmn_PauseMenu_00": {"keys": {
    "In": {"kind": "switch", "children": [
      {"key": "In_00", "sound": "M1_Pose", "if": {"cmp": "==", "enum": "M1"}},
      {"key": "In_02", "sound": "MW_Pose", "volume": 0.8, "if": {"cmp": "==", "enum": "MW"}}]},
    "Out": {"kind": "switch", "children": [
      {"key": "In_02_00", "sound": "MW_Back", "if": {"cmp": "==", "enum": "MW"}}]}}},
  "UI_Cmn_PauseMenu_00_L_RetryBtn_00": {"keys": {
    "Decide": {"sound": "Chukettei"}, "TouchSelect": {"sound": "Cursor"}}},
  "UI_Play_CourseClear_00": {"keys": {"In": {"sound": "ClearFilter"}}},
  "UI_Play_CourseClear_00_L_ExitBtn_00": {"keys": {
    "Decide": {"sound": "PlayExit"}, "TouchSelect": {"sound": "Cursor"}}},
  "UI_Play_SlidePassIconKey_00": {"keys": {
    "Countdown1": {"sound": "Off"}, "Countdown2": {"sound": "On1"}, "Countdown3": {"sound": "On2"}}},
  "UI_Play_SlidePass_00": {"keys": {"SlidePass": {"kind": "switch", "children": [
      {"key": "SlidePass_UP", "sound": "SlideUp", "if": {"cmp": "==", "enum": "cUp"}}]}}},
  "UI_Play_CourseIn100Mario_00": {"keys": {
    "CouseInWalk100": {"sound": "Footsteps"},
    "CouseInJump100": {"kind": "blend", "children": [
      {"key": "j", "sound": "Hop"},
      {"key": "w", "kind": "switch", "children": [
        {"key": "w80", "sound": "Walk80", "delay": 32, "if": {"cmp": ">=", "int": 80}},
        {"key": "w0", "sound": "Walk0", "delay": 32, "if": {"cmp": ">=", "int": 0}}]}]}}}
}}"#;

/// An SMM1 "import" in memory: `layouts` drawn by the sample layout, every animation the
/// screens play standing in as the sample one, and [`SOUNDS`].
fn synthetic_ui(layouts: &[&str]) -> Ui {
    let layout = Lyt::parse(&sample_bflyt()).unwrap();
    let anim = lyt::json::animation_to_json(&Animation::parse(&sample_bflan()).unwrap());
    let mut anims = Map::new();
    for a in [
        "In",
        "Out",
        "Default",
        "Pass",
        "CountDown_00",
        "CountDown_01",
        "CountDown_02",
        "AnimA",
        "AnimB",
        "OutMiss",
    ] {
        anims.insert(a.into(), anim.clone());
    }
    let doc = json!({ "layout": lyt::json::layout_to_json(&layout), "animations": anims });
    let mut m = MemFiles::new("memory");
    m.insert("manifest.json", br#"{"game": "SMM1"}"#.to_vec());
    for l in layouts {
        m.insert(format!("ui/layouts/{l}.json"), doc.to_string().into_bytes());
    }
    let cache = UiCache::from_files(Box::new(m)).unwrap();
    Ui::with_sounds(UiKind::Smm1, cache, None, Smm1Sounds::from_json(SOUNDS))
}

fn taken(ui: &Ui) -> Vec<String> {
    ui.take_cues().into_iter().map(|c| c.name).collect()
}

#[test]
fn run_overlay_plays_the_pause_clear_skip_and_card_sounds() {
    let ui = synthetic_ui(&[
        "Cmn_PauseMenu_00",
        "Play_CourseClear_00",
        "Play_SlidePassIconKey_00",
        "Play_CourseIn100Mario_00",
    ]);
    let mut o = RunOverlay::new(&ui);
    // the pause menu: In by the course's style, the cursor, a decide, Out
    o.style = Some(2);
    o.show_pause("100 Mario", "Course", "Maker", 0);
    let cues = ui.take_cues();
    assert_eq!((cues[0].name.as_str(), cues[0].volume), ("MW_Pose", 0.8));
    assert_eq!(cues[0].ui, UiKind::Smm1);
    o.pause_select(1);
    assert_eq!(taken(&ui), ["Cursor"]);
    o.pause_select(1);
    assert!(taken(&ui).is_empty(), "no move, no sound");
    o.pause_decide(1);
    assert_eq!(taken(&ui), ["Chukettei"]);
    o.hide_pause();
    assert_eq!(taken(&ui), ["MW_Back"]);
    // course clear: In; the initial focus is silent, a move is not; the decide
    o.show_clear("Course", "Maker", Some(600), None, false);
    assert_eq!(taken(&ui), ["ClearFilter"]);
    o.clear_select(0);
    assert_eq!(taken(&ui), ["Cursor"]);
    o.press_clear();
    assert_eq!(taken(&ui), ["PlayExit"]);
    // the skip countdown: one key per 32-frame step, dropped when the key is let go
    o.clear_all();
    o.skip_begin();
    let mut heard = Vec::new();
    for _ in 0..40 {
        o.step();
        heard.extend(taken(&ui));
    }
    assert_eq!(heard, ["On2", "On1"]);
    o.skip_cancel();
    for _ in 0..80 {
        o.step();
        assert!(taken(&ui).is_empty());
    }
    o.skip_begin();
    o.skip_pass();
    assert_eq!(taken(&ui), ["SlideUp"]);
    // the card after a miss: the jump, its walk sound by the lives
    o.clear_all();
    let info = RunCardInfo {
        lives: 85,
        ..Default::default()
    };
    o.show_card(&info);
    let cues = ui.take_cues();
    let names: Vec<_> = cues.iter().map(|c| (c.name.as_str(), c.delay)).collect();
    assert_eq!(names, [("Hop", 0.0), ("Walk80", 32.0)]);
}

#[test]
fn without_sounds_the_screens_stay_silent() {
    let layout = Lyt::parse(&sample_bflyt()).unwrap();
    let doc = json!({ "layout": lyt::json::layout_to_json(&layout), "animations": {} });
    let mut m = MemFiles::new("memory");
    m.insert("manifest.json", br#"{"game": "SMM1"}"#.to_vec());
    m.insert(
        "ui/layouts/Cmn_PauseMenu_00.json",
        doc.to_string().into_bytes(),
    );
    let ui = Ui::new(
        UiKind::Smm1,
        UiCache::from_files(Box::new(m)).unwrap(),
        None,
    );
    let mut o = RunOverlay::new(&ui);
    o.show_pause("", "", "", 0);
    o.pause_select(2);
    assert!(ui.take_cues().is_empty());
}

struct AllOpen;

impl RunQuery for AllOpen {
    fn unlocked(&self, _: usize) -> bool {
        true
    }
    fn can_continue(&self, _: usize) -> bool {
        false
    }
    fn continue_info(&self, _: usize) -> Option<(u32, usize)> {
        None
    }
}

/// The ticks (from 1) at which each sound started over `ticks` steps.
fn run_intro(
    ui: &Ui,
    s: &mut MapIntroScene,
    ticks: i32,
    skip_at: Option<i32>,
) -> Vec<(i32, String)> {
    let mut out = Vec::new();
    for t in 1..=ticks {
        let input = if skip_at == Some(t) {
            press(Buttons::CONFIRM)
        } else {
            UiInput::default()
        };
        s.step(&input);
        out.extend(
            ui.take_cues()
                .into_iter()
                .map(|c| (t + c.delay as i32, c.name)),
        );
    }
    out
}

/// The owner's import (skipped without it): a New Game's map demo and card, with the data's
/// sounds where the animations they go with start.
#[test]
fn owner_new_game_map_and_card_sounds() {
    let Some(ui) = smm1() else { return };
    if ui.smm1_sounds().is_none() {
        eprintln!("SMM1 import without sounds (importer < 4); skipping");
        return;
    }
    let info = RunCardInfo {
        lives: 100,
        ..Default::default()
    };
    let mut s = MapIntroScene::new(&ui, Entry::NewGame, 0, &info, 0, None).unwrap();
    let heard = run_intro(&ui, &mut s, 1600, None);
    let at = |name: &str| heard.iter().find(|(_, n)| n == name).map(|(t, _)| *t);
    // Telop and MoveKuribo at 200 (the Goombas walk from MoveKuribo frame 60), AnimB after
    assert_eq!(at("M3_StarsGather"), Some(200));
    assert_eq!(at("SE_KuriboAshiotoShort"), Some(260));
    let peach = at("MW_PeachHelp").unwrap();
    assert!(peach > 400, "{heard:?}");
    // Mario jumps out of the castle and walks, the wipe, then the card's 100 Marios
    let jump = at("M1_SmallMarioJump").unwrap();
    assert_eq!(jump, peach + 20);
    assert_eq!(at("M3_MoveMap"), Some(jump + 28 + 5));
    let wipe = at("M3_Wipe").unwrap();
    assert_eq!(at("SE_100marioAshioto"), Some(wipe + 32));
    assert!(at("SE_walk80").is_some(), "100 lives walk: {heard:?}");

    // skipping the demo: democancel, and nothing the demo had still to play
    let mut s = MapIntroScene::new(&ui, Entry::NewGame, 0, &info, 0, None).unwrap();
    let heard = run_intro(&ui, &mut s, 400, Some(100));
    let names: Vec<&str> = heard.iter().map(|(_, n)| n.as_str()).collect();
    assert!(names.contains(&"yr_Sweep_GOAL"), "{heard:?}");
    assert!(!names.contains(&"M3_StarsGather") && !names.contains(&"MW_PeachHelp"));
}

#[test]
fn owner_difficulty_select_sounds() {
    let Some(ui) = smm1() else { return };
    if ui.smm1_sounds().is_none() {
        return;
    }
    let mut m = ChallengeMenuScene::new(&ui, 0, &AllOpen).unwrap();
    m.step(&UiInput::default(), &AllOpen);
    ui.take_cues();
    // a tab: SE_NetworkLeft and the curtain (closing, opening 40 frames later)
    m.step(&press(Buttons::RIGHT), &AllOpen);
    let cues: Vec<(String, f32)> = ui
        .take_cues()
        .into_iter()
        .map(|c| (c.name, c.delay))
        .collect();
    let names: Vec<&str> = cues.iter().map(|c| c.0.as_str()).collect();
    assert_eq!(names, ["SE_NetworkLeft", "SE_Carten", "SE_cartenOpen"]);
    assert_eq!(cues[2].1, 40.0);
    // New Game on Normal: the start button's decide
    m.step(&press(Buttons::CONFIRM), &AllOpen);
    let names: Vec<String> = ui.take_cues().into_iter().map(|c| c.name).collect();
    assert_eq!(names, ["daikettefinal"]);
}
