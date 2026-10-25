//! The SMM1-layout 100 Mario run end to end on the owner's import (skipped without it): the
//! hand-offs between the screens, their fades and the input locks, with the frame counts of
//! research/specs/smm1-100mario-screens.md §8.

use crate::challenge::{ChallengeFlow, Presentation};
use crate::host::{CourseEntry, Saves, Source};
use crate::menu::{Buttons, UiInput};
use crate::play::tests::{MockHost, with};
use crate::play::{Phase, PlayInput};
use crate::testing::{press, smm1};
use crate::{CueKind, Ui, UiKind};

fn library(n: usize) -> Vec<CourseEntry> {
    (0..n)
        .map(|i| CourseEntry {
            id: format!("c{i}"),
            name: format!("Course {i}"),
            playable: true,
            key: format!("k{i}"),
            ..Default::default()
        })
        .collect()
}

fn input(b: Buttons) -> PlayInput {
    PlayInput {
        ui: if b == Buttons::NONE {
            UiInput::default()
        } else {
            press(b)
        },
        start: false,
    }
}

/// Step until `done` holds (at most `max` ticks); the ticks it took.
fn until(
    f: &mut ChallengeFlow,
    host: &mut MockHost,
    saves: &mut Saves,
    max: i32,
    done: impl Fn(&ChallengeFlow, &MockHost) -> bool,
) -> Option<i32> {
    for t in 1..=max {
        with(host, saves, |cx| f.step(&input(Buttons::NONE), cx));
        if done(f, host) {
            return Some(t);
        }
    }
    None
}

#[test]
fn owner_new_game_reaches_the_course_through_the_map_fade_and_card() {
    let Some(smm1) = smm1() else { return };
    let smm2 = Ui::new(UiKind::Smm2, mmk_ui::UiCache::in_memory(), None);
    let lib = library(12);
    let mut host = MockHost::default();
    let mut saves = Saves::default();
    let mut f = with(&mut host, &mut saves, |cx| {
        ChallengeFlow::new(cx, &smm2, Some(&smm1), &lib, Presentation::Smm1)
    });
    assert_eq!(f.presentation, Presentation::Smm1);
    // the select takes no input before the title bar's and the map's In (48)
    with(&mut host, &mut saves, |cx| {
        f.step(&input(Buttons::CONFIRM), cx)
    });
    assert!(f.at_menu(), "locked");
    for _ in 0..48 {
        with(&mut host, &mut saves, |cx| {
            f.step(&input(Buttons::NONE), cx)
        });
    }
    with(&mut host, &mut saves, |cx| {
        f.step(&input(Buttons::CONFIRM), cx)
    });
    assert!(!f.at_menu(), "New Game");
    // the New Game demo (1128 frames on Easy), then the scene fade covers in 25
    let loaded = until(&mut f, &mut host, &mut saves, 2000, |_, h| {
        !h.loaded.is_empty()
    });
    let loaded = loaded.expect("the course loads");
    assert!((1128..=1128 + 25 + 2).contains(&loaded), "{loaded}");
    assert!(matches!(host.loaded[0], Source::Library(_)));
    // the card first (AnimA 215, AnimB 70, Out 150): the course waits under it
    let p = f.playing.as_ref().unwrap();
    assert_eq!(p.phase, Phase::Card);
    let playing = until(&mut f, &mut host, &mut saves, 600, |f, _| {
        f.playing
            .as_ref()
            .is_some_and(|p| p.phase == Phase::Playing)
    })
    .unwrap();
    assert!((433..=440).contains(&playing), "{playing}");
    assert!(host.steps <= 1, "the engine waited for the card");
}

#[test]
fn owner_a_death_fades_to_the_card_and_a_clear_to_the_map() {
    let Some(smm1) = smm1() else { return };
    let smm2 = Ui::new(UiKind::Smm2, mmk_ui::UiCache::in_memory(), None);
    let lib = library(12);
    let mut host = MockHost {
        death_clock: true,
        ..Default::default()
    };
    host.status.lives = 100;
    let mut saves = Saves::default();
    let mut f = with(&mut host, &mut saves, |cx| {
        let mut f = ChallengeFlow::new(cx, &smm2, Some(&smm1), &lib, Presentation::Smm1);
        f.start(cx, 0, false);
        f
    });
    // A skips the map demo up to Mario's Go; then the fade and the card (A cuts it too)
    with(&mut host, &mut saves, |cx| {
        f.step(&input(Buttons::CONFIRM), cx)
    });
    until(&mut f, &mut host, &mut saves, 400, |_, h| {
        !h.loaded.is_empty()
    })
    .unwrap();
    for _ in 0..30 {
        with(&mut host, &mut saves, |cx| {
            f.step(&input(Buttons::NONE), cx)
        });
    }
    with(&mut host, &mut saves, |cx| {
        f.step(&input(Buttons::CONFIRM), cx)
    });
    until(&mut f, &mut host, &mut saves, 5, |f, _| {
        f.playing
            .as_ref()
            .is_some_and(|p| p.phase == Phase::Playing)
    })
    .unwrap();
    // a death: the fade covers as the engine restarts the course, then the card's AnimB and
    // OutMiss (70 + 60) over it
    host.status.dying = true;
    host.status.restart_in = 60;
    let covered = until(&mut f, &mut host, &mut saves, 120, |f, _| {
        f.playing.as_ref().is_some_and(|p| p.covered())
    })
    .unwrap();
    assert!(covered <= 40, "{covered}");
    let card = until(&mut f, &mut host, &mut saves, 120, |f, _| {
        f.playing.as_ref().is_some_and(|p| p.phase == Phase::Card)
    });
    assert!(card.is_some());
    let back = until(&mut f, &mut host, &mut saves, 200, |f, _| {
        f.playing
            .as_ref()
            .is_some_and(|p| p.phase == Phase::Playing)
    })
    .unwrap();
    assert!((129..=133).contains(&back), "{back}");
    assert_eq!(f.run.status().lives, 99);
    // a clear: the clear screen (In 180 locks the buttons), Next, then blocks to the map
    host.status.cleared = true;
    with(&mut host, &mut saves, |cx| {
        f.step(&input(Buttons::NONE), cx)
    });
    host.status.cleared = false;
    for _ in 0..179 {
        with(&mut host, &mut saves, |cx| {
            f.step(&input(Buttons::CONFIRM), cx)
        });
    }
    assert!(f.playing.is_some(), "still on the clear screen");
    for _ in 0..3 {
        with(&mut host, &mut saves, |cx| {
            f.step(&input(Buttons::CONFIRM), cx)
        });
    }
    smm1.take_cues();
    let map = until(&mut f, &mut host, &mut saves, 200, |f, _| {
        f.playing.is_none()
    })
    .unwrap();
    // Next's Decide, the layout's Out (16), then the blocks (56)
    assert!(map > 56, "{map}");
    assert_eq!(f.run.status().course_number, 2);
    // the music: the blocks' fade-out, then the between-course map's tune
    let music: Vec<(String, CueKind, f32)> = smm1
        .take_cues()
        .into_iter()
        .filter(|c| c.kind != CueKind::Sound)
        .map(|c| (c.name, c.kind, c.delay))
        .collect();
    assert_eq!(
        music,
        [
            (String::new(), CueKind::StopMusic, 56.0),
            ("BGM_100MarioMap".into(), CueKind::Music, 0.0)
        ]
    );
}

/// The owner's import from importer 5 on: GAME OVER's letters drop in one by one (the text
/// pane's per-character transform, animated by `OneTime`'s `FLCT`).
#[test]
fn owner_game_over_letters_drop_in() {
    let Some(smm1) = smm1() else { return };
    let version = smm1
        .cache_mut()
        .json_file("manifest.json")
        .and_then(|m| m["importer_version"].as_u64())
        .unwrap_or(0);
    if version < 5 {
        eprintln!("SMM1 import without per-character transforms (importer < 5); skipping");
        return;
    }
    let Some(mut s) = crate::smm1::GameOverScene::new(&smm1) else {
        return;
    };
    let above = |s: &mut crate::smm1::GameOverScene| {
        let mut out = Vec::new();
        s.draw(&mut out);
        out.iter()
            .map(|d| match d {
                crate::DrawItem::Layout { quads, .. } => quads
                    .iter()
                    .filter(|q| q.pos.iter().all(|p| p[1] > 360.0))
                    .count(),
                _ => 0,
            })
            .sum::<usize>()
    };
    for _ in 0..50 {
        s.step(&UiInput::default());
    }
    let before = above(&mut s);
    // the eight letters and their shadows wait above the screen (Mario too, from 360)…
    assert!(before >= 16, "{before}");
    for _ in 50..74 {
        s.step(&UiInput::default());
    }
    // …drop one after another…
    let mid = above(&mut s);
    assert!(mid > 0 && mid < before, "{mid}");
    for _ in 74..200 {
        s.step(&UiInput::default());
    }
    // …and have all landed
    assert_eq!(above(&mut s), 0);
}

/// The card's maker group is centred on the name's width (0x02524d0c): a short name moves it
/// right of a long one.
#[test]
fn owner_card_centres_the_maker_group() {
    let Some(smm1) = smm1() else { return };
    let Some(mut card) = smm1.layout("Play_CourseIn100Mario_00") else {
        return;
    };
    let x = |card: &mut crate::layout::Layout, maker: &str| {
        let info = crate::smm1::RunCardInfo {
            maker: maker.into(),
            ..Default::default()
        };
        crate::smm1::configure_card(card, &info);
        card.apply(0);
        card.rect("N_Player_00").unwrap().center[0]
    };
    let short = x(&mut card, "Al");
    let long = x(&mut card, "Alexandrina Montgomery");
    assert!(short > long, "{short} {long}");
}

/// The pause shows the course's stars and players when the course data has them.
#[test]
fn owner_pause_shows_the_course_counts() {
    let Some(smm1) = smm1() else { return };
    let mut o = crate::smm1::RunOverlay::new(&smm1);
    o.counts = (Some(1234), None);
    o.show_pause("100 Mario Challenge (1/8)", "Course", "Maker", 0);
    let mut out = Vec::new();
    o.draw(&mut out);
    let shown =
        |o: &mut crate::smm1::RunOverlay, pane: &str| o.pause_rect(pane).is_some_and(|r| r.visible);
    assert!(shown(&mut o, "P_YeahCountIcon_00"));
    assert!(!shown(&mut o, "P_PlayerCountIcon_00"));
}
