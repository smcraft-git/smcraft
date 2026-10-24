use super::*;
use crate::Saves;
use crate::layout::UiKind;
use crate::play::tests::{MockHost, with};

/// A UI import without layouts: the maker's logic runs, nothing is drawn.
fn bare_ui() -> Ui {
    let mut files = mmk_ui::MemFiles::new("test");
    files.insert("manifest.json", br#"{"game": "SMM2"}"#.to_vec());
    files.insert("ui/layouts/None.json", b"{}".to_vec());
    let cache = mmk_ui::UiCache::from_files(Box::new(files)).expect("empty import");
    Ui::new(UiKind::Smm2, cache, None)
}

fn ui() -> Ui {
    crate::testing::smm2().unwrap_or_else(bare_ui)
}

fn press(b: Buttons) -> MakerInput {
    MakerInput {
        ui: UiInput {
            pressed: b,
            held: b,
            ..Default::default()
        },
        ..Default::default()
    }
}

fn held(b: Buttons) -> MakerInput {
    MakerInput {
        ui: UiInput {
            held: b,
            ..Default::default()
        },
        ..Default::default()
    }
}

fn keys(k: u16) -> MakerInput {
    MakerInput {
        keys: k,
        ..Default::default()
    }
}

fn idle() -> MakerInput {
    MakerInput::default()
}

fn run(m: &mut MakerFlow, host: &mut MockHost, saves: &mut Saves, i: MakerInput) {
    with(host, saves, |cx| {
        m.step(&i, cx);
    });
}

fn maker(host: &mut MockHost, saves: &mut Saves, start: MakerStart) -> MakerFlow {
    let ui = ui();
    with(host, saves, |cx| MakerFlow::new(&ui, cx, start))
}

#[test]
fn a_new_course_is_shown_and_parts_go_down_where_the_cursor_is() {
    let (mut host, mut saves) = (MockHost::default(), Saves::default());
    let mut m = maker(&mut host, &mut saves, MakerStart::New("M1".into()));
    assert!(m.shown && host.shows == 1, "the host draws the course");
    assert!(m.editor.has_ground(20, 0), "one row of ground");
    assert_eq!(m.tool(), Tool::Ground);
    // the Goomba slot, the cursor two tiles up, confirm
    let goomba = m.palette.iter().position(|&t| t == Tool::Part(0)).unwrap();
    m.slot = goomba;
    m.cur = (12, 3);
    run(&mut m, &mut host, &mut saves, press(Buttons::CONFIRM));
    run(&mut m, &mut host, &mut saves, idle());
    let i = m.editor.object_at(12, 3).expect("a Goomba");
    assert_eq!(m.editor.object(i).unwrap().id, 0);
    assert!(host.shows >= 2, "shown again after the change");
    // undo takes it away, one step for the press
    run(&mut m, &mut host, &mut saves, keys(keys::UNDO));
    run(&mut m, &mut host, &mut saves, idle());
    assert!(m.editor.object_at(12, 3).is_none());
}

#[test]
fn ground_paints_while_held_and_back_erases() {
    let (mut host, mut saves) = (MockHost::default(), Saves::default());
    let mut m = maker(&mut host, &mut saves, MakerStart::New("M1".into()));
    m.cur = (20, 4);
    run(&mut m, &mut host, &mut saves, press(Buttons::CONFIRM));
    // hold confirm and move right: the cursor steps a tile on the press, then repeats
    for t in 0..20 {
        let mut i = held(Buttons::CONFIRM.union(Buttons::RIGHT));
        if t == 0 {
            i.ui.pressed = Buttons::RIGHT;
        }
        run(&mut m, &mut host, &mut saves, i);
    }
    run(&mut m, &mut host, &mut saves, idle());
    assert!(m.editor.has_ground(20, 4) && m.editor.has_ground(21, 4));
    let painted = (20..30).filter(|&x| m.editor.has_ground(x, 4)).count();
    assert!(painted >= 3, "{painted}");
    // one undo step for the whole stroke
    run(&mut m, &mut host, &mut saves, keys(keys::UNDO));
    run(&mut m, &mut host, &mut saves, idle());
    assert!(!m.editor.has_ground(20, 4) && !m.editor.has_ground(21, 4));
    run(&mut m, &mut host, &mut saves, keys(keys::REDO));
    run(&mut m, &mut host, &mut saves, idle());
    assert!(m.editor.has_ground(20, 4));
    // back held erases under the cursor
    m.cur = (20, 4);
    run(&mut m, &mut host, &mut saves, press(Buttons::BACK));
    run(&mut m, &mut host, &mut saves, idle());
    assert!(!m.editor.has_ground(20, 4));
}

#[test]
fn a_part_picked_up_moves_with_the_cursor() {
    let (mut host, mut saves) = (MockHost::default(), Saves::default());
    let mut m = maker(&mut host, &mut saves, MakerStart::New("M1".into()));
    m.editor.place(5, 15, 4).unwrap();
    m.cur = (15, 4);
    run(&mut m, &mut host, &mut saves, press(Buttons::CONFIRM));
    assert!(m.carrying());
    m.cur = (18, 5);
    run(&mut m, &mut host, &mut saves, held(Buttons::CONFIRM));
    run(&mut m, &mut host, &mut saves, idle());
    assert!(m.editor.object_at(15, 4).is_none());
    assert!(m.editor.object_at(18, 5).is_some(), "the ? Block moved");
}

#[test]
fn the_part_window_puts_its_choice_at_the_front_of_the_palette() {
    let (mut host, mut saves) = (MockHost::default(), Saves::default());
    let mut m = maker(&mut host, &mut saves, MakerStart::New("M1".into()));
    run(&mut m, &mut host, &mut saves, keys(keys::PARTS));
    assert_eq!(m.mode, Mode::Ring);
    // Ground is the first Terrain part; one to the right is the gentle slope
    run(&mut m, &mut host, &mut saves, idle());
    run(&mut m, &mut host, &mut saves, press(Buttons::RIGHT));
    run(&mut m, &mut host, &mut saves, press(Buttons::CONFIRM));
    assert_eq!(m.mode, Mode::Edit);
    assert_eq!(m.palette[0], Tool::Part(87));
    assert_eq!(m.slot, 0);
    assert_eq!(m.palette.len(), SLOTS, "the oldest part dropped out");
    // down pages on to the next category
    run(&mut m, &mut host, &mut saves, keys(keys::PARTS));
    run(&mut m, &mut host, &mut saves, idle());
    let cat = m.ring.category;
    for _ in 0..4 {
        run(&mut m, &mut host, &mut saves, press(Buttons::DOWN));
        run(&mut m, &mut host, &mut saves, idle());
    }
    assert_ne!(m.ring.category, cat);
    run(&mut m, &mut host, &mut saves, press(Buttons::BACK));
    assert_eq!(m.mode, Mode::Edit);
}

#[test]
fn test_play_starts_where_asked_and_comes_back() {
    let (mut host, mut saves) = (MockHost::default(), Saves::default());
    let mut m = maker(&mut host, &mut saves, MakerStart::New("M1".into()));
    // a tap of the test-play key (once it was up): from Mario (the start at first)
    run(&mut m, &mut host, &mut saves, idle());
    let mut i = idle();
    i.play = true;
    run(&mut m, &mut host, &mut saves, i);
    run(&mut m, &mut host, &mut saves, idle());
    assert!(m.testing());
    assert_eq!(host.tests, vec![(0, None)]);
    run(&mut m, &mut host, &mut saves, idle());
    assert!(host.steps >= 1, "the engine runs");
    // the key again: back to the maker
    run(&mut m, &mut host, &mut saves, i);
    assert!(!m.testing());
    // from the cursor
    m.cur = (30, 2);
    run(&mut m, &mut host, &mut saves, keys(keys::PLAY_HERE));
    assert_eq!(
        host.tests.last(),
        Some(&(0, Some([30.0 * 16.0 + 8.0, 32.0])))
    );
    // a death ends it
    host.status.dying = true;
    run(&mut m, &mut host, &mut saves, idle());
    host.status.dying = false;
    run(&mut m, &mut host, &mut saves, idle());
    assert!(!m.testing());
}

#[test]
fn saving_keeps_the_entry_and_copies_of_others_courses_are_new_and_credited() {
    let (mut host, mut saves) = (MockHost::default(), Saves::default());
    let mut m = maker(&mut host, &mut saves, MakerStart::New("M1".into()));
    with(&mut host, &mut saves, |cx| {
        m.save_to_coursebot(cx.host, false)
    })
    .unwrap();
    assert_eq!(m.entry.as_deref(), Some("slot:0"));
    with(&mut host, &mut saves, |cx| {
        m.save_to_coursebot(cx.host, false)
    })
    .unwrap();
    assert_eq!(
        host.saved[1].0.as_deref(),
        Some("slot:0"),
        "overwrites its entry"
    );
    // someone else's course
    let theirs = new_course("M3").save().unwrap();
    host.courses.insert("lib/theirs".into(), theirs);
    let mut m = maker(
        &mut host,
        &mut saves,
        MakerStart::Open {
            source: Source::Library("lib/theirs".into()),
            entry: Some("lib/theirs".into()),
            based_on: Some("based on ABC-DEF-GHJ by Someone".into()),
        },
    );
    assert_eq!(m.style(), "M3");
    with(&mut host, &mut saves, |cx| {
        m.save_to_coursebot(cx.host, false)
    })
    .unwrap();
    let (id, data) = host.saved.last().unwrap();
    assert_eq!(
        id.as_deref(),
        Some("slot:1"),
        "a new course, not over theirs"
    );
    let c = Editor::open(data).unwrap();
    assert!(
        c.course
            .header
            .description
            .to_string_lossy()
            .contains("based on ABC-DEF-GHJ by Someone")
    );
    // the draft reopens with the maker
    let m2 = maker(&mut host, &mut saves, MakerStart::Resume);
    assert_eq!(m2.style(), "M3");
    assert_eq!(m2.entry, m.entry);
}

#[test]
fn course_data_survives_the_draft_text() {
    let b = new_course("MW").save().unwrap();
    let t = hex_rle(&b);
    assert!(t.len() < b.len() / 20, "{} bytes", t.len());
    assert_eq!(un_hex_rle(&t).unwrap(), b);
}

#[test]
fn layouts_and_view_coordinates() {
    let (mut host, mut saves) = (MockHost::default(), Saves::default());
    let mut m = maker(&mut host, &mut saves, MakerStart::New("M1".into()));
    // a tile's centre on screen and back
    let p = m.tile_center((3, 2));
    assert_eq!(m.tile_at(p), (3, 2));
    // the cursor pushing right scrolls the view
    m.cur = (21, 2);
    m.follow_cursor();
    assert!(m.cam[0] > 0.0);
    let mut out = Vec::new();
    m.draw(&mut out);
    assert!(matches!(out[0], DrawItem::Course));
    if crate::testing::smm2().is_some() {
        assert!(m.page.is_some(), "Edit_MainPage_00 in the import");
        assert!(out.len() >= 3);
    }
}

#[test]
fn make_save_and_find_it_in_the_coursebot() {
    let Some(ui) = crate::testing::smm2() else {
        return;
    };
    let mut host = MockHost::default();
    let opts = crate::Options {
        boot: false,
        instant_fades: true,
        ..Default::default()
    };
    let mut f = crate::Front::new(ui, None, opts, &mut host);
    assert!(f.capture(&mut host, "maker", 0, 0));
    let tick = |f: &mut crate::Front, host: &mut MockHost, i: crate::FrontInput| {
        f.tick(&i, host);
        f.tick(&crate::FrontInput::default(), host);
    };
    // a Goomba where the cursor is, then save
    let crate::Screen::Maker(m) = &mut f.screen else {
        panic!("the maker");
    };
    m.slot = m.palette.iter().position(|&t| t == Tool::Part(0)).unwrap();
    m.cur = (14, 1);
    m.editor.set_name("Goomba walk");
    let press = |b| crate::FrontInput {
        menu: b,
        ..Default::default()
    };
    tick(&mut f, &mut host, press(Buttons::CONFIRM));
    let crate::Screen::Maker(m) = &mut f.screen else {
        panic!("still the maker");
    };
    assert!(m.editor.object_at(14, 1).is_some());
    let saves = &mut Saves::default();
    with(&mut host, saves, |cx| m.save_to_coursebot(cx.host, false)).unwrap();
    // + → main menu, right → Coursebot, confirm: the Coursebot list with the course in it
    let start = crate::FrontInput {
        start: true,
        ..Default::default()
    };
    tick(&mut f, &mut host, start);
    tick(&mut f, &mut host, press(Buttons::RIGHT));
    tick(&mut f, &mut host, press(Buttons::CONFIRM));
    for _ in 0..120 {
        f.tick(&crate::FrontInput::default(), &mut host);
    }
    let crate::Screen::Coursebot(b) = &f.screen else {
        panic!("the Coursebot");
    };
    assert_eq!(
        b.scene.course(0).map(|e| e.name.as_str()),
        Some("Goomba walk")
    );
}

#[test]
fn the_left_column_windows_change_timer_autoscroll_style_and_theme() {
    let (mut host, mut saves) = (MockHost::default(), Saves::default());
    let mut m = maker(&mut host, &mut saves, MakerStart::New("M1".into()));
    m.open_window(2);
    for b in [Buttons::UP, Buttons::UP, Buttons::RIGHT, Buttons::CONFIRM] {
        run(&mut m, &mut host, &mut saves, press(b));
        run(&mut m, &mut host, &mut saves, idle());
    }
    assert_eq!(m.editor.course.header.timer, 420);
    assert_eq!(m.mode, Mode::Edit);
    m.open_window(3);
    for b in [Buttons::DOWN, Buttons::DOWN, Buttons::CONFIRM] {
        run(&mut m, &mut host, &mut saves, press(b));
        run(&mut m, &mut host, &mut saves, idle());
    }
    assert_eq!(m.editor.area().header.autoscroll_type, 2, "normal");
    m.open_window(0);
    for b in [Buttons::RIGHT, Buttons::CONFIRM] {
        run(&mut m, &mut host, &mut saves, press(b));
        run(&mut m, &mut host, &mut saves, idle());
    }
    assert_eq!(m.style(), "M3");
    m.open_window(1);
    for b in [Buttons::DOWN, Buttons::CONFIRM] {
        run(&mut m, &mut host, &mut saves, press(b));
        run(&mut m, &mut host, &mut saves, idle());
    }
    assert_eq!(m.editor.theme().0, 1, "underground, under ground");
    // back out of a window changes nothing
    m.open_window(2);
    run(&mut m, &mut host, &mut saves, press(Buttons::UP));
    run(&mut m, &mut host, &mut saves, press(Buttons::BACK));
    assert_eq!(m.editor.course.header.timer, 420);
}

#[test]
fn the_scroll_bar_jumps_to_the_goal_and_back() {
    let Some(ui) = crate::testing::smm2() else {
        return;
    };
    let (mut host, mut saves) = (MockHost::default(), Saves::default());
    let mut m = with(&mut host, &mut saves, |cx| {
        MakerFlow::new(&ui, cx, MakerStart::New("M1".into()))
    });
    for _ in 0..30 {
        run(&mut m, &mut host, &mut saves, idle());
    }
    let centre = |m: &mut MakerFlow, pane: &str| {
        let p = m.page.as_mut().unwrap();
        p.apply(m.tick);
        p.rect(pane).unwrap().center
    };
    let g = centre(&mut m, "L_Slider_00/L_BtnG_00/B_Btn_00");
    let click = MakerInput {
        ui: UiInput {
            click: Some(g),
            ..Default::default()
        },
        ..Default::default()
    };
    run(&mut m, &mut host, &mut saves, click);
    assert_eq!(m.cam[0], 240.0 * 16.0 - VIEW_W, "at the goal");
    let s = centre(&mut m, "L_Slider_00/L_BtnS_00/B_Btn_00");
    let click = MakerInput {
        ui: UiInput {
            click: Some(s),
            ..Default::default()
        },
        ..Default::default()
    };
    run(&mut m, &mut host, &mut saves, click);
    assert_eq!(m.cam[0], 0.0);
}

#[test]
fn upload_sends_the_saved_course_with_its_credit_and_load_goes_to_the_coursebot() {
    let (mut host, mut saves) = (MockHost::default(), Saves::default());
    host.server = true;
    let theirs = new_course("MW").save().unwrap();
    host.courses.insert("lib/theirs".into(), theirs);
    let mut m = maker(
        &mut host,
        &mut saves,
        MakerStart::Open {
            source: Source::Library("lib/theirs".into()),
            entry: None,
            based_on: Some("based on XYZ-123-456 by Them".into()),
        },
    );
    // a new copy: Save As (the Coursebot picks the slot) and Load only
    m.open_save();
    assert_eq!(m.save_buttons(), vec!["L_NSaveBtn_00", "L_LoadBtn_00"]);
    let mut ev = None;
    with(&mut host, &mut saves, |cx| {
        ev = m.step(&press(Buttons::CONFIRM), cx)
    });
    assert_eq!(ev, Some(MakerEvent::SaveAs));
    with(&mut host, &mut saves, |cx| m.save_to_slot(cx.host, 3)).unwrap();
    assert_eq!(host.saved.len(), 1);
    assert_eq!(m.entry.as_deref(), Some("slot:3"));
    // now ours: Save, Save As, Upload, Load
    m.open_save();
    assert_eq!(m.save_buttons().len(), 4);
    for b in [Buttons::DOWN, Buttons::DOWN, Buttons::CONFIRM] {
        run(&mut m, &mut host, &mut saves, press(b));
        run(&mut m, &mut host, &mut saves, idle());
    }
    let call = host
        .calls
        .iter()
        .find(|c| c.1 == "upload")
        .expect("uploaded");
    let data = un_hex_rle(call.2["course"].as_str().unwrap()).unwrap();
    let c = Editor::open(&data).unwrap();
    assert!(
        c.course
            .header
            .description
            .to_string_lossy()
            .contains("XYZ-123-456")
    );
    // Load
    m.open_save();
    let mut ev = None;
    for b in [Buttons::UP, Buttons::CONFIRM] {
        with(&mut host, &mut saves, |cx| ev = m.step(&press(b), cx));
        run(&mut m, &mut host, &mut saves, idle());
        if ev.is_some() {
            break;
        }
    }
    assert_eq!(ev, Some(MakerEvent::Load));
}

#[test]
fn items_go_into_blocks_and_a_mushroom_makes_an_enemy_big() {
    let (mut host, mut saves) = (MockHost::default(), Saves::default());
    let mut m = maker(&mut host, &mut saves, MakerStart::New("M1".into()));
    let q = m.editor.place(5, 15, 4).unwrap();
    let g = m.editor.place(0, 20, 1).unwrap();
    // the Super Mushroom slot on the ? block: inside; on the Goomba: a big Goomba
    m.slot = m.palette.iter().position(|&t| t == Tool::Part(20)).unwrap();
    m.cur = (15, 4);
    run(&mut m, &mut host, &mut saves, press(Buttons::CONFIRM));
    run(&mut m, &mut host, &mut saves, idle());
    assert_eq!(m.editor.object(q).unwrap().child_id, 20);
    m.cur = (20, 1);
    run(&mut m, &mut host, &mut saves, press(Buttons::CONFIRM));
    run(&mut m, &mut host, &mut saves, idle());
    assert!(m.editor.object(g).unwrap().is_big());
    // one undo step each
    run(&mut m, &mut host, &mut saves, keys(keys::UNDO));
    run(&mut m, &mut host, &mut saves, idle());
    assert!(!m.editor.object(g).unwrap().is_big());
    // a coin carried onto a brick goes inside it
    let b = m.editor.place(4, 30, 4).unwrap();
    m.editor.place(8, 33, 4).unwrap();
    m.cur = (33, 4);
    run(&mut m, &mut host, &mut saves, press(Buttons::CONFIRM));
    assert!(m.carrying());
    m.cur = (30, 4);
    run(&mut m, &mut host, &mut saves, held(Buttons::CONFIRM));
    run(&mut m, &mut host, &mut saves, idle());
    let brick = m.editor.object_at(30, 4).unwrap();
    assert_eq!(m.editor.object(brick).unwrap().child_id, 8, "{b}");
    assert!(m.editor.object_at(33, 4).is_none(), "the coin went in");
}

#[test]
fn the_clear_condition_window_sets_and_removes_the_condition() {
    let Some(ui) = crate::testing::smm2() else {
        return;
    };
    let (mut host, mut saves) = (MockHost::default(), Saves::default());
    let mut m = with(&mut host, &mut saves, |cx| {
        MakerFlow::new(&ui, cx, MakerStart::New("M1".into()))
    });
    m.open_mission();
    assert_eq!(m.mode, Mode::Mission);
    // Parts: Goombas (the first), amount 1 → 3, OK
    for b in [Buttons::RIGHT, Buttons::UP, Buttons::UP, Buttons::CONFIRM] {
        run(&mut m, &mut host, &mut saves, press(b));
        run(&mut m, &mut host, &mut saves, idle());
    }
    assert_eq!(m.mode, Mode::Edit);
    assert_eq!(m.clear_condition(), (1, 1, 3));
    // Actions (L): without taking damage
    m.open_mission();
    run(&mut m, &mut host, &mut saves, keys(keys::PREV));
    run(&mut m, &mut host, &mut saves, press(Buttons::CONFIRM));
    assert_eq!(m.clear_condition(), (3, 70, 0));
    // Remove (L from Actions wraps to it)
    m.open_mission();
    run(&mut m, &mut host, &mut saves, keys(keys::PREV));
    run(&mut m, &mut host, &mut saves, idle());
    run(&mut m, &mut host, &mut saves, press(Buttons::CONFIRM));
    assert_eq!(m.clear_condition(), (0, 0, 0));
    // one undo brings the action back
    run(&mut m, &mut host, &mut saves, keys(keys::UNDO));
    assert_eq!(m.clear_condition(), (3, 70, 0));
}

#[test]
fn the_start_and_goal_drag_their_heights_and_the_course_length() {
    let (mut host, mut saves) = (MockHost::default(), Saves::default());
    let mut m = maker(&mut host, &mut saves, MakerStart::New("M1".into()));
    m.cur = (2, 1);
    run(&mut m, &mut host, &mut saves, press(Buttons::CONFIRM));
    m.cur = (2, 5);
    run(&mut m, &mut host, &mut saves, held(Buttons::CONFIRM));
    run(&mut m, &mut host, &mut saves, idle());
    assert_eq!(m.editor.course.header.start_y, 5);
    let start: Vec<&str> = host
        .edit_sounds
        .iter()
        .filter(|(u, _)| u == "M1_EditGroundStart")
        .map(|(_, k)| k.as_str())
        .collect();
    assert_eq!(start, ["drag", "move", "set"], "picked up, moved, put down");
    // the goal ground starts 10 tiles from the right edge of a 240-tile course
    m.cur = (233, 1);
    run(&mut m, &mut host, &mut saves, press(Buttons::CONFIRM));
    m.cur = (213, 3);
    run(&mut m, &mut host, &mut saves, held(Buttons::CONFIRM));
    run(&mut m, &mut host, &mut saves, idle());
    assert_eq!(m.editor.size().0, 220, "20 tiles shorter");
    assert_eq!(m.editor.course.header.goal_y, 3);
    // one undo step per drag
    run(&mut m, &mut host, &mut saves, keys(keys::UNDO));
    run(&mut m, &mut host, &mut saves, idle());
    assert_eq!(m.editor.size().0, 240);
    assert_eq!(m.editor.course.header.start_y, 5);
}

#[test]
fn save_as_picks_a_coursebot_slot_and_comes_back_and_load_opens_a_course() {
    let Some(ui) = crate::testing::smm2() else {
        return;
    };
    let mut host = MockHost::default();
    let opts = crate::Options {
        boot: false,
        instant_fades: true,
        ..Default::default()
    };
    let mut f = crate::Front::new(ui, None, opts, &mut host);
    assert!(f.capture(&mut host, "maker", 0, 0));
    let tick = |f: &mut crate::Front, host: &mut MockHost, b: Buttons| {
        let i = crate::FrontInput {
            menu: b,
            ..Default::default()
        };
        f.tick(&i, host);
        f.tick(&crate::FrontInput::default(), host);
    };
    if let crate::Screen::Maker(m) = &mut f.screen {
        m.editor.set_name("Slot test");
        m.open_save();
    }
    // Save As (the only save button of a new course, first) → the Coursebot on slot 0
    tick(&mut f, &mut host, Buttons::CONFIRM);
    let crate::Screen::Coursebot(b) = &f.screen else {
        panic!("the Coursebot");
    };
    assert_eq!(b.scene.mode, crate::coursebot::BotMode::Save);
    tick(&mut f, &mut host, Buttons::CONFIRM);
    let crate::Screen::Maker(m) = &f.screen else {
        panic!("back in the maker");
    };
    assert_eq!(m.entry.as_deref(), Some("slot:0"));
    assert!(host.courses.contains_key("slot:0"));
    // Load: the Coursebot on "Select a Course to Load", the course, Load → the maker with it
    if let crate::Screen::Maker(m) = &mut f.screen {
        m.editor.set_name("Changed");
        m.open_save();
        m.save_sel = m.save_buttons().len() - 1;
    }
    tick(&mut f, &mut host, Buttons::CONFIRM);
    let crate::Screen::Coursebot(b) = &f.screen else {
        panic!("the Coursebot to load from");
    };
    assert_eq!(b.scene.mode, crate::coursebot::BotMode::Load);
    tick(&mut f, &mut host, Buttons::CONFIRM);
    tick(&mut f, &mut host, Buttons::CONFIRM);
    let crate::Screen::Maker(m) = &f.screen else {
        panic!("the maker with the loaded course");
    };
    assert_eq!(m.editor.course.header.name.to_string_lossy(), "Slot test");
    assert_eq!(m.entry.as_deref(), Some("slot:0"));
}

#[test]
fn a_finger_paints_and_its_closing_tap_does_nothing_more() {
    let (mut host, mut saves) = (MockHost::default(), Saves::default());
    let mut m = maker(&mut host, &mut saves, MakerStart::New("M1".into()));
    run(&mut m, &mut host, &mut saves, idle());
    let at = |m: &MakerFlow, t: (i32, i32)| m.tile_center(t);
    let finger = |p: [f32; 2]| MakerInput {
        pointer: Some(p),
        down: true,
        ..Default::default()
    };
    // ground (slot 0) painted along the drag
    let p = at(&m, (20, 5));
    run(&mut m, &mut host, &mut saves, finger(p));
    for x in 21..24 {
        let p = at(&m, (x, 5));
        run(&mut m, &mut host, &mut saves, finger(p));
    }
    run(&mut m, &mut host, &mut saves, idle());
    assert!((20..24).all(|x| m.editor.has_ground(x, 5)));
    // the tap the touch ends with lands where the finger started: ignored (it would erase
    // nothing here but would place a second time)
    let tap = MakerInput {
        ui: UiInput {
            click: Some(at(&m, (26, 5))),
            ..Default::default()
        },
        ..Default::default()
    };
    run(&mut m, &mut host, &mut saves, tap);
    assert!(!m.editor.has_ground(26, 5));
    // a lone tap later places
    for _ in 0..TAP_AFTER {
        run(&mut m, &mut host, &mut saves, idle());
    }
    run(&mut m, &mut host, &mut saves, tap);
    assert!(m.editor.has_ground(26, 5));
}

#[test]
fn semisolids_and_pipes_stretch_by_their_edges() {
    let (mut host, mut saves) = (MockHost::default(), Saves::default());
    let mut m = maker(&mut host, &mut saves, MakerStart::New("M1".into()));
    let s = m.editor.place(16, 20, 1).unwrap();
    // the top-right corner tile: both ways
    m.cur = (22, 3);
    run(&mut m, &mut host, &mut saves, press(Buttons::CONFIRM));
    m.cur = (25, 6);
    run(&mut m, &mut host, &mut saves, held(Buttons::CONFIRM));
    run(&mut m, &mut host, &mut saves, idle());
    let o = m.editor.object(s).unwrap();
    assert_eq!((o.width, o.height), (6, 6));
    let heard: Vec<&str> = host
        .edit_sounds
        .iter()
        .filter(|(u, _)| u == "M1_EditGroundBox")
        .map(|(_, k)| k.as_str())
        .collect();
    assert_eq!(
        heard,
        ["drag", "move", "set"],
        "picked by its edge, stretched, let go"
    );
    // a pipe's top: longer
    m.slot = m.palette.iter().position(|&t| t == Tool::Pipe).unwrap();
    m.cur = (40, 1);
    run(&mut m, &mut host, &mut saves, press(Buttons::CONFIRM));
    run(&mut m, &mut host, &mut saves, idle());
    let p = m.editor.object_at(40, 1).unwrap();
    m.cur = (40, 2);
    run(&mut m, &mut host, &mut saves, press(Buttons::CONFIRM));
    m.cur = (40, 6);
    run(&mut m, &mut host, &mut saves, held(Buttons::CONFIRM));
    run(&mut m, &mut host, &mut saves, idle());
    assert_eq!(m.editor.object(p).unwrap().height, 6);
    // one undo step for the stretch
    run(&mut m, &mut host, &mut saves, keys(keys::UNDO));
    run(&mut m, &mut host, &mut saves, idle());
    assert_eq!(m.editor.object(p).unwrap().height, 2);
}

#[test]
fn multigrab_selects_moves_copies_and_erases() {
    let (mut host, mut saves) = (MockHost::default(), Saves::default());
    let mut m = maker(&mut host, &mut saves, MakerStart::New("M1".into()));
    for x in 20..23 {
        m.editor.place(4, x, 4).unwrap();
    }
    let multi = |b: Buttons, pressed: bool| MakerInput {
        ui: UiInput {
            pressed: if pressed { b } else { Buttons::NONE },
            held: b,
            ..Default::default()
        },
        keys: keys::MULTI,
        ..Default::default()
    };
    m.cur = (20, 4);
    run(&mut m, &mut host, &mut saves, multi(Buttons::CONFIRM, true));
    m.cur = (22, 4);
    run(
        &mut m,
        &mut host,
        &mut saves,
        multi(Buttons::CONFIRM, false),
    );
    run(&mut m, &mut host, &mut saves, idle());
    assert_eq!(m.selection, Some(Rect::new(20, 4, 3, 1)));
    // carried up two rows
    m.cur = (21, 4);
    run(&mut m, &mut host, &mut saves, press(Buttons::CONFIRM));
    m.cur = (21, 6);
    run(&mut m, &mut host, &mut saves, held(Buttons::CONFIRM));
    run(&mut m, &mut host, &mut saves, idle());
    assert!(m.editor.object_at(20, 6).is_some() && m.editor.object_at(20, 4).is_none());
    assert_eq!(m.selection, Some(Rect::new(20, 6, 3, 1)));
    // a copy at the cursor
    m.cur = (30, 4);
    run(&mut m, &mut host, &mut saves, keys(keys::COPY));
    run(&mut m, &mut host, &mut saves, idle());
    assert!((30..33).all(|x| m.editor.object_at(x, 4).is_some()));
    // erase on the selection empties it
    m.cur = (31, 4);
    run(&mut m, &mut host, &mut saves, press(Buttons::BACK));
    run(&mut m, &mut host, &mut saves, idle());
    assert!((30..33).all(|x| m.editor.object_at(x, 4).is_none()));
    assert!(m.editor.object_at(21, 6).is_some(), "the first group stays");
}

#[test]
fn the_maker_plays_its_music_and_the_parts_sounds() {
    let (mut host, mut saves) = (MockHost::default(), Saves::default());
    let mut m = maker(&mut host, &mut saves, MakerStart::New("M1".into()));
    run(&mut m, &mut host, &mut saves, idle());
    run(&mut m, &mut host, &mut saves, idle());
    let edit = |s: &str, t: u8| Some((s.to_string(), t));
    assert_eq!(
        host.music,
        [edit("M1", 0)],
        "the style's edit track, asked once"
    );
    let sounds = |host: &mut MockHost| -> Vec<String> {
        host.edit_sounds
            .drain(..)
            .map(|(u, k)| format!("{u}:{k}"))
            .collect()
    };
    // a Goomba goes down
    m.slot = m.palette.iter().position(|&t| t == Tool::Part(0)).unwrap();
    m.cur = (12, 3);
    run(&mut m, &mut host, &mut saves, press(Buttons::CONFIRM));
    run(&mut m, &mut host, &mut saves, idle());
    assert_eq!(
        sounds(&mut host),
        ["M1_EditKuribo:set", "M1_EditKuribo:voice"]
    );
    // picked up, then dropped two tiles on
    run(&mut m, &mut host, &mut saves, press(Buttons::CONFIRM));
    assert_eq!(sounds(&mut host), ["M1_EditKuribo:drag"]);
    m.cur = (14, 3);
    run(&mut m, &mut host, &mut saves, idle());
    // carried to a new tile (`move`), then put down
    assert_eq!(
        sounds(&mut host),
        ["M1_EditKuribo:move", "M1_EditKuribo:set"]
    );
    // a Super Mushroom on it makes it big
    m.slot = m.palette.iter().position(|&t| t == Tool::Part(20)).unwrap();
    run(&mut m, &mut host, &mut saves, press(Buttons::CONFIRM));
    run(&mut m, &mut host, &mut saves, idle());
    assert_eq!(sounds(&mut host), ["M1_EditKuribo:scaleChange"]);
    // erased
    run(&mut m, &mut host, &mut saves, press(Buttons::BACK));
    run(&mut m, &mut host, &mut saves, idle());
    assert_eq!(sounds(&mut host), ["M1_EditKuribo:eraser"]);
    // a theme switch changes the track
    m.editor.set_theme(2);
    run(&mut m, &mut host, &mut saves, idle());
    assert_eq!(host.music.last(), Some(&edit("M1", 2)));
    // test play stops it (before the course's own music starts); back, it plays again
    run(&mut m, &mut host, &mut saves, keys(keys::PLAY_HERE));
    assert!(m.testing());
    assert_eq!(host.music.last(), Some(&None));
    let mut i = idle();
    i.play = true;
    run(&mut m, &mut host, &mut saves, i);
    assert!(!m.testing());
    assert_eq!(host.music.last(), Some(&edit("M1", 2)));
    // leaving the maker stops it
    let n = host.music.len();
    with(&mut host, &mut saves, |cx| m.leave(cx.host));
    assert_eq!(host.music[n..], [None]);
}

/// Hold confirm on the part under the cursor for `ticks` ticks (the press, then held).
fn hold_on_part(m: &mut MakerFlow, host: &mut MockHost, saves: &mut Saves, ticks: i32) {
    run(m, host, saves, press(Buttons::CONFIRM));
    for _ in 0..ticks {
        run(m, host, saves, held(Buttons::CONFIRM));
    }
}

#[test]
fn holding_a_part_still_opens_its_balloon_and_wings_and_a_parachute_go_on() {
    let (mut host, mut saves) = (MockHost::default(), Saves::default());
    let mut m = maker(&mut host, &mut saves, MakerStart::New("M1".into()));
    let g = m.editor.place(0, 12, 3).unwrap();
    m.cur = (12, 3);
    // a short hold is still a pick-up
    hold_on_part(&mut m, &mut host, &mut saves, 5);
    assert!(m.carrying() && m.attr_options_shown().is_none());
    // held still long enough: the balloon, with the Goomba's attributes
    for _ in 0..attr::HOLD {
        run(&mut m, &mut host, &mut saves, held(Buttons::CONFIRM));
    }
    let shown = m.attr_options_shown().expect("the balloon is open");
    use mmk_editor::attr::Attr;
    assert_eq!(
        shown,
        [
            Attr::Form(false),
            Attr::Form(true),
            Attr::Big(true),
            Attr::Wings(true),
            Attr::Parachute(true)
        ]
    );
    assert!(!m.carrying(), "the hold no longer carries the part");
    // the press ends (choosing nothing), right twice from the first new choice (the other
    // form) to the wings, confirm
    run(&mut m, &mut host, &mut saves, idle());
    assert!(
        m.attr_options_shown().is_some(),
        "still open after the release"
    );
    host.edit_sounds.clear();
    run(&mut m, &mut host, &mut saves, press(Buttons::RIGHT));
    run(&mut m, &mut host, &mut saves, press(Buttons::RIGHT));
    run(&mut m, &mut host, &mut saves, press(Buttons::CONFIRM));
    run(&mut m, &mut host, &mut saves, idle());
    assert!(m.attr_options_shown().is_none(), "the choice closes it");
    let f = m.editor.object(g).unwrap().flags;
    assert_eq!(f & 0x2, 0x2, "winged");
    assert_eq!(f & 0x8004, 0, "nothing else changed");
    assert!(
        host.edit_sounds
            .iter()
            .any(|(u, k)| u == "M1_EditKuribo" && k == "voice_getWing"),
        "{:?}",
        host.edit_sounds
    );
    // again: now it offers the wings off and the parachute on; back closes without a change
    hold_on_part(&mut m, &mut host, &mut saves, attr::HOLD);
    let shown = m.attr_options_shown().unwrap();
    assert!(shown.contains(&Attr::Wings(false)) && shown.contains(&Attr::Parachute(true)));
    run(&mut m, &mut host, &mut saves, idle());
    run(&mut m, &mut host, &mut saves, press(Buttons::BACK));
    assert!(m.attr_options_shown().is_none());
    assert_eq!(m.editor.object(g).unwrap().flags, f);
    // the parachute (the last button)
    hold_on_part(&mut m, &mut host, &mut saves, attr::HOLD);
    run(&mut m, &mut host, &mut saves, idle());
    for _ in 0..4 {
        run(&mut m, &mut host, &mut saves, press(Buttons::RIGHT));
    }
    run(&mut m, &mut host, &mut saves, press(Buttons::LEFT));
    host.edit_sounds.clear();
    run(&mut m, &mut host, &mut saves, press(Buttons::CONFIRM));
    run(&mut m, &mut host, &mut saves, idle());
    assert_eq!(m.editor.object(g).unwrap().flags & 0x8002, 0x8002);
    assert!(
        host.edit_sounds
            .iter()
            .any(|(u, k)| u == "M1_EditKuribo" && k == "p_EquipPara")
    );
    // one undo step takes the parachute off again, another the wings
    run(&mut m, &mut host, &mut saves, keys(keys::UNDO));
    run(&mut m, &mut host, &mut saves, idle());
    assert_eq!(m.editor.object(g).unwrap().flags & 0x8002, 0x2);
    run(&mut m, &mut host, &mut saves, keys(keys::UNDO));
    run(&mut m, &mut host, &mut saves, idle());
    assert_eq!(m.editor.object(g).unwrap().flags & 0x8002, 0);
}

#[test]
fn the_balloon_offers_only_what_the_part_can_carry() {
    use mmk_editor::attr::Attr;
    let (mut host, mut saves) = (MockHost::default(), Saves::default());
    let mut m = maker(&mut host, &mut saves, MakerStart::New("M1".into()));
    // a Hard Block: wings, no parachute
    m.editor.place(6, 12, 3).unwrap();
    m.cur = (12, 3);
    hold_on_part(&mut m, &mut host, &mut saves, attr::HOLD);
    assert_eq!(m.attr_options_shown(), Some(vec![Attr::Wings(true)]));
    run(&mut m, &mut host, &mut saves, idle());
    run(&mut m, &mut host, &mut saves, press(Buttons::BACK));
    // a Goomba on a track: the parachute is offered greyed, and refused
    let g = m.editor.place(0, 20, 3).unwrap();
    m.editor.set_flags(g, DEFAULT_FLAGS | 0x400);
    m.cur = (20, 3);
    hold_on_part(&mut m, &mut host, &mut saves, attr::HOLD);
    let opts = m.editor.attr_options(g);
    let para = opts
        .iter()
        .find(|o| o.attr == Attr::Parachute(true))
        .unwrap();
    assert!(!para.enabled);
    assert!(
        opts.iter()
            .any(|o| o.attr == Attr::Wings(true) && o.enabled)
    );
    run(&mut m, &mut host, &mut saves, idle());
    // from the first new choice (the other form) to the last
    for _ in 0..3 {
        run(&mut m, &mut host, &mut saves, press(Buttons::RIGHT));
    }
    run(&mut m, &mut host, &mut saves, press(Buttons::CONFIRM));
    assert!(
        m.attr_options_shown().is_some(),
        "a greyed choice keeps it open"
    );
    assert_eq!(m.editor.object(g).unwrap().flags & 0x8000, 0);
    run(&mut m, &mut host, &mut saves, press(Buttons::BACK));
    // a part with nothing to choose (24: one form, no wings, no parachute, no big size): the
    // hold just carries it
    let n = m.editor.place(24, 30, 3).unwrap();
    assert!(m.editor.attr_options(n).is_empty());
    m.cur = (30, 3);
    hold_on_part(&mut m, &mut host, &mut saves, attr::HOLD + 4);
    assert!(m.attr_options_shown().is_none() && m.carrying());
    run(&mut m, &mut host, &mut saves, idle());
    // moving while held is a carry, never a balloon
    m.cur = (12, 3);
    run(&mut m, &mut host, &mut saves, press(Buttons::CONFIRM));
    m.cur = (13, 3);
    for _ in 0..attr::HOLD + 4 {
        run(&mut m, &mut host, &mut saves, held(Buttons::CONFIRM));
    }
    assert!(m.attr_options_shown().is_none() && m.carrying());
}

#[test]
fn holding_a_part_of_a_selection_gives_the_whole_selection_wings() {
    use mmk_editor::attr::Attr;
    let (mut host, mut saves) = (MockHost::default(), Saves::default());
    let mut m = maker(&mut host, &mut saves, MakerStart::New("M1".into()));
    let a = m.editor.place(0, 12, 3).unwrap();
    let b = m.editor.place(0, 14, 3).unwrap();
    let block = m.editor.place(6, 16, 3).unwrap();
    m.selection = Some(Rect::new(12, 3, 5, 1));
    m.cur = (14, 3);
    hold_on_part(&mut m, &mut host, &mut saves, attr::HOLD);
    let shown = m.attr_options_shown().expect("the selection's balloon");
    assert!(shown.contains(&Attr::Wings(true)));
    run(&mut m, &mut host, &mut saves, idle());
    let k = shown.iter().position(|a| *a == Attr::Wings(true)).unwrap();
    let first = shown
        .iter()
        .position(|a| !matches!(a, Attr::Form(false)))
        .unwrap();
    for _ in first..k {
        run(&mut m, &mut host, &mut saves, press(Buttons::RIGHT));
    }
    run(&mut m, &mut host, &mut saves, press(Buttons::CONFIRM));
    run(&mut m, &mut host, &mut saves, idle());
    for i in [a, b, block] {
        assert_eq!(
            m.editor.object(i).unwrap().flags & 0x2,
            0x2,
            "part {i} winged"
        );
    }
    assert_eq!(m.selection, Some(Rect::new(12, 3, 5, 1)), "nothing moved");
    // one undo step takes the wings off them all
    m.editor.undo();
    for i in [a, b, block] {
        assert_eq!(m.editor.object(i).unwrap().flags & 0x2, 0);
    }
}

#[test]
fn a_winged_part_put_into_a_block_keeps_its_wings() {
    let (mut host, mut saves) = (MockHost::default(), Saves::default());
    let mut m = maker(&mut host, &mut saves, MakerStart::New("M1".into()));
    m.editor.place(5, 30, 4).unwrap();
    let g = m.editor.place(0, 33, 4).unwrap();
    m.editor.set_wings(g, true).unwrap();
    m.cur = (33, 4);
    run(&mut m, &mut host, &mut saves, press(Buttons::CONFIRM));
    assert!(m.carrying());
    m.cur = (30, 4);
    run(&mut m, &mut host, &mut saves, held(Buttons::CONFIRM));
    run(&mut m, &mut host, &mut saves, idle());
    let block = m.editor.object_at(30, 4).unwrap();
    let o = m.editor.object(block).unwrap();
    assert_eq!(o.child_id, 0, "the Goomba went in");
    assert_eq!(o.child_flags & 0x2, 0x2, "with its wings");
}

#[test]
fn a_goomba_dropped_on_a_goomba_stacks_on_it() {
    let (mut host, mut saves) = (MockHost::default(), Saves::default());
    let mut m = maker(&mut host, &mut saves, MakerStart::New("M1".into()));
    let base = m.editor.place(0, 20, 1).unwrap();
    m.editor.place(0, 24, 1).unwrap();
    m.cur = (24, 1);
    run(&mut m, &mut host, &mut saves, press(Buttons::CONFIRM));
    assert!(m.carrying());
    m.cur = (20, 1);
    run(&mut m, &mut host, &mut saves, held(Buttons::CONFIRM));
    host.edit_sounds.clear();
    run(&mut m, &mut host, &mut saves, idle());
    let base = m.editor.object_at(20, 1).unwrap_or(base);
    let column = m.editor.column_from(base);
    assert_eq!(column.len(), 2, "on top of it");
    assert!(m.editor.object_at(24, 1).is_none(), "moved, not copied");
    for i in column {
        assert_ne!(m.editor.object(i).unwrap().flags & 0x1000, 0);
    }
    // the stacked Goomba sounds `Tower_stack`, pitched by its tile row (2)
    assert!(
        host.edit_sounds
            .iter()
            .any(|(u, k)| u == "M1_EditKuribo" && k == "Tower_stack")
    );
    assert_eq!(host.edit_scales, [("Tower_stack".to_string(), 2.0)]);
}

#[test]
fn tower_stack_pitch_wraps_every_28_rows() {
    assert_eq!(
        [0, 2, 27, 28, 30, -1].map(super::tower_scale),
        [0.0, 2.0, 27.0, 0.0, 2.0, 0.0]
    );
}

#[test]
fn a_pipe_dragged_by_its_mouth_turns_round_its_base() {
    use mmk_editor::parts::Dir;
    let (mut host, mut saves) = (MockHost::default(), Saves::default());
    let mut m = maker(&mut host, &mut saves, MakerStart::New("M1".into()));
    let p = m.editor.place(9, 30, 6).unwrap();
    // an up pipe two tall: its mouth is the top row (y 7); drag it to the right of the base
    m.cur = (30, 7);
    run(&mut m, &mut host, &mut saves, press(Buttons::CONFIRM));
    m.cur = (35, 6);
    run(&mut m, &mut host, &mut saves, held(Buttons::CONFIRM));
    run(&mut m, &mut host, &mut saves, idle());
    let o = m.editor.object(p).unwrap();
    assert_eq!(Dir::from_flags(o.flags), Dir::Right);
    assert_eq!(o.height, 6, "base to cursor, both ends counted");
}

#[test]
fn a_track_is_drawn_by_dragging_and_a_lift_put_on_it_rides_it() {
    let (mut host, mut saves) = (MockHost::default(), Saves::default());
    let mut m = maker(&mut host, &mut saves, MakerStart::New("M1".into()));
    m.palette[0] = Tool::Track;
    m.slot = 0;
    m.cur = (20, 6);
    run(&mut m, &mut host, &mut saves, press(Buttons::CONFIRM));
    m.cur = (24, 6);
    run(&mut m, &mut host, &mut saves, held(Buttons::CONFIRM));
    m.cur = (28, 6);
    run(&mut m, &mut host, &mut saves, held(Buttons::CONFIRM));
    run(&mut m, &mut host, &mut saves, idle());
    assert_eq!(
        m.editor.area().tracks.len(),
        3,
        "a piece, then one per 2 tiles dragged"
    );
    // a press on its far end opens it (new ends are closed)
    assert_eq!(m.editor.track_end_open((25, 6)), Some(false));
    m.cur = (25, 6);
    run(&mut m, &mut host, &mut saves, press(Buttons::CONFIRM));
    run(&mut m, &mut host, &mut saves, idle());
    assert_eq!(m.editor.track_end_open((25, 6)), Some(true));
    assert_eq!(m.editor.area().tracks.len(), 3, "no new piece");
    m.editor.undo();
    // one undo step for the whole drag
    m.editor.undo();
    assert_eq!(m.editor.area().tracks.len(), 0);
    m.editor.redo();
    // a lift put down on the track rides it
    m.palette[1] = Tool::Part(11);
    m.slot = 1;
    m.cur = (22, 6);
    run(&mut m, &mut host, &mut saves, press(Buttons::CONFIRM));
    run(&mut m, &mut host, &mut saves, idle());
    let lift = m
        .editor
        .area()
        .objects
        .items
        .iter()
        .find(|o| o.id == 11)
        .expect("a lift");
    assert_ne!(lift.flags & 0x400, 0, "on the track");
}

#[test]
fn every_catalog_part_can_be_put_down_in_the_styles_that_offer_it() {
    for style in ["M1", "M3", "MW", "WU", "3W"] {
        for c in catalog::CATEGORIES {
            for x in catalog::of(c, style) {
                let Tool::Part(p) = x.tool else { continue };
                let mut e = mmk_editor::Editor::new(style);
                assert!(
                    e.place(p, 30, 6).is_ok(),
                    "{style}: part {p} ({}) can't be put down",
                    x.label
                );
            }
        }
    }
}

#[test]
fn an_empty_sub_area_asks_for_its_orientation() {
    let (mut host, mut saves) = (MockHost::default(), Saves::default());
    let mut m = maker(&mut host, &mut saves, MakerStart::New("M1".into()));
    if m.ui.layout("Edit_SubAreaWindow_00").is_none() {
        return; // no UI import here
    }
    run(&mut m, &mut host, &mut saves, keys(keys::AREA));
    run(&mut m, &mut host, &mut saves, idle());
    assert_eq!(m.editor.area, 1);
    assert!(m.subarea.is_some(), "the orientation window");
    run(&mut m, &mut host, &mut saves, press(Buttons::RIGHT));
    run(&mut m, &mut host, &mut saves, press(Buttons::CONFIRM));
    run(&mut m, &mut host, &mut saves, idle());
    assert!(m.subarea.is_none());
    assert_ne!(m.editor.area().header.orientation, 0, "vertical");
}
