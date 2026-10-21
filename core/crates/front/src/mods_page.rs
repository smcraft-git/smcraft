//! The Mods page: every mod option (`mods`) on one settings window, drawn with SMM2's own
//! settings window `Cmn_OptionWindow_00` (Type 0, the online variant with its eight setting
//! rows) [obs layout]. Its setting buttons (`Cmn_OptionSettingBtn_00`) show an option's value;
//! the labels above them are the options' names, the Player Controls row's label is the
//! selected option's description, and the character row, the controls button and the help /
//! rules buttons are hidden [departure: SMM2 has no such page]. The top-left row turns the page
//! (two pages); confirm (or a click) on a row moves its value on, L / R (ZL / ZR) turn the
//! page, B or OK closes. Values are kept and applied at once.
//!
//! The page opens from the title (`TitleMenuButton`: SMM2's round menu button,
//! `Cmn_MenuOpenBtn_00`, in the top-left corner while Make / Play or the Play menu is up; up
//! from Make / Play selects it) [departure: SMM2's title has no menu button; DECISIONS.md
//! 2026-10-21 "The Mods page"].

use crate::challenge::Presentation;
use crate::layout::{Layout, Ui};
use crate::menu::{Buttons, MenuCursor, UiInput, hit, nearest};
use crate::mods::{self, CHANGES_PLAY_NOTE, ModOption, PAGES, Target};
use crate::sound::Event;
use crate::title;
use crate::{Cx, DrawItem, push_layout};

pub const WINDOW: &str = "Cmn_OptionWindow_00";

/// The window's setting rows: (row, label, button). Row 0 turns the page.
const ROWS: [(&str, &str, &str); 7] = [
    ("N_EditMode_00", "T_EditMode_00", "L_EditModeBtn_00"),
    ("N_Friend_00", "T_Feinend_00", "L_FriendBtn_00"),
    ("N_Comment_00", "T_Comment_00", "L_W_CommentBtn_00"),
    ("N_PlayInfo_00", "T_PlayInfo_00", "L_OptionSettingBtn_00"),
    (
        "N_RealTimeInfo_00",
        "T_RealTimeInfo_00",
        "L_RealTimeInfoBtn_00",
    ),
    (
        "N_CommentLock_00",
        "T_CommentLock_00",
        "L_CommentLockBtn_00",
    ),
    ("N_TagLock_00", "T_TagLock_00", "L_TagLockBtn_00"),
];
const OK: &str = "L_OKBtn_00";
/// The description: the Player Controls row's label, its button hidden.
const ABOUT: &str = "T_PlayerSetting_00";
const ABOUT_ROW: &str = "N_PlayerSetting_00";
/// The description's width (pane units) and lines.
const ABOUT_WIDTH: f32 = 430.0;
const ABOUT_LINES: usize = 5;

/// What the page tells its owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModsEvent {
    /// The window closed; `hundred`: the 100 Mario screens chosen on it.
    Closed { hundred: Option<Presentation> },
}

/// The Mods settings window.
pub struct ModsPage {
    ui: Ui,
    pub layout: Layout,
    /// The dim under the window (as Course World's windows have).
    veil: Option<Layout>,
    cursor: MenuCursor,
    pub tick: i32,
    pub page: usize,
    /// 0–6 a row, 7 OK.
    pub selection: usize,
    costumes: Vec<(u16, String)>,
    /// The SMM1 import is there (its 100 Mario screens can be chosen).
    smm1: bool,
    busy_until: i32,
    closing: Option<i32>,
    hundred: Option<Presentation>,
}

impl ModsPage {
    pub fn new(ui: &Ui, smm1: bool, cx: &mut Cx) -> Option<Self> {
        let mut l = ui.layout(WINDOW)?;
        l.hold("Type", 0.0);
        for p in [
            "N_Charcter_00",
            "L_PlayerSettingBtn_00",
            "L_HelpBtn_00",
            "L_RuleBtn_00",
        ] {
            l.show(p, false);
        }
        l.text("L_Window_00/T_Index_00", "Mods");
        l.text_from("L_OKBtn_00/T_Btn_00", "L_OKBtn_00-T_Btn_00");
        // the description a little lower than the label it is, so five lines fit under the
        // title bar [inf]
        l.set_translate(ABOUT_ROW, [240.0, 150.0]);
        for (_, _, b) in ROWS {
            l.hold(&format!("{b}/Check"), 0.0);
            l.hold(&format!("{b}/Select"), 0.0);
            // the region icon of the comment settings' button isn't ours
            l.show(&format!("{b}/P_OptionMarkRegion_00"), false);
        }
        l.hold(&format!("{OK}/Select"), 0.0);
        l.play("In", 0);
        let mut veil = ui.layout("Cmn_Fade_00");
        if let Some(f) = &mut veil {
            f.hold_end("FadeOutB");
            for p in ["P_FadeW_00", "N_Pos_00", "N_Block_00", "N_CaptureFade_00"] {
                f.show(p, false);
            }
            f.set_alpha("P_FadeB_00", 200.0);
        }
        let busy = l.ticks("In", 1);
        let mut s = Self {
            ui: ui.clone(),
            layout: l,
            veil,
            cursor: MenuCursor::new(ui),
            tick: 0,
            page: 0,
            selection: 1,
            costumes: cx.host.costumes(),
            smm1,
            busy_until: busy,
            closing: None,
            hundred: None,
        };
        s.refresh(cx);
        s.select(1, false);
        Some(s)
    }

    /// The option on row `row` of this page.
    fn option_at(&self, row: usize) -> Option<&'static ModOption> {
        let i = *PAGES[self.page].1.get(row.checked_sub(1)?)?;
        mods::OPTIONS.get(i)
    }

    fn row_shown(&self, row: usize) -> bool {
        row == 0 || self.option_at(row).is_some()
    }

    /// The bounding pane of selectable item `i`.
    fn bound(i: usize) -> String {
        match ROWS.get(i) {
            Some((_, _, b)) => format!("{b}/B_Btn_00"),
            None => format!("{OK}/B_Btn_00"),
        }
    }

    fn part(i: usize) -> &'static str {
        ROWS.get(i).map_or(OK, |r| r.2)
    }

    /// Labels and values of the page.
    fn refresh(&mut self, cx: &mut Cx) {
        let n = PAGES.len();
        let l = &mut self.layout;
        l.text(ROWS[0].1, "Page (L / R)");
        l.text(
            &format!("{}/T_Option_00", ROWS[0].2),
            &format!("{} ({}/{n})", PAGES[self.page].0, self.page + 1),
        );
        for (row, &(node, label, button)) in ROWS.iter().enumerate().skip(1) {
            let Some(o) = self.option_at(row) else {
                self.layout.show(node, false);
                continue;
            };
            let shown = self.value_text(o, cx);
            let l = &mut self.layout;
            l.visible(node, None);
            let name = if o.changes_play {
                format!("{} *", o.label)
            } else {
                o.label.to_string()
            };
            l.text(label, &name);
            l.text(&format!("{button}/T_Option_00"), &shown);
        }
        self.describe();
    }

    fn value_text(&self, o: &ModOption, cx: &mut Cx) -> String {
        let v = o.get(cx.host);
        let choices = o.choices(&self.costumes);
        let mut shown = choices
            .iter()
            .find(|(k, _)| *k == v)
            .map(|c| c.1.clone())
            .unwrap_or_else(|| format!("#{v}"));
        if o.target == Target::Hundred && !self.smm1 && v == "smm1" {
            shown += " (no SMM1 import)";
        }
        shown
    }

    /// The selected option's description in the description pane.
    fn describe(&mut self) {
        let text = match self.option_at(self.selection) {
            Some(o) if o.changes_play => format!("{} {}", o.about, CHANGES_PLAY_NOTE),
            Some(o) => o.about.to_string(),
            None if self.selection == 0 => "Turn the page: L / R, or confirm here. Options \
                                            marked * change how courses play."
                .to_string(),
            None => "Values are kept as soon as they change.".to_string(),
        };
        self.layout
            .wrapped_text(ABOUT, &text, ABOUT_WIDTH, ABOUT_LINES);
    }

    fn select(&mut self, i: usize, sound: bool) {
        if sound && i != self.selection {
            self.ui.sound_button(WINDOW, Self::part(i), Event::Select);
        }
        self.selection = i;
        let t = self.tick;
        for k in 0..=ROWS.len() {
            let a = format!("{}/Select", Self::part(k));
            if k == i {
                self.layout.play(&a, t);
            } else {
                self.layout.hold(&a, 0.0);
            }
        }
        self.describe();
    }

    /// Turn to page `p`.
    pub fn turn(&mut self, p: usize, cx: &mut Cx) {
        self.page = p % PAGES.len();
        self.refresh(cx);
        if !self.row_shown(self.selection) {
            self.select(1, false);
        }
        self.describe();
    }

    /// Confirm on the selected item. True: the window closes.
    fn decide(&mut self, cx: &mut Cx) -> bool {
        let i = self.selection;
        let part = Self::part(i);
        self.ui.sound_button(WINDOW, part, Event::Decide);
        self.layout.play(&format!("{part}/Decide"), self.tick);
        if i == ROWS.len() {
            return true;
        }
        if i == 0 {
            self.turn(self.page + 1, cx);
            return false;
        }
        if let Some(o) = self.option_at(i) {
            let choices = o.choices(&self.costumes);
            let v = o.get(cx.host);
            let at = choices.iter().position(|(k, _)| *k == v);
            let next = at.map_or(0, |k| (k + 1) % choices.len());
            let value = choices[next].0.clone();
            o.set(cx.host, &value);
            if o.target == Target::Hundred {
                self.hundred = Some(if value == "smm1" {
                    Presentation::Smm1
                } else {
                    Presentation::Smm2
                });
            }
            self.refresh(cx);
        }
        false
    }

    fn close(&mut self) {
        if self.closing.is_some() {
            return;
        }
        let t = self.tick;
        self.layout.stop("In");
        self.layout.play("Out", t);
        self.closing = Some(t + self.layout.ticks("Out", 1));
        self.cursor.hide();
    }

    pub fn step(&mut self, input: &UiInput, cx: &mut Cx) -> Option<ModsEvent> {
        self.tick += 1;
        if let Some(at) = self.closing {
            if self.tick >= at {
                return Some(ModsEvent::Closed {
                    hundred: self.hundred,
                });
            }
            return None;
        }
        if self.tick >= self.busy_until {
            self.handle(input, cx);
        }
        if self.closing.is_none() {
            let pane = Self::bound(self.selection);
            let t = self.tick;
            self.cursor.place(&mut self.layout, &pane, t, 1.0);
        }
        None
    }

    fn handle(&mut self, input: &UiInput, cx: &mut Cx) {
        let mut p = input.pressed;
        if let Some(c) = input.click {
            let shown: Vec<usize> = (0..=ROWS.len())
                .filter(|&i| i == ROWS.len() || self.row_shown(i))
                .collect();
            let panes: Vec<String> = shown.iter().map(|&i| Self::bound(i)).collect();
            let t = self.tick;
            if let Some(k) = hit(&mut self.layout, t, &panes, c) {
                self.select(shown[k], false);
                p = p | Buttons::CONFIRM;
            }
        }
        if p.has(Buttons::BACK) {
            self.ui.sound_back(WINDOW, "normal");
            self.close();
            return;
        }
        if p.has(Buttons::CONFIRM) {
            if self.decide(cx) {
                self.close();
            }
            return;
        }
        let n = PAGES.len();
        if p.has(Buttons::L) || p.has(Buttons::ZL) {
            self.ui.sound_button(WINDOW, ROWS[0].2, Event::Select);
            self.turn(self.page + n - 1, cx);
            return;
        }
        if p.has(Buttons::R) || p.has(Buttons::ZR) {
            self.ui.sound_button(WINDOW, ROWS[0].2, Event::Select);
            self.turn(self.page + 1, cx);
            return;
        }
        if let Some(dir) = p.direction() {
            let t = self.tick;
            self.layout.apply(t);
            let centre = |l: &Layout, i: usize| l.rect(&Self::bound(i)).map(|r| r.center);
            let Some(from) = centre(&self.layout, self.selection) else {
                return;
            };
            let targets: Vec<(usize, [f32; 2])> = (0..=ROWS.len())
                .filter(|&i| i != self.selection && (i == ROWS.len() || self.row_shown(i)))
                .filter_map(|i| centre(&self.layout, i).map(|c| (i, c)))
                .collect();
            if let Some(i) = nearest(from, dir, &targets) {
                self.select(i, true);
            }
        }
    }

    pub fn draw(&mut self, out: &mut Vec<DrawItem>) {
        let t = self.tick;
        if let Some(v) = &mut self.veil {
            push_layout(out, v, 0);
        }
        push_layout(out, &mut self.layout, t);
        self.cursor.draw(t, out);
    }
}

/// SMM2's round menu button in the title's top-left corner, opening the Mods page.
pub struct TitleMenuButton {
    ui: Ui,
    layout: Layout,
    cursor: MenuCursor,
    tick: i32,
    pub focused: bool,
    shown: bool,
}

const MENU_BUTTON: &str = "Cmn_MenuOpenBtn_00";
/// Where the button sits (layout units): the top-left corner, clear of the logo [inf].
pub const MENU_BUTTON_AT: [f32; 2] = [-568.0, 296.0];

impl TitleMenuButton {
    pub fn new(ui: &Ui) -> Option<Self> {
        let mut l = ui.layout(MENU_BUTTON)?;
        l.set_translate("N_Btn_00", MENU_BUTTON_AT);
        l.set_translate("B_Btn_00", MENU_BUTTON_AT);
        // its "+" badge: + isn't the key for it here
        l.show("L_ShortCut_00", false);
        l.hold("Select", 0.0);
        Some(Self {
            ui: ui.clone(),
            layout: l,
            cursor: MenuCursor::new(ui),
            tick: 0,
            focused: false,
            shown: false,
        })
    }

    fn focus(&mut self, on: bool) {
        if on == self.focused {
            return;
        }
        self.focused = on;
        if on {
            self.ui.sound_button(MENU_BUTTON, "N_Btn_00", Event::Select);
            self.layout.play("Select", self.tick);
        } else {
            self.layout.hold("Select", 0.0);
        }
    }

    /// One tick over the title in `state`: true when the page should open. `input` loses what
    /// the button used.
    fn step(&mut self, input: &mut UiInput, state: title::State) -> bool {
        self.tick += 1;
        self.shown = matches!(state, title::State::Main | title::State::Category);
        if !self.shown {
            self.focus(false);
            self.cursor.hide();
            return false;
        }
        if state != title::State::Main {
            self.focus(false);
        }
        let mut open = false;
        if let Some(c) = input.click {
            let t = self.tick;
            if hit(&mut self.layout, t, &["B_Btn_00".to_string()], c).is_some() {
                input.click = None;
                open = true;
            }
        }
        let p = input.pressed;
        if self.focused {
            if p.has(Buttons::CONFIRM) {
                open = true;
            } else if p.has(Buttons::DOWN) || p.has(Buttons::BACK) {
                self.focus(false);
            }
            // the title doesn't see the presses the button takes
            input.pressed = Buttons::NONE;
        } else if state == title::State::Main && p.has(Buttons::UP) {
            self.focus(true);
            input.pressed = p.minus(Buttons::UP);
        }
        if open {
            self.ui.sound_button(MENU_BUTTON, "N_Btn_00", Event::Decide);
            self.layout.play("Decide", self.tick);
        }
        let t = self.tick;
        if self.focused {
            self.cursor.place(&mut self.layout, "B_Btn_00", t, 1.0);
        } else {
            self.cursor.hide();
        }
        open
    }

    fn draw(&mut self, out: &mut Vec<DrawItem>) {
        if self.shown {
            let t = self.tick;
            push_layout(out, &mut self.layout, t);
            self.cursor.draw(t, out);
        }
    }
}

/// The Mods page and the title's button to it, as the front end keeps them.
pub struct ModsUi {
    ui: Ui,
    smm1: bool,
    button: Option<TitleMenuButton>,
    pub page: Option<ModsPage>,
}

impl ModsUi {
    pub fn new(ui: &Ui, smm1: bool) -> Self {
        Self {
            ui: ui.clone(),
            smm1,
            button: TitleMenuButton::new(ui),
            page: None,
        }
    }

    /// Open the page.
    pub fn open(&mut self, cx: &mut Cx) {
        if self.page.is_none() {
            self.page = ModsPage::new(&self.ui, self.smm1, cx);
        }
    }

    /// One title tick: returns the input the title still gets and the page's event. While the
    /// page is open the title gets nothing (its demo course plays on behind).
    pub fn title_step(
        &mut self,
        input: &UiInput,
        state: title::State,
        cx: &mut Cx,
    ) -> (UiInput, Option<ModsEvent>) {
        if let Some(p) = &mut self.page {
            let ev = p.step(input, cx);
            if ev.is_some() {
                self.page = None;
            }
            return (UiInput::default(), ev);
        }
        let mut rest = *input;
        let open = self
            .button
            .as_mut()
            .is_some_and(|b| b.step(&mut rest, state));
        if open {
            self.open(cx);
            return (UiInput::default(), None);
        }
        (rest, None)
    }

    /// The title's own cursor should hide (the button or the page has the focus).
    pub fn has_focus(&self) -> bool {
        self.page.is_some() || self.button.as_ref().is_some_and(|b| b.focused)
    }

    pub fn draw(&mut self, out: &mut Vec<DrawItem>) {
        if let Some(b) = &mut self.button {
            b.draw(out);
        }
        if let Some(p) = &mut self.page {
            p.draw(out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::Saves;
    use crate::mods::tests::MemHost;
    use crate::testing::{click, press, smm2};

    fn with<R>(host: &mut MemHost, f: impl FnOnce(&mut Cx) -> R) -> R {
        let mut saves = Saves::default();
        let mut online = crate::online::Online::default();
        let mut fade = crate::fade::SceneFade::new(None);
        let (mut pending, mut now, mut toast) = (None, Vec::new(), None);
        let mut cx = Cx {
            host,
            saves: &mut saves,
            online: &mut online,
            fade: &mut fade,
            pending: &mut pending,
            now: &mut now,
            toast: &mut toast,
        };
        f(&mut cx)
    }

    fn run(page: &mut ModsPage, host: &mut MemHost, input: UiInput) -> Option<ModsEvent> {
        with(host, |cx| page.step(&input, cx))
    }

    #[test]
    fn rows_change_values_pages_turn_and_ok_closes() {
        let Some(ui) = smm2() else { return };
        let mut host = MemHost::default();
        let mut page = with(&mut host, |cx| ModsPage::new(&ui, false, cx)).unwrap();
        for _ in 0..30 {
            run(&mut page, &mut host, UiInput::default());
        }
        // row 1 is the shell jumps: confirm turns it on, through to the engine
        assert_eq!(page.selection, 1);
        run(&mut page, &mut host, press(Buttons::CONFIRM));
        assert_eq!(host.values["mods.smm1_shell_kicks"], "1");
        assert_eq!(host.engine, vec![("smm1_shell_kicks".to_string(), 1)]);
        // R: the second page; its first option is quick level return
        run(&mut page, &mut host, press(Buttons::R));
        assert_eq!(page.page, 1);
        run(&mut page, &mut host, press(Buttons::CONFIRM));
        assert_eq!(host.values[crate::play::QUICK_RETURN_KEY], "1");
        // a click on the 100 Mario screens' button (row 3 on this page)
        page.layout.apply(page.tick);
        let c = page.layout.rect(&ModsPage::bound(3)).unwrap().center;
        run(&mut page, &mut host, click(c));
        assert_eq!(host.values[mods::HUNDRED_SCREENS_KEY], "smm1");
        // the second page has five options: row 6 is hidden, down from row 5 goes to OK
        assert!(!page.row_shown(6));
        // OK
        page.select(ROWS.len(), false);
        run(&mut page, &mut host, press(Buttons::CONFIRM));
        let mut ev = None;
        for _ in 0..30 {
            ev = ev.or(run(&mut page, &mut host, UiInput::default()));
        }
        assert_eq!(
            ev,
            Some(ModsEvent::Closed {
                hundred: Some(Presentation::Smm1)
            })
        );
    }

    #[test]
    fn the_title_button_takes_up_and_confirm_on_make_play() {
        let Some(ui) = smm2() else { return };
        let mut host = MemHost::default();
        let mut m = ModsUi::new(&ui, false);
        let step = |m: &mut ModsUi, host: &mut MemHost, i: UiInput, s: title::State| {
            with(host, |cx| m.title_step(&i, s, cx))
        };
        // the press-a-button screen: no button
        let (rest, _) = step(&mut m, &mut host, press(Buttons::UP), title::State::Top);
        assert_eq!(rest.pressed, Buttons::UP);
        let (rest, _) = step(&mut m, &mut host, press(Buttons::UP), title::State::Main);
        assert_eq!(rest.pressed, Buttons::NONE);
        assert!(m.has_focus());
        step(
            &mut m,
            &mut host,
            press(Buttons::CONFIRM),
            title::State::Main,
        );
        assert!(m.page.is_some());
        // a click on it from the Play menu opens it too
        let mut m = ModsUi::new(&ui, false);
        step(
            &mut m,
            &mut host,
            click(MENU_BUTTON_AT),
            title::State::Category,
        );
        assert!(m.page.is_some());
    }
}
