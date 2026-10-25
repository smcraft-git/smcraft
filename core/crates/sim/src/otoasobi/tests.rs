use super::*;
use crate::input::{self, Input};
use mmk_formats::record::Record;
use mmk_formats::smm2::{GroundTile, SoundEffect};

fn object(id: i16, x: i32, y: i32, sound: i16) -> CourseObject {
    let mut o = CourseObject::from_slice(&[0; 0x20], 0);
    (o.x, o.y, o.width, o.height) = (x * 160 + 80, y * 160 + 80, 1, 1);
    (o.flags, o.child_flags) = (0x0600_0040, 0x0600_0040);
    (o.id, o.child_id, o.link_id, o.sound_effect_id) = (id, -1, -1, sound);
    o
}

fn spot(id: u8, x: u8, y: u8) -> SoundEffect {
    let mut s = SoundEffect::from_slice(&[0; 4], 0);
    (s.id, s.x, s.y) = (id, x, y);
    s
}

/// A flat SMB1 course (ground row 0, tiles 0–79) with these spots and parts.
fn course(spots: Vec<SoundEffect>, objects: Vec<CourseObject>) -> Course {
    let mut c = Course::blank("M1");
    c.header.timer = 500;
    c.main.ground.items = (0..80)
        .map(|x| {
            let mut g = GroundTile::from_slice(&[0; 4], 0);
            (g.x, g.y) = (x, 0);
            g
        })
        .collect();
    c.main.sound_effects.items = spots;
    c.main.objects.items = objects;
    c
}

/// Run `frames` ticks holding `buttons`; every cue seen.
fn run(w: &mut World, s: &mut SoundParts, buttons: u32, frames: usize) -> Vec<Cue> {
    let mut out = Vec::new();
    for _ in 0..frames {
        w.step(Input { buttons });
        s.step(w);
        out.extend(s.cues.iter().copied());
    }
    out
}

#[test]
fn tables_agree_with_the_game() {
    // the 47 offered ids, each once, in five pages
    let mut ids = ICON_IDS.to_vec();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), 47);
    let mut on_pages: Vec<usize> = PAGES.iter().flat_map(|p| p.iter().copied()).collect();
    on_pages.sort();
    assert_eq!(
        on_pages,
        (0..47).collect::<Vec<_>>(),
        "every icon on one page"
    );
    assert!(PAGES.iter().all(|p| p.len() <= 13), "a ring page holds 13");
    assert_eq!(sheet_cell(36), Some((8, 3)), "Flash with the animations");
    for unused in [21, 22, 27, 28, 33, 34, 39, 40, 55] {
        assert!(!valid(unused));
    }
    // the music table 0x7102c17050: slot → id
    let slots = [10, 20, 9, 19, 48, 49, 50, 51, 52, 53, 54];
    for (slot, &id) in slots.iter().enumerate() {
        let v = VOICES[id as usize];
        assert!(
            matches!(v, Voice::Music(s) | Voice::Ambient(s) if s as usize == slot),
            "{id}"
        );
        // slots 0, 1, 5, 6 are the ambient ones (mask 99)
        assert_eq!(matches!(v, Voice::Ambient(_)), 99 >> slot & 1 != 0);
    }
    assert_eq!(label(25), Some("LoudLaughter"));
    assert_eq!(page_of(54), Some((4, 10)));
    assert_eq!(page_of(47), Some((3, 7)));
    assert_eq!(user(9, Variant::Link), "Otoasobi09_Link");
    assert_eq!(user(9, Variant::Usa), "Otoasobi09_USA");
    assert_eq!(user(8, Variant::Usa), "Otoasobi08");
    // Mario's triggers: no id in two masks
    let n = (0..IDS as u8)
        .filter(|&i| player_trigger(i).is_some())
        .count();
    assert_eq!(n, 6 + 8 + 4 + 4 + 5 + 2);
    assert_eq!(player_trigger(13), Some(PlayerTrigger::Jump));
    assert_eq!(player_trigger(11), Some(PlayerTrigger::Damage));
    assert_eq!(player_trigger(24), Some(PlayerTrigger::Fall));
    assert_eq!(player_trigger(3), Some(PlayerTrigger::PowerUp));
    assert_eq!(player_trigger(45), Some(PlayerTrigger::Ride));
    assert_eq!(player_trigger(29), Some(PlayerTrigger::Turn));
    assert_eq!(player_trigger(8), None, "Ding Dong: the start only");
}

#[test]
fn a_spot_plays_on_each_entry() {
    let c = course(vec![spot(0, 8, 1)], vec![]);
    let mut w = World::from_course(&c);
    let mut s = SoundParts::new(&c);
    assert!(s.any());
    let cues = run(&mut w, &mut s, input::RIGHT, 120);
    let hits: Vec<&Cue> = cues.iter().filter(|c| c.source == Source::Spot).collect();
    assert_eq!(hits.len(), 1, "{cues:?}");
    assert_eq!(
        (hits[0].id, hits[0].x, hits[0].y),
        (0, 8.0 * 16.0 + 8.0, 16.0)
    );
    // back through it: again
    let back = run(&mut w, &mut s, input::LEFT, 160);
    assert_eq!(back.iter().filter(|c| c.source == Source::Spot).count(), 1);
}

#[test]
fn a_spot_out_of_reach_stays_quiet() {
    // two tiles over small Mario's head
    let c = course(vec![spot(3, 8, 3)], vec![]);
    let mut w = World::from_course(&c);
    let mut s = SoundParts::new(&c);
    let cues = run(&mut w, &mut s, input::RIGHT, 120);
    assert!(cues.is_empty(), "{cues:?}");
}

#[test]
fn a_coin_plays_when_taken() {
    let c = course(vec![], vec![object(8, 8, 1, 23), object(8, 30, 5, 24)]);
    let mut w = World::from_course(&c);
    let mut s = SoundParts::new(&c);
    let cues = run(&mut w, &mut s, input::RIGHT, 120);
    let got: Vec<(u8, Source)> = cues.iter().map(|c| (c.id, c.source)).collect();
    assert_eq!(got, vec![(23, Source::Part(PartTrigger::Taken))]);
}

#[test]
fn a_pink_coin_plays_when_taken() {
    let c = course(vec![], vec![object(92, 8, 1, 42)]);
    let mut w = World::from_course(&c);
    let mut s = SoundParts::new(&c);
    let cues = run(&mut w, &mut s, input::RIGHT, 120);
    assert_eq!(w.pink.len(), 1, "taken");
    assert_eq!(
        cues.iter().map(|c| (c.id, c.source)).collect::<Vec<_>>(),
        vec![(42, Source::Part(PartTrigger::Taken))]
    );
}

#[test]
fn mario_s_effect_on_damage() {
    let mut c = course(vec![], vec![]);
    c.header.player_sound_effect = 0; // Shock
    let mut w = World::from_course(&c);
    let mut s = SoundParts::new(&c);
    w.player.form = crate::player::Form::Fire;
    run(&mut w, &mut s, 0, 2);
    s.cues.clear();
    // a hit: Fire → Super, the world's power-down event
    w.player.form = crate::player::Form::Super;
    w.events.push(Event::PowerDown);
    w.tick += 1;
    s.step(&w);
    assert_eq!(s.cues.len(), 1, "{:?}", s.cues);
    assert_eq!(s.cues[0].source, Source::Player(0));
}

#[test]
fn mario_s_effect_at_the_start_and_on_a_running_jump() {
    let mut c = course(vec![], vec![]);
    c.header.player_sound_effect = 13; // Jump
    let mut w = World::from_course(&c);
    let mut s = SoundParts::new(&c);
    let first = run(&mut w, &mut s, 0, 1);
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].source, Source::Player(0));
    // a standing jump: nothing
    let still = run(&mut w, &mut s, input::JUMP, 10);
    assert!(still.is_empty(), "{still:?}");
    run(&mut w, &mut s, 0, 60);
    // a running jump at full speed
    let cues = run(&mut w, &mut s, input::RIGHT | input::RUN, 90);
    assert!(cues.is_empty());
    let cues = run(&mut w, &mut s, input::RIGHT | input::RUN | input::JUMP, 5);
    assert_eq!(cues.len(), 1, "{cues:?}");
}

#[test]
fn music_effects_start_and_end_by_range() {
    // Boss Music on a spot: plays from the spot, ends 160 px away
    let c = course(vec![spot(9, 6, 1)], vec![]);
    let mut w = World::from_course(&c);
    let mut s = SoundParts::new(&c);
    run(&mut w, &mut s, input::RIGHT, 50);
    let m = s.music.expect("the boss music");
    assert_eq!(
        (m.id, m.slot, m.ambient, m.trigger),
        (9, 2, false, Trigger::Spot)
    );
    run(&mut w, &mut s, input::RIGHT | input::RUN, 200);
    assert!(s.music.is_none(), "{:?} at x {}", s.music, w.player.x);
    // Mario's own music lasts
    let mut c = course(vec![], vec![]);
    c.header.player_sound_effect = 52;
    let mut w = World::from_course(&c);
    let mut s = SoundParts::new(&c);
    run(&mut w, &mut s, input::RIGHT | input::RUN, 300);
    assert_eq!(s.music.map(|m| (m.id, m.slot)), Some((52, 8)));
}

#[test]
fn shared_voices_start_once_a_frame_and_own_voices_hold() {
    let c = course(vec![], vec![]);
    let w = World::from_course(&c);
    let mut s = SoundParts::new(&c);
    s.started = true;
    let fire = |s: &mut SoundParts, id| s.fire(&w, id, w.player.x, w.player.y, Source::Spot, true);
    fire(&mut s, 2);
    fire(&mut s, 2);
    fire(&mut s, 45);
    fire(&mut s, 45);
    fire(&mut s, 0);
    fire(&mut s, 0);
    let ids: Vec<u8> = s.cues.iter().map(|c| c.id).collect();
    // Kick once this frame; the guitar holds; Shock (own, no hold) restarts
    assert_eq!(ids, vec![2, 45, 0, 0]);
}

#[test]
fn coverage_counts_what_is_not_modelled() {
    let c = course(
        vec![spot(46, 3, 1)],
        vec![object(8, 3, 3, 23), object(11, 9, 3, 5)],
    );
    let u = unsupported(&c);
    assert!(
        u.contains(&"sound effect 46 (audio filter)".to_string()),
        "{u:?}"
    );
    assert!(
        u.contains(&"sound effect on part 11".to_string()),
        "a lift: {u:?}"
    );
    assert!(!u.iter().any(|m| m.ends_with("part 8")), "{u:?}");
}

#[test]
fn a_block_plays_when_hit() {
    let c = course(vec![], vec![object(4, 6, 3, 15)]);
    let mut w = World::from_course(&c);
    let mut s = SoundParts::new(&c);
    // under the brick, then up into it
    let mut cues = Vec::new();
    for _ in 0..300 {
        if w.player.x >= 6.0 * 16.0 - 2.0 {
            break;
        }
        cues.extend(run(&mut w, &mut s, input::RIGHT, 1));
    }
    cues.extend(run(&mut w, &mut s, 0, 40));
    assert!((96.0..112.0).contains(&w.player.x), "x {}", w.player.x);
    assert!(cues.is_empty(), "{cues:?}");
    let up = run(&mut w, &mut s, input::JUMP, 30);
    assert_eq!(
        up.iter().map(|c| (c.id, c.source)).collect::<Vec<_>>(),
        vec![(15, Source::Part(PartTrigger::Hit))]
    );
}

#[test]
fn an_enemy_plays_when_defeated() {
    let c = course(vec![], vec![object(0, 10, 1, 6), object(0, 14, 1, -1)]);
    let mut w = World::from_course(&c);
    let mut s = SoundParts::new(&c);
    run(&mut w, &mut s, 0, 2);
    let tagged = w
        .actors
        .iter()
        .position(|a| {
            a.kind == Kind::Goomba && a.spawn.is_some_and(|i| w.level.spawns[i].x == 168.0)
        })
        .expect("the tagged Goomba");
    let plain = w
        .actors
        .iter()
        .position(|a| a.kind == Kind::Goomba && a.x > 200.0)
        .unwrap();
    w.actors[plain].mode = crate::actors::Mode::Stomped;
    w.actors[tagged].mode = crate::actors::Mode::Stomped;
    s.step(&w);
    w.tick += 1;
    s.step(&w);
    assert_eq!(
        s.cues.iter().map(|c| (c.id, c.source)).collect::<Vec<_>>(),
        vec![(6, Source::Part(PartTrigger::Defeated))]
    );
}
