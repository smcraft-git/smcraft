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
    assert_eq!(m.entry.as_deref(), Some("bot:1"));
    with(&mut host, &mut saves, |cx| {
        m.save_to_coursebot(cx.host, false)
    })
    .unwrap();
    assert_eq!(
        host.saved[1].0.as_deref(),
        Some("bot:1"),
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
    assert_eq!(id, &None, "a new course, not over theirs");
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
    let crate::Screen::World(w) = &f.screen else {
        panic!("the Coursebot list");
    };
    assert_eq!(w.stage_name(), "list");
    assert!(host.library.iter().any(|e| e.name == "Goomba walk"));
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
    // a new copy: Save As and Load only
    m.open_save();
    assert_eq!(m.save_buttons(), vec!["L_NSaveBtn_00", "L_LoadBtn_00"]);
    run(&mut m, &mut host, &mut saves, press(Buttons::CONFIRM));
    assert_eq!(host.saved.len(), 1);
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
    assert_eq!(ev, Some(MakerEvent::Coursebot));
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
