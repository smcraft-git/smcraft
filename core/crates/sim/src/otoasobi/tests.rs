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
    // the filters, a lift, a Bill Blaster, a POW (it never sets off), a Seesaw, a Tree and a
    // Dry Bones Shell (Dry Bones' 0x4 form) are all played
    let mut shell = object(46, 24, 1, 5);
    shell.flags |= 0x4;
    let c = course(
        vec![spot(46, 3, 1), spot(47, 4, 1)],
        vec![
            object(8, 3, 3, 23),
            object(11, 9, 3, 5),
            object(13, 12, 1, 5),
            object(19, 14, 1, 5),
            object(91, 20, 4, 5),
            object(106, 30, 1, 5),
            shell,
        ],
    );
    let u = unsupported(&c);
    assert!(u.is_empty(), "{u:?}");
}

#[test]
fn a_seesaw_plays_on_a_landing_on_its_empty_plank() {
    let c = course(vec![], vec![object(91, 12, 4, 0)]);
    let mut w = World::from_course(&c);
    let mut s = SoundParts::new(&c);
    run(&mut w, &mut s, 0, 2);
    let seesaw = w
        .actors
        .iter()
        .find(|a| matches!(a.kind, Kind::Seesaw { .. }))
        .map(|a| a.id)
        .expect("the seesaw");
    let tick = |w: &mut World, s: &mut SoundParts, on: bool| {
        w.player.ride = on.then_some(seesaw);
        w.player.on_ground = true;
        w.tick += 1;
        s.step(w);
        s.cues.iter().map(|c| (c.id, c.source)).collect::<Vec<_>>()
    };
    assert_eq!(
        tick(&mut w, &mut s, true),
        vec![(0, Source::Part(PartTrigger::Ridden))]
    );
    for _ in 0..3 {
        assert!(tick(&mut w, &mut s, true).is_empty(), "standing on");
    }
    // off and on again inside the 25 frames: nothing, and the count starts over
    for _ in 0..20 {
        tick(&mut w, &mut s, false);
    }
    assert!(tick(&mut w, &mut s, true).is_empty());
    for _ in 0..20 {
        tick(&mut w, &mut s, false);
    }
    assert!(
        tick(&mut w, &mut s, true).is_empty(),
        "the count was refilled"
    );
    for _ in 0..25 {
        tick(&mut w, &mut s, false);
    }
    assert_eq!(tick(&mut w, &mut s, true).len(), 1);
}

#[test]
fn a_p_switch_plays_when_picked_up_not_when_stood_on() {
    let c = course(vec![], vec![object(18, 12, 1, 0)]);
    let mut w = World::from_course(&c);
    let mut s = SoundParts::new(&c);
    run(&mut w, &mut s, 0, 2);
    let ps = w
        .actors
        .iter()
        .find(|a| a.kind == Kind::PSwitch)
        .map(|a| a.id)
        .expect("the P Switch");
    let tick = |w: &mut World, s: &mut SoundParts, on: bool, held: bool| {
        w.player.ride = on.then_some(ps);
        w.player.held = held.then_some(ps);
        w.tick += 1;
        s.step(w);
        s.cues.iter().map(|c| (c.id, c.source)).collect::<Vec<_>>()
    };
    assert!(tick(&mut w, &mut s, true, false).is_empty(), "stood on");
    assert_eq!(
        tick(&mut w, &mut s, false, true),
        vec![(0, Source::Part(PartTrigger::PickedUp))]
    );
    assert!(tick(&mut w, &mut s, false, true).is_empty(), "still held");
}

#[test]
fn a_tree_plays_each_time_a_player_reaches_its_top() {
    let mut tree = object(106, 12, 1, 0);
    tree.height = 6;
    let c = course(vec![], vec![tree]);
    let mut w = World::from_course(&c);
    let mut s = SoundParts::new(&c);
    run(&mut w, &mut s, 0, 2);
    let t = w.level.trees[0];
    let top = t.canopy().1 - 16.0;
    let tick = |w: &mut World, s: &mut SoundParts, y: f32| {
        (w.player.x, w.player.y) = (t.x, y);
        w.player.climbing = Some((t.x, 0));
        w.tick += 1;
        s.step(w);
        s.cues
            .iter()
            .map(|c| (c.id, c.source, c.y))
            .collect::<Vec<_>>()
    };
    assert!(tick(&mut w, &mut s, top - 8.0).is_empty(), "on the trunk");
    assert_eq!(
        tick(&mut w, &mut s, top),
        vec![(0, Source::Part(PartTrigger::Shaken), t.y + 40.0)]
    );
    assert!(tick(&mut w, &mut s, top).is_empty(), "still at the top");
    tick(&mut w, &mut s, top - 8.0);
    assert_eq!(tick(&mut w, &mut s, top).len(), 1, "again");
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

#[test]
fn trigger_kinds_and_their_ranges() {
    // 0x7101537ab0 / 0x7100e443c0: start within, end beyond
    let r = |t: Trigger| (t.kind(), t.start_range(), t.end_range());
    assert_eq!(r(Trigger::Enemy), (1, 200.0, 304.0));
    assert_eq!(r(Trigger::Held), (4, 200.0, 304.0));
    assert_eq!(r(Trigger::Object), (5, 160.0, 180.0));
    assert_eq!(r(Trigger::Ridden), (6, 200.0, 304.0));
    assert_eq!(Trigger::Spot.end_range(), 160.0);
    assert!(Trigger::Spot.start_range().is_infinite());
    let k = |t: PartTrigger| Source::Part(t).trigger().kind();
    // pipes go through the warp sequence (kind 5), lifts are stood on (6)
    assert_eq!(k(PartTrigger::Entered), 5);
    assert_eq!(k(PartTrigger::Out), 5);
    assert_eq!(k(PartTrigger::Ridden), 6);
    assert_eq!(k(PartTrigger::Collected), 4);
    assert_eq!(k(PartTrigger::Caught), 4);
    assert_eq!(k(PartTrigger::Shot), 1);
}

#[test]
fn a_filter_lasts_sixteen_beats_of_the_course_music() {
    assert_eq!(filter_frames(120.0), 480);
    assert_eq!(filter_frames(100.0), 576);
    assert_eq!(filter_frames(112.5), 512);
    assert_eq!(filter_frames(0.0), 480, "the 120 bpm default");
    let c = course(vec![spot(46, 8, 1)], vec![]);
    let mut w = World::from_course(&c);
    let mut s = SoundParts::new(&c);
    s.set_tempo(0, [150.0, 200.0]);
    let mut start = None;
    let mut end = None;
    for _ in 0..800 {
        w.step(Input {
            buttons: if start.is_none() { input::RIGHT } else { 0 },
        });
        s.step(&w);
        if s.cues.iter().any(|c| c.id == 46) {
            assert!(start.is_none(), "one start");
            start = Some(w.tick);
            assert_eq!(
                s.filters,
                vec![Filter {
                    id: 46,
                    left: 384,
                    age: 0
                }]
            );
        }
        if s.filters_ended.contains(&46) {
            end = Some(w.tick);
            assert!(s.filters.is_empty());
        }
    }
    let (start, end) = (start.expect("set off"), end.expect("ran out"));
    assert_eq!(end - start, 384, "16 beats at 150 bpm");
}

#[test]
fn a_lift_plays_once_a_landing() {
    let c = course(vec![], vec![object(11, 12, 3, 0)]);
    let mut w = World::from_course(&c);
    let mut s = SoundParts::new(&c);
    run(&mut w, &mut s, 0, 2);
    let lift = w
        .actors
        .iter()
        .find(|a| matches!(a.kind, Kind::Lift { .. }))
        .map(|a| a.id)
        .expect("the lift");
    let tick = |w: &mut World, s: &mut SoundParts, on: bool| {
        w.player.ride = on.then_some(lift);
        w.player.on_ground = true;
        w.tick += 1;
        s.step(w);
        s.cues.iter().map(|c| (c.id, c.source)).collect::<Vec<_>>()
    };
    assert_eq!(
        tick(&mut w, &mut s, true),
        vec![(0, Source::Part(PartTrigger::Ridden))]
    );
    assert!(tick(&mut w, &mut s, true).is_empty(), "standing on");
    // off and on again inside the 8 frames: nothing; after them: again
    tick(&mut w, &mut s, false);
    assert!(tick(&mut w, &mut s, true).is_empty());
    for _ in 0..8 {
        tick(&mut w, &mut s, false);
    }
    assert_eq!(tick(&mut w, &mut s, true).len(), 1);
}

#[test]
fn a_key_and_a_bill_blaster() {
    let c = course(vec![], vec![object(95, 8, 3, 2), object(13, 12, 1, 6)]);
    let mut w = World::from_course(&c);
    let mut s = SoundParts::new(&c);
    run(&mut w, &mut s, 0, 2);
    let key = w.actors.iter().position(|a| a.kind == Kind::Key).unwrap();
    w.actors[key].mode = crate::actors::Mode::Dead;
    w.tick += 1;
    s.step(&w);
    assert_eq!(
        s.cues.iter().map(|c| (c.id, c.source)).collect::<Vec<_>>(),
        vec![(2, Source::Part(PartTrigger::Collected))]
    );
    // a shot leaves the blaster: Laughter at its muzzle (2 tiles tall: 8 px under its top)
    let b = w.level.blasters[0];
    let mut shot = crate::actors::Actor::new(Kind::Goomba, b.x, b.y + 16.0);
    shot.id = 9999;
    shot.blaster = Some(0);
    w.actors.push(shot);
    w.tick += 1;
    s.step(&w);
    let cue = s
        .cues
        .iter()
        .find(|c| c.id == 6)
        .expect("the shot's effect");
    assert_eq!(cue.source, Source::Part(PartTrigger::Shot));
    assert_eq!((cue.x, cue.y), (b.x, b.y + b.h as f32 * 16.0 - 8.0));
    w.tick += 1;
    s.step(&w);
    assert!(s.cues.is_empty(), "one effect a shot");
}

#[test]
fn an_enemy_s_music_plays_while_it_is_near_and_follows_it() {
    // a Goomba with Boss Music 5 tiles from the start
    let c = course(vec![], vec![object(0, 6, 1, 9)]);
    let mut w = World::from_course(&c);
    let mut s = SoundParts::new(&c);
    run(&mut w, &mut s, 0, 2);
    let m = s
        .music
        .expect("within 200 px: the source registry starts it");
    assert_eq!((m.id, m.trigger), (9, Trigger::Enemy));
    let g = w
        .actors
        .iter()
        .position(|a| a.kind == Kind::Goomba)
        .unwrap();
    assert_eq!(m.actor, Some(w.actors[g].id));
    w.actors[g].x += 50.0;
    w.tick += 1;
    s.step(&w);
    assert_eq!(s.music.map(|m| m.x), Some(w.actors[g].x), "follows");
    // defeated, its music stays where it was; 304 px from there it is over
    w.actors[g].mode = crate::actors::Mode::Dead;
    w.tick += 1;
    s.step(&w);
    assert!(s.music.is_some());
    w.player.x += 400.0;
    w.tick += 1;
    s.step(&w);
    assert!(s.music.is_none());
}

#[test]
fn a_moving_block_and_a_clown_car() {
    // a winged brick (actor flags 0x2) and a Koopa Clown Car
    let mut brick = object(4, 10, 4, 3);
    brick.flags |= 0x2;
    let c = course(vec![], vec![brick, object(42, 16, 3, 14)]);
    let mut w = World::from_course(&c);
    let mut s = SoundParts::new(&c);
    run(&mut w, &mut s, 0, 2);
    let block = w
        .actors
        .iter()
        .find(|a| matches!(a.kind, Kind::MovingBlock { .. }))
        .map(|a| a.id)
        .expect("the moving block");
    let car = w
        .actors
        .iter()
        .find(|a| matches!(a.kind, Kind::ClownCar { .. }))
        .map(|a| a.id)
        .expect("the car");
    let cues = |w: &mut World, s: &mut SoundParts| {
        w.tick += 1;
        s.step(w);
        s.cues.iter().map(|c| (c.id, c.source)).collect::<Vec<_>>()
    };
    w.player.head_solid = Some(block);
    assert_eq!(
        cues(&mut w, &mut s),
        vec![(3, Source::Part(PartTrigger::Hit))]
    );
    assert!(cues(&mut w, &mut s).is_empty(), "one hit");
    w.player.head_solid = None;
    w.player.clown = Some(car);
    assert_eq!(
        cues(&mut w, &mut s),
        vec![(14, Source::Part(PartTrigger::Boarded))]
    );
    assert_eq!(Source::Part(PartTrigger::Boarded).trigger().kind(), 1);
}

#[test]
fn a_pow_sets_off_only_a_music_effect_while_it_is_there() {
    // a POW with Shock and one with Boss Music, near the start
    let c = course(vec![], vec![object(19, 5, 1, 0), object(19, 7, 1, 9)]);
    let mut w = World::from_course(&c);
    let mut s = SoundParts::new(&c);
    let cues = run(&mut w, &mut s, 0, 3);
    assert!(cues.iter().all(|c| c.id == 9), "{cues:?}");
    assert_eq!(
        s.music.map(|m| (m.id, m.trigger)),
        Some((9, Trigger::Enemy))
    );
}
