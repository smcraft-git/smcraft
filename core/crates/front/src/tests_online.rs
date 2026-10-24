//! Tests of the online screens and of clicks / taps over the game's own layouts, with a mock
//! host. They need the owner's UI imports (see `testing`) and return early without them.

use serde_json::json;

use crate::account::{AccountScene, Action};
use crate::challenge::{ChallengeFlow, Presentation};
use crate::coursepage::{CoursePageScene, PAGE_BUTTONS, PageButton, PageEvent};
use crate::endless::{EndlessEvent, EndlessMenuScene};
use crate::host::WorldCourse;
use crate::layout::Layout;
use crate::leaderboard::LeaderboardScene;
use crate::menu::{Buttons, Script, UiInput};
use crate::online::Online;
use crate::play::tests::{MockHost, with_online};
use crate::play::{CoursePlay, PlayInput};
use crate::screens::{CardInfo, CourseScreens};
use crate::smm1::{ChallengeMenuScene, RunQuery, Smm1Event};
use crate::testing::{click, press, smm1, smm2, typed};
use crate::world::{CourseListScene, HubChoice, HubEvent, HubScene, ListEvent, Tab, hub_bound};
use crate::{Saves, Ui, UiKind};

fn center(l: &mut Layout, tick: i32, pane: &str) -> [f32; 2] {
    l.apply(tick);
    l.rect(pane)
        .unwrap_or_else(|| panic!("no pane {pane}"))
        .center
}

fn idle() -> UiInput {
    UiInput::default()
}

fn server_course(code: &str, likes: i64) -> WorldCourse {
    WorldCourse::server(&json!({
        "code": code, "name": format!("Course {code}"), "maker": "Ada", "maker_id": "M1",
        "plays": 10, "clears": 5, "likes": likes, "uploaded": 1_700_000_000 + likes,
        "record_ms": 30_000, "record_holder": "M2", "first_clear": "M2",
        "record_verified": true,
    }))
    .unwrap()
}

fn signed_in() -> Online {
    let mut o = Online::default();
    o.account = Some(crate::online::Account {
        name: "Ada".into(),
        maker_id: "M1".into(),
        token: "tk".into(),
    });
    o
}

#[test]
fn hub_clicks_open_the_leaderboards_and_the_account() {
    let Some(ui) = smm2() else { return };
    for (part, choice) in [
        ("L_Maker_00", HubChoice::Leaderboards),
        ("L_PlayerBody_00", HubChoice::Account),
    ] {
        let mut h = HubScene::new(&ui, 0, Some("Ada")).unwrap();
        let mut script = Script::default();
        h.step(&idle(), &mut script);
        let p = center(&mut h.layout, h.tick, &hub_bound(part));
        let mut ev = h.step(&click(p), &mut script);
        for _ in 0..120 {
            if ev.is_some() {
                break;
            }
            ev = h.step(&idle(), &mut script);
        }
        assert_eq!(ev, Some(HubEvent::Choice(choice)), "{part}");
    }
}

#[test]
fn course_list_clicks_select_open_and_decide() {
    let Some(ui) = smm2() else { return };
    let mut s = CourseListScene::new(&ui).unwrap();
    s.set_courses(
        (0..4)
            .map(|k| server_course(&format!("AAA-AAA-AA{k}"), k))
            .collect(),
        false,
    );
    let mut sc = Script::default();
    for _ in 0..30 {
        s.step(&idle(), &mut sc);
    }
    // a tab
    let p = center(&mut s.layout, s.tick, "L_LatestBtn_00/B_Btn_00");
    s.step(&click(p), &mut sc);
    assert_eq!(s.tab, Tab::New);
    // the second row: selected; clicked again: opened
    let row = |s: &CourseListScene, i: i64| format!("{}/B_Btn_00", s.row_pane(i));
    let pane = row(&s, 1);
    let p = center(&mut s.layout, s.tick, &pane);
    s.step(&click(p), &mut sc);
    assert_eq!((s.selection, s.open), (1, false));
    let pane = row(&s, 1);
    let p = center(&mut s.layout, s.tick, &pane);
    s.step(&click(p), &mut sc);
    assert!(s.open);
    for _ in 0..20 {
        s.step(&idle(), &mut sc);
    }
    // its "View comments" button
    let pane = format!("{}/L_CommentBtn_00/B_Btn_00", s.row_pane(1));
    let p = center(&mut s.layout, s.tick, &pane);
    let want = s.courses[1].code.clone();
    let mut ev = s.step(&click(p), &mut sc);
    for _ in 0..90 {
        if ev.is_some() {
            break;
        }
        ev = s.step(&idle(), &mut sc);
    }
    match ev {
        Some(ListEvent::Comments(c)) => assert_eq!(c.code, want),
        e => panic!("{e:?}"),
    }
}

#[test]
fn open_row_keys_move_between_its_buttons() {
    let Some(ui) = smm2() else { return };
    let mut s = CourseListScene::new(&ui).unwrap();
    s.set_courses(vec![server_course("BBB-BBB-BBB", 1)], false);
    let mut sc = Script::default();
    for _ in 0..30 {
        s.step(&idle(), &mut sc);
    }
    s.step(&press(Buttons::CONFIRM), &mut sc);
    for _ in 0..20 {
        s.step(&idle(), &mut sc);
    }
    assert!(s.open);
    assert_eq!(s.button, 0, "Play first");
    s.step(&press(Buttons::UP), &mut sc);
    assert_eq!(s.button, 1, "View comments above Play");
    s.step(&press(Buttons::LEFT), &mut sc);
    assert_eq!(s.button, 2, "More info left of it");
    let mut ev = s.step(&press(Buttons::CONFIRM), &mut sc);
    for _ in 0..90 {
        if ev.is_some() {
            break;
        }
        ev = s.step(&idle(), &mut sc);
    }
    assert!(matches!(ev, Some(ListEvent::Detail(_))), "{ev:?}");
}

#[test]
fn account_registers_signs_in_and_out() {
    let Some(ui) = smm2() else { return };
    let mut host = MockHost {
        server: true,
        ..Default::default()
    };
    let (mut saves, mut online) = (Saves::default(), Online::default());
    let mut a = with_online(&mut host, &mut saves, &mut online, |cx| {
        let mut a = AccountScene::new(&ui, cx).unwrap();
        assert_eq!(a.buttons[0].1, Action::Create);
        for _ in 0..40 {
            a.step(&idle(), cx);
        }
        a.step(&press(Buttons::CONFIRM), cx);
        assert!(a.typing());
        for c in "Ad\u{8}da\n".chars() {
            a.step(&typed(c), cx);
        }
        assert!(!a.typing());
        a
    });
    assert_eq!(host.calls[0].1, "register");
    assert_eq!(host.calls[0].2, json!({"name": "Ada"}));
    host.answers.push((
        host.calls[0].0,
        Ok(json!({"account": {"id": 1, "name": "Ada", "maker_id": "M1"}, "token": "tk"})),
    ));
    with_online(&mut host, &mut saves, &mut online, |cx| {
        cx.online.poll(cx.host);
        a.step(&idle(), cx);
        assert_eq!(a.buttons[0].1, Action::SignOut);
    });
    assert_eq!(host.values["online.token"], "tk");
    assert!(online.signed_in());
    with_online(&mut host, &mut saves, &mut online, |cx| {
        a.step(&press(Buttons::CONFIRM), cx);
    });
    assert!(!online.signed_in());
    assert_eq!(host.values["online.token"], "");
}

#[test]
fn leaderboards_ask_the_server_per_board() {
    let Some(ui) = smm2() else { return };
    let mut host = MockHost {
        server: true,
        ..Default::default()
    };
    let (mut saves, mut online) = (Saves::default(), Online::default());
    let mut b = with_online(&mut host, &mut saves, &mut online, |cx| {
        LeaderboardScene::new(&ui, cx).unwrap()
    });
    assert_eq!(host.calls[0].2["kind"], "maker_points");
    host.answers.push((
        host.calls[0].0,
        Ok(json!([
            {"rank": 1, "score": 9, "maker_id": "M1", "name": "Ada"},
            {"rank": 2, "score": 3, "maker_id": "M2", "name": "Bo"}
        ])),
    ));
    with_online(&mut host, &mut saves, &mut online, |cx| {
        cx.online.poll(cx.host);
        for _ in 0..30 {
            b.step(&idle(), cx);
        }
        assert_eq!(b.rows.len(), 2);
        b.step(&press(Buttons::DOWN), cx);
        assert_eq!(b.selection, 1);
        b.step(&press(Buttons::RIGHT), cx);
    });
    assert_eq!(host.calls[1].2["kind"], "clears");
    assert!(b.rows.is_empty(), "the next board loads");
}

#[test]
fn course_page_races_the_record_ghost() {
    let Some(ui) = smm2() else { return };
    let mut host = MockHost {
        server: true,
        ..Default::default()
    };
    let (mut saves, mut online) = (Saves::default(), signed_in());
    let c = server_course("CCC-CCC-CCC", 2);
    let mut p = with_online(&mut host, &mut saves, &mut online, |cx| {
        CoursePageScene::new(&ui, c.clone(), cx).unwrap()
    });
    let actions: Vec<&str> = host.calls.iter().map(|c| c.1.as_str()).collect();
    assert_eq!(actions, ["info", "maker"], "fresh stats, the holder's name");
    let ev = with_online(&mut host, &mut saves, &mut online, |cx| {
        for _ in 0..20 {
            p.step(&idle(), cx);
        }
        // a click on the record holder's button
        let record = PAGE_BUTTONS
            .iter()
            .position(|b| b.1 == PageButton::Record)
            .unwrap();
        let at = center(&mut p.layout, p.tick, PAGE_BUTTONS[record].0);
        p.step(&click(at), cx)
    });
    assert_eq!(ev, None);
    let ghost = host.calls.last().unwrap().clone();
    assert_eq!(
        (ghost.1.as_str(), &ghost.2),
        ("ghost", &json!({"code": "CCC-CCC-CCC"}))
    );
    host.answers.push((ghost.0, Ok(json!({"frames": 900}))));
    let ev = with_online(&mut host, &mut saves, &mut online, |cx| {
        cx.online.poll(cx.host);
        p.step(&idle(), cx)
    });
    assert!(matches!(ev, Some(PageEvent::Ghost(g)) if g.code == c.code));
    // SMM1 play options: no ghost
    host.affects_play = true;
    let n = host.calls.len();
    with_online(&mut host, &mut saves, &mut online, |cx| {
        p.step(&press(Buttons::CONFIRM), cx);
    });
    assert_eq!(host.calls.len(), n);
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

#[test]
fn endless_menu_clicks_choose_and_start() {
    let Some(ui) = smm2() else { return };
    let mut m = EndlessMenuScene::new(&ui, 0, &AllOpen).unwrap();
    for _ in 0..40 {
        m.step(&idle(), &AllOpen);
    }
    let b = m.board.as_mut().unwrap();
    let p = center(&mut b.layout, m.tick, "L_Btn_02/B_Btn_00");
    m.step(&click(p), &AllOpen);
    assert_eq!(m.difficulty, 2);
    let b = m.board.as_mut().unwrap();
    let p = center(b.button_layer(), m.tick, "L_StartBtn_00/B_Btn_00");
    let mut ev = m.step(&click(p), &AllOpen);
    for _ in 0..90 {
        if ev.is_some() {
            break;
        }
        ev = m.step(&idle(), &AllOpen);
    }
    assert!(
        matches!(
            ev,
            Some(EndlessEvent::Start {
                difficulty: 2,
                resume: false
            })
        ),
        "{ev:?}"
    );
}

#[test]
fn smm1_menu_clicks_choose_and_start() {
    let Some(ui) = smm1() else { return };
    let mut m = ChallengeMenuScene::new(&ui, 0, &AllOpen).unwrap();
    for _ in 0..40 {
        m.step(&idle(), &AllOpen);
    }
    let p = center(
        &mut m.title,
        m.tick,
        &format!("{}/B_Btn_00", crate::smm1::TABS[1]),
    );
    m.step(&click(p), &AllOpen);
    assert_eq!(m.difficulty, 1);
    for _ in 0..40 {
        m.step(&idle(), &AllOpen);
    }
    let p = m.start_button_center("L_StartBtn_00").unwrap();
    let ev = m.step(&click(p), &AllOpen);
    assert!(
        matches!(
            ev,
            Some(Smm1Event::Start {
                difficulty: 1,
                resume: false
            })
        ),
        "{ev:?}"
    );
}

#[test]
fn completed_runs_reach_the_server_when_signed_in() {
    // (no layouts needed)
    let ui = Ui::new(UiKind::Smm2, mmk_ui::UiCache::in_memory(), None);
    let mut host = MockHost {
        server: true,
        ..Default::default()
    };
    let mut saves = Saves::default();
    let mut anon = Online::default();
    let mut f = with_online(&mut host, &mut saves, &mut anon, |cx| {
        ChallengeFlow::new(cx, &ui, None, &[], Presentation::Smm2)
    });
    with_online(&mut host, &mut saves, &mut anon, |cx| f.report_complete(cx));
    assert!(host.calls.is_empty(), "signed out: nothing sent");
    let mut me = signed_in();
    f.run.difficulty = 1;
    with_online(&mut host, &mut saves, &mut me, |cx| f.report_complete(cx));
    assert_eq!(host.calls[0].1, "hundred");
    assert_eq!(host.calls[0].2, json!({"difficulty": "normal"}));
}

fn play_input(b: Buttons) -> PlayInput {
    PlayInput {
        ui: press(b),
        start: false,
    }
}

#[test]
fn pause_votes_go_to_the_server() {
    let Some(ui) = smm2() else { return };
    let mut host = MockHost {
        server: true,
        ..Default::default()
    };
    let (mut saves, mut online) = (Saves::default(), signed_in());
    let info = CardInfo {
        name: "C".into(),
        server_code: Some("DDD-DDD-DDD".into()),
        ..Default::default()
    };
    let mut p = with_online(&mut host, &mut saves, &mut online, |cx| {
        let s = CourseScreens::new(&ui);
        CoursePlay::new(cx, "C".into(), None, s, None, info, false, true, false)
    });
    with_online(&mut host, &mut saves, &mut online, |cx| {
        p.step(&play_input(Buttons::NONE), cx, None);
        p.pause(None);
        for _ in 0..60 {
            p.step(&play_input(Buttons::NONE), cx, None);
        }
        p.step(&play_input(Buttons::DOWN), cx, None);
        assert_eq!(p.screens.as_ref().unwrap().pause_selection, 5, "Like");
        p.step(&play_input(Buttons::CONFIRM), cx, None);
        p.step(&play_input(Buttons::LEFT), cx, None);
        assert_eq!(p.screens.as_ref().unwrap().pause_selection, 4, "Boo");
        p.step(&play_input(Buttons::CONFIRM), cx, None);
        p.step(&play_input(Buttons::DOWN), cx, None);
        assert_eq!(p.screens.as_ref().unwrap().pause_selection, 1, "Start Over");
    });
    let votes: Vec<_> = host.calls.iter().filter(|c| c.1 == "vote").collect();
    assert_eq!(votes[0].2, json!({"code": "DDD-DDD-DDD", "like": true}));
    assert_eq!(votes[1].2, json!({"code": "DDD-DDD-DDD", "like": false}));
    assert_eq!(p.screens.as_ref().unwrap().voted, Some(false));
}

#[test]
fn tab_lists_arrive_per_tab_in_the_sources_order() {
    let Some(ui) = smm2() else { return };
    let mut s = CourseListScene::new(&ui).unwrap();
    s.use_tab_lists();
    assert_eq!(s.want_tab, Some(Tab::Hot));
    s.want_tab = None;
    // (TGRCode's order, not sorted by date or likes)
    let hot: Vec<WorldCourse> = [1, 9, 4]
        .iter()
        .map(|&k| server_course(&format!("AAA-AAA-AA{k}"), k))
        .collect();
    s.set_tab_courses(Tab::Hot, hot);
    let codes = |s: &CourseListScene| -> Vec<String> {
        s.courses.iter().filter_map(|c| c.code.clone()).collect()
    };
    assert_eq!(codes(&s), ["AAA-AAA-AA1", "AAA-AAA-AA9", "AAA-AAA-AA4"]);
    let mut sc = Script::default();
    for _ in 0..30 {
        s.step(&idle(), &mut sc);
    }
    let p = center(&mut s.layout, s.tick, "L_LatestBtn_00/B_Btn_00");
    s.step(&click(p), &mut sc);
    assert_eq!((s.tab, s.want_tab), (Tab::New, Some(Tab::New)));
    assert!(s.courses.is_empty(), "the New list hasn't arrived");
    s.set_tab_courses(Tab::New, vec![server_course("BBB-BBB-BB2", 0)]);
    assert_eq!(codes(&s), ["BBB-BBB-BB2"]);
    // back on Hot: kept, not asked for again
    let p = center(&mut s.layout, s.tick, "L_PickUpBtn_00/B_Btn_00");
    s.want_tab = None;
    s.step(&click(p), &mut sc);
    assert_eq!((s.tab, s.want_tab), (Tab::Hot, None));
    assert_eq!(codes(&s).len(), 3);
    // a server row's Download button
    s.step(&press(Buttons::CONFIRM), &mut sc);
    for _ in 0..20 {
        s.step(&idle(), &mut sc);
    }
    let pane = format!("{}/L_DownloadPlayBtn_00/B_Btn_00", s.row_pane(0));
    let p = center(&mut s.layout, s.tick, &pane);
    let mut ev = s.step(&click(p), &mut sc);
    for _ in 0..90 {
        if ev.is_some() {
            break;
        }
        ev = s.step(&idle(), &mut sc);
    }
    match ev {
        Some(ListEvent::Download(c)) => assert_eq!(c.code.as_deref(), Some("AAA-AAA-AA1")),
        e => panic!("{e:?}"),
    }
}

#[test]
fn the_id_search_takes_a_typed_course_or_maker_id() {
    use crate::search::{IdSearchScene, SearchEvent};
    let Some(ui) = smm2() else { return };
    let code = mmk_formats::course_id::smm2_data_id_to_code(5000, false);
    let maker = mmk_formats::course_id::smm2_data_id_to_code(77, true);
    for (id, want) in [
        (code.clone(), SearchEvent::Course(code.clone())),
        (maker.replace('-', ""), SearchEvent::Maker(maker.clone())),
    ] {
        let mut s = IdSearchScene::new(&ui).unwrap();
        s.step(&idle());
        let mut ev = None;
        for c in id.chars().chain(['\n']) {
            ev = ev.or(s.step(&typed(c)));
        }
        assert_eq!(ev, Some(want));
    }
    // not an ID: stays, with a note
    let mut s = IdSearchScene::new(&ui).unwrap();
    for c in "ABC-DEF-GHJ\n".chars() {
        assert_eq!(s.step(&typed(c)), None);
    }
    assert!(s.typing());
}

#[test]
fn tgrcode_listings_bring_names_and_tags() {
    let c = WorldCourse::server(&json!({
        "code": "AAA-AAA-AAA", "name": "N", "record_holder": "Q8V-RKX-8MG",
        "record_holder_name": "Bo", "first_clear": "Q8V-RKX-8MG", "first_clear_name": "Bo",
        "tags": [5, 5], "source": "tgrcode",
    }))
    .unwrap();
    assert_eq!(c.record_holder_name.as_deref(), Some("Bo"));
    assert_eq!(c.tags, [5], "the same tag twice is one tag");
}

/// Course and maker names in Japanese, Korean and Chinese draw glyphs of their own (rasterised
/// at run time from the import's scalable fonts), never the replacement glyph.
#[test]
fn cjk_names_draw_their_own_glyphs() {
    let Some(ui) = smm2() else { return };
    let names = [
        "スーパーマリオの城",
        "슈퍼 마리오 월드",
        "超级马里奥的世界",
        "超級瑪利歐兄弟",
    ];
    let sheet = |ui: &Ui, id: u32| {
        ui.cache()
            .textures
            .get(id as usize)
            .is_some_and(|t| t.name.starts_with("glyphs/"))
    };
    // course names (NX_std), player names (MessageFont), titles (SpecialFont)
    for name in ["NX_std", "MessageFont", "SpecialFont"] {
        let font = ui.cache_mut().font(name).unwrap();
        if font.dynamic.is_none() {
            // an import from before run-time glyphs (SMM2 UI importer 5)
            eprintln!("{name}: no scalable fonts in this UI import; skipped");
            return;
        }
        for s in names {
            for c in s.chars().filter(|&c| c != ' ') {
                let f = font.for_char(c);
                assert!(f.draws(c), "{name} {c}");
                // baked (the English messages have a few kana), else from a glyph sheet
                assert!(
                    f.has_char(c) || sheet(&ui, f.glyph(c).texture),
                    "{name} {c}"
                );
            }
        }
    }
    // a course list with those names: a glyph-sheet quad for each character of the visible rows
    // the sheets lack
    let mut s = CourseListScene::new(&ui).unwrap();
    let courses = names
        .iter()
        .enumerate()
        .map(|(k, n)| {
            WorldCourse::server(&json!({
                "code": format!("CJK-AAA-AA{k}"), "name": n, "maker": "マリオ",
                "maker_id": "M1", "plays": 10, "clears": 5, "likes": 1,
                "uploaded": 1_700_000_000,
            }))
            .unwrap()
        })
        .collect();
    s.set_courses(courses, false);
    let mut sc = Script::default();
    for _ in 0..30 {
        s.step(&idle(), &mut sc);
    }
    let mut items = Vec::new();
    s.draw(&mut items);
    assert!(ui.cache_mut().update_glyphs() || ui.cache().revision() > 0);
    let quads: Vec<mmk_ui::Quad> = items
        .into_iter()
        .flat_map(|i| match i {
            crate::DrawItem::Layout { quads, .. } => quads,
            _ => Vec::new(),
        })
        .collect();
    let drawn = quads.iter().filter(|q| sheet(&ui, q.tex0)).count();
    let course_font = ui.cache_mut().font("NX_std").unwrap();
    let visible: usize = names
        .iter()
        .take(3)
        .map(|n| n.chars().filter(|&c| !course_font.has_char(c)).count())
        .sum();
    assert!(
        drawn >= visible,
        "{drawn} glyph quads, {visible} characters"
    );
    // and the sheets they use have their pixels
    let c = ui.cache();
    for q in quads.iter().filter(|q| sheet(&ui, q.tex0)) {
        let t = &c.textures[q.tex0 as usize];
        assert!(t.ready && t.texture.data.len() == 1024 * 1024 * 4);
    }
}
