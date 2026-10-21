//! The Coursebot with SMM2's own layouts (research/specs/coursebot.md A.4, private):
//! `Lcl_CourseList_00` (the robot frame, its header text, the My Courses / Downloaded Courses
//! tabs) over `Lcl_CourseListContents_00` (rows `Lcl_CourseDataList_00` of four slot buttons
//! `Lcl_CourseDataPostBtn_00`), and the course window `Lcl_CourseInfoWindow_00`.
//!
//! - **Browse** (title Play → Coursebot, the maker's main menu): a course's window has Make,
//!   Play, Upload, Name, Description, Delete (`TypeMode` 0) [obs data]; an empty slot offers
//!   "Make New Course" (`TypeMode` 2). Downloaded Courses lists the library's other courses
//!   (Play, and Make [departure: the game can't edit downloaded courses; the owner's rule is
//!   that their copies save as new courses with a credit]).
//! - **Save** (the maker's Save / Save As): "Select Save Location"; an empty slot saves at once,
//!   a used one asks "This data will be overwritten." with Overwrite.
//! - **Load** (the maker's Load): "Select a Course to Load"; "Load this course data?" with Load,
//!   or "Make New Course" on an empty slot.
//!
//! The slots are the host's (`Host::coursebot_slots`); course pictures are the host's start
//! views (`Host::course_thumbnail`). Three rows show at a time [inf: the layout's five recycled
//! rows scroll behind a scissor the layout player doesn't clip yet, so the outer two stay
//! hidden]. Name and Description are typed with the keyboard ([`crate::FrontInput::text`]).

use std::collections::HashMap;

use crate::host::{COURSEBOT_SLOTS, CourseEntry, Source};
use crate::layout::{Layout, Ui};
use crate::maker::UPLOAD_NEEDS_SERVER;
use crate::menu::{Buttons, MenuCursor, UiInput, hit};
use crate::sound::Event;
use crate::{Cx, DrawItem, push_layout};

const FRAME: &str = "Lcl_CourseList_00";
const LIST: &str = "Lcl_CourseListContents_00";
const INFO: &str = "Lcl_CourseInfoWindow_00";
/// Slot buttons per row, rows shown.
const COLS: usize = 4;
const ROWS: usize = 3;
/// The slot label's width (`T_Title_00`) [obs data].
const TITLE_W: f32 = 150.0;
/// Downloaded Courses slots [obs: 60, coursebot.md A.3].
const DOWNLOADED: usize = 60;

/// Why the Coursebot is open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BotMode {
    Browse,
    /// Choose where the maker's course goes.
    Save,
    /// Choose a course for the maker.
    Load,
}

/// What the Coursebot asks of its owner.
#[derive(Clone, Debug, PartialEq)]
pub enum BotEvent {
    Back,
    /// Open a course in the maker (the entry it saves back to; someone else's credit).
    Edit {
        source: Source,
        entry: Option<String>,
        based_on: Option<String>,
    },
    /// A new course in the maker.
    New,
    Play(Box<CourseEntry>),
    /// Save the maker's course into this slot.
    SaveTo(usize),
}

/// The course window's buttons by mode (part, what it does).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Btn {
    Make,
    Play,
    Upload,
    Name,
    Description,
    Delete,
    /// The window's message button (Make New Course, Overwrite, Load).
    Mode,
}

const MY_BUTTONS: [(&str, Btn); 6] = [
    ("L_Edit_00", Btn::Make),
    ("L_Upload_00", Btn::Upload),
    ("L_CoursName_00", Btn::Name),
    ("L_CourseInfo_00", Btn::Description),
    ("L_Del_00", Btn::Delete),
    ("L_Play_00", Btn::Play),
];
/// Downloaded: Play, and Make on the co-op button's place (see the module notes).
const DL_BUTTONS: [(&str, Btn); 2] = [
    ("L_CoopModeBtn_01", Btn::Make),
    ("L_DLPlayBtn_00", Btn::Play),
];

/// A text field being typed: the course's name or description.
#[derive(Clone, Debug, PartialEq)]
struct Typing {
    description: bool,
    text: String,
}

pub struct CoursebotScene {
    ui: Ui,
    pub frame: Layout,
    pub list: Layout,
    pub info: Layout,
    cursor: MenuCursor,
    pub mode: BotMode,
    /// 0 My Courses, 1 Downloaded Courses.
    pub tab: usize,
    pub my: Vec<Option<CourseEntry>>,
    pub downloaded: Vec<CourseEntry>,
    pub sel: usize,
    /// First shown row.
    pub first: usize,
    /// The course window is open, and the button under the cursor.
    pub window: bool,
    pub button: usize,
    typing: Option<Typing>,
    /// Course pictures: the UI texture name by course key (None: none).
    thumbs: HashMap<String, Option<String>>,
    pub tick: i32,
    /// An upload's call id.
    upload: Option<u32>,
}

impl CoursebotScene {
    pub fn new(ui: &Ui, mode: BotMode, cx: &mut Cx) -> Option<Self> {
        let mut frame = ui.layout(FRAME)?;
        let mut list = ui.layout(LIST)?;
        let mut info = ui.layout(INFO)?;
        frame.play_loop("Loop", 0);
        list.play_loop("Loop", 0);
        frame.text_from("L_MyDataBtn_00/T_Btn_00", "L_MyDataBtn_00-T_Btn_00");
        frame.text_from("L_DLDataBtn_00/T_Btn_00", "L_DLDataBtn_00-T_Btn_00");
        // no Super World filter, no upload counter (our server has no limit) [departure]
        frame.show("L_SearchBtn_00", false);
        frame.show("L_UploadInfo_00", false);
        list.show("L_SearchBtn_00", false);
        list.show("L_CourseDataTarget_00", false);
        list.show("L_CourseDataDrag_00", false);
        // the outer rows scroll in behind the scissor (not clipped here): hidden
        list.show("L_CourseDataList_00", false);
        list.show("L_CourseDataList_04", false);
        for (pane, label) in [
            ("L_Edit_00/T_Btn_00", "L_Edit_00-T_Btn_00"),
            ("L_Play_00/T_Btn_00", "L_Play_00-T_Btn_00"),
            ("L_Upload_00/T_Btn_00", "L_Upload_00-T_Btn_00"),
            ("L_CoursName_00/T_Btn_00", "L_CoursName_00-T_Btn_00"),
            ("L_CourseInfo_00/T_Btn_00", "L_CourseInfo_00-T_Btn_00"),
            ("L_DLPlayBtn_00/T_Btn_00", "L_DLPlayBtn_00-T_Btn_00"),
            ("L_CoopModeBtn_01/T_Btn_00", "L_Edit_00-T_Btn_00"),
        ] {
            info.text_from(pane, label);
        }
        info.text_from("L_Del_00/T_Btn_00", "0008");
        info.text_from("L_DLDel_00/T_Btn_00", "0008");
        for p in [
            "L_CoopModeBtn_00",
            "L_DLDel_00",
            "L_IDBtn_00",
            "N_Player_00",
        ] {
            info.show(p, false);
        }
        let mut s = Self {
            ui: ui.clone(),
            frame,
            list,
            info,
            cursor: MenuCursor::new(ui),
            mode,
            tab: 0,
            my: Vec::new(),
            downloaded: Vec::new(),
            sel: 0,
            first: 0,
            window: false,
            button: 0,
            typing: None,
            thumbs: HashMap::new(),
            tick: 0,
            upload: None,
        };
        s.reload(cx);
        // Save starts on the first free slot, the others on the first course [inf]
        s.sel = match mode {
            BotMode::Save => s.my.iter().position(Option::is_none).unwrap_or(0),
            _ => s.my.iter().position(Option::is_some).unwrap_or(0),
        };
        s.scroll_to_selection();
        s.fill(cx);
        Some(s)
    }

    /// Read the slots and the downloaded courses again.
    pub fn reload(&mut self, cx: &mut Cx) {
        self.my = cx.host.coursebot_slots();
        self.my.resize(COURSEBOT_SLOTS, None);
        let mine: Vec<String> = self.my.iter().flatten().map(|e| e.id.clone()).collect();
        self.downloaded = cx
            .host
            .library()
            .into_iter()
            .filter(|e| !e.built_in && !mine.contains(&e.id))
            .take(DOWNLOADED)
            .collect();
    }

    fn slots(&self) -> usize {
        if self.tab == 0 {
            COURSEBOT_SLOTS
        } else {
            DOWNLOADED
        }
    }

    /// The course in slot `i` of the current tab.
    pub fn course(&self, i: usize) -> Option<&CourseEntry> {
        if self.tab == 0 {
            self.my.get(i).and_then(Option::as_ref)
        } else {
            self.downloaded.get(i)
        }
    }

    fn header(&self) -> &'static str {
        match self.mode {
            BotMode::Browse => "0000",
            BotMode::Save => "0001",
            BotMode::Load => "0004",
        }
    }

    fn scroll_to_selection(&mut self) {
        let row = self.sel / COLS;
        if row < self.first {
            self.first = row;
        } else if row >= self.first + ROWS {
            self.first = row + 1 - ROWS;
        }
        let rows = self.slots().div_ceil(COLS);
        self.first = self.first.min(rows.saturating_sub(ROWS));
    }

    /// The pane of slot `i` (it must be shown).
    fn slot_pane(&self, i: usize) -> String {
        let row = i / COLS - self.first + 1;
        format!("L_CourseDataList_{row:02}/L_CourseBtn_{:02}", i % COLS)
    }

    /// A course picture (asked of the host once per course).
    fn thumbnail(&mut self, cx: &mut Cx, e: &CourseEntry) -> Option<String> {
        if let Some(t) = self.thumbs.get(&e.key) {
            return t.clone();
        }
        let name = cx
            .host
            .course_data(&Source::Library(e.id.clone()))
            .ok()
            .and_then(|b| cx.host.course_thumbnail(&b))
            .map(|i| {
                let name = format!("thumb_{}", e.key);
                let tex = mmk_assets::Mtex {
                    format: mmk_assets::MtexFormat::Rgba8,
                    variant: mmk_assets::MtexVariant::Srgb,
                    width: i.width,
                    height: i.height,
                    channel_select: [2, 3, 4, 5],
                    data: i.rgba,
                };
                self.ui.cache_mut().add_texture_file("Thumb", &name, tex);
                name
            });
        self.thumbs.insert(e.key.clone(), name.clone());
        name
    }

    /// Put the slots into the rows, the header, the tabs.
    pub fn fill(&mut self, cx: &mut Cx) {
        let header = self.header();
        self.frame.text_from("T_IndexInfo_00", header);
        self.frame.text_from("T_IndexInfo_01", header);
        for (k, t) in ["L_MyDataBtn_00", "L_DLDataBtn_00"].iter().enumerate() {
            if k == self.tab {
                self.frame.hold_end(&format!("{t}/Decide"));
            } else {
                self.frame.stop(&format!("{t}/Decide"));
                self.frame.hold(&format!("{t}/Select"), 0.0);
            }
        }
        // the Downloaded tab only while browsing [inf: saving and loading are about My Courses]
        self.frame
            .show("L_DLDataBtn_00", self.mode == BotMode::Browse);
        for r in 0..ROWS {
            for c in 0..COLS {
                let i = (self.first + r) * COLS + c;
                let pane = format!("L_CourseDataList_{:02}/L_CourseBtn_{c:02}", r + 1);
                if i >= self.slots() {
                    self.list.hold(&format!("{pane}/Vanish"), 1.0);
                    continue;
                }
                self.list.hold(&format!("{pane}/Vanish"), 0.0);
                let course = self.course(i).cloned();
                let sel = if i == self.sel && !self.window {
                    2.0
                } else {
                    0.0
                };
                self.list.hold(&format!("{pane}/Select"), sel);
                for p in [
                    "L_ClearIcon_00",
                    "L_MyWIcon_00",
                    "L_UploadCheck_00",
                    "N_Load_00",
                ] {
                    self.list.show(&format!("{pane}/{p}"), false);
                }
                match course {
                    Some(e) => {
                        self.list.show(&format!("{pane}/N_NoSave_00"), false);
                        self.list.show(&format!("{pane}/N_Save_00"), true);
                        self.list.show(&format!("{pane}/N_Title_00"), true);
                        let title = format!("{pane}/T_Title_00");
                        self.list.text(&title, &e.name);
                        // a long name is squeezed into the label [inf]
                        let w = self.list.text_width(&title).unwrap_or(0.0);
                        let squeeze = if w > TITLE_W { TITLE_W / w } else { 1.0 };
                        self.list.set_pane(
                            &title,
                            mmk_ui::PaneOverride {
                                scale: Some([squeeze, 1.0]),
                                ..Default::default()
                            },
                        );
                        let thumb = format!("{pane}/P_DummyThumbnail_00");
                        match self.thumbnail(cx, &e) {
                            Some(t) => {
                                self.list.image(&thumb, "Thumb", &t);
                            }
                            None => self.list.show(&thumb, false),
                        }
                    }
                    None => {
                        self.list.show(&format!("{pane}/N_NoSave_00"), true);
                        self.list.show(&format!("{pane}/N_Save_00"), false);
                        self.list.show(&format!("{pane}/N_Title_00"), false);
                    }
                }
            }
        }
    }

    /// The course window's buttons now.
    fn buttons(&self) -> Vec<(&'static str, Btn)> {
        let Some(_) = self.course(self.sel) else {
            return vec![("L_ModeBtn_00", Btn::Mode)];
        };
        match (self.mode, self.tab) {
            (BotMode::Browse, 0) => MY_BUTTONS.to_vec(),
            (BotMode::Browse, _) => DL_BUTTONS.to_vec(),
            _ => vec![("L_ModeBtn_00", Btn::Mode)],
        }
    }

    fn open_window(&mut self, cx: &mut Cx) {
        let e = self.course(self.sel).cloned();
        self.window = true;
        self.button = 0;
        let i = &mut self.info;
        i.stop("Out");
        i.play("In", self.tick);
        let (mode, msg, btn) = match (&e, self.mode, self.tab) {
            (None, BotMode::Save, _) => (2.0, "0002", "0007"),
            (None, _, _) => (2.0, "0002", "0006"),
            (Some(_), BotMode::Save, _) => (2.0, "0003", "0007"),
            (Some(_), BotMode::Load, _) => (2.0, "0001", "0005"),
            (Some(_), BotMode::Browse, 0) => (0.0, "", ""),
            (Some(_), BotMode::Browse, _) => (1.0, "", ""),
        };
        i.hold("TypeMode", mode);
        i.hold("TypeInfo", if e.is_some() { 0.0 } else { 1.0 });
        if mode == 2.0 {
            i.text_from("T_ModeMsg_00", msg);
            i.text_from("L_ModeBtn_00/T_Btn_00", btn);
        }
        i.show("L_CoopModeBtn_01", true);
        match &e {
            Some(e) => {
                i.text("T_CourseName_00", &e.name);
                let d = if e.description.is_empty() {
                    i.message("0000").unwrap_or_default()
                } else {
                    e.description.clone()
                };
                i.wrapped_text("T_CourseInfo_00", &d, 560.0, 2);
                let (day, time) = match e.modified {
                    Some(t) => {
                        let (y, mo, d, h, mi) = crate::civil(t + cx.host.local_offset(t));
                        (
                            i.format(
                                "Day_0000",
                                &[format!("{mo:02}"), format!("{d:02}"), y.to_string()],
                            ),
                            i.format("Time_0000", &[format!("{h:02}"), format!("{mi:02}")]),
                        )
                    }
                    None => (None, None),
                };
                i.text("T_Day_00", &day.unwrap_or_default());
                i.text("T_Time_00", &time.unwrap_or_default());
            }
            None => i.text("T_CourseName_00", ""),
        }
        for p in [
            "L_ClearIcon_00",
            "N_GoodCheck_00",
            "N_Icon_00",
            "N_Mission_00",
        ] {
            i.show(p, false);
        }
        if let Some(e) = &e {
            match self.thumbnail(cx, e) {
                Some(t) => {
                    self.info.show("P_DummyThumbnail_00", true);
                    self.info.image("P_DummyThumbnail_00", "Thumb", &t);
                }
                None => self.info.show("P_DummyThumbnail_00", false),
            }
        }
        self.ui.sound_button(LIST, "L_CourseBtn", Event::Decide);
        self.fill(cx);
    }

    fn close_window(&mut self, cx: &mut Cx) {
        self.window = false;
        self.typing = None;
        self.info.stop("In");
        self.info.play("Out", self.tick);
        self.fill(cx);
    }

    pub fn ui(&self) -> Ui {
        self.ui.clone()
    }

    /// Captures: the selected slot's window.
    pub fn debug_window(&mut self, cx: &mut Cx) {
        self.open_window(cx);
    }

    pub fn typing(&self) -> bool {
        self.typing.is_some()
    }

    pub fn step(&mut self, input: &UiInput, cx: &mut Cx) -> Option<BotEvent> {
        self.tick += 1;
        self.upload_answer(cx);
        if self.typing.is_some() {
            self.type_key(input.text, cx);
            return None;
        }
        let pressed = input.pressed;
        if self.window {
            return self.window_step(pressed, input.click, cx);
        }
        if pressed.has(Buttons::BACK) {
            self.ui.sound_back(FRAME, "normal");
            return Some(BotEvent::Back);
        }
        if let Some(p) = input.click {
            let tabs = [
                "L_MyDataBtn_00/B_Btn_00".to_string(),
                "L_DLDataBtn_00/B_Btn_00".to_string(),
            ];
            if let Some(t) = hit(&mut self.frame, self.tick, &tabs, p) {
                self.switch_tab(t, cx);
                return None;
            }
            let panes: Vec<String> = (0..ROWS * COLS)
                .map(|k| self.first * COLS + k)
                .filter(|&i| i < self.slots())
                .map(|i| format!("{}/B_Btn_00", self.slot_pane(i)))
                .collect();
            if let Some(k) = hit(&mut self.list, self.tick, &panes, p) {
                let i = self.first * COLS + k;
                if i != self.sel {
                    self.sel = i;
                    self.fill(cx);
                }
                return self.decide(cx);
            }
            return None;
        }
        let n = self.slots() as i64;
        let step = match pressed {
            b if b.has(Buttons::RIGHT) => 1,
            b if b.has(Buttons::LEFT) => -1,
            b if b.has(Buttons::DOWN) => COLS as i64,
            b if b.has(Buttons::UP) => -(COLS as i64),
            _ => 0,
        };
        if step != 0 {
            let to = self.sel as i64 + step;
            if (0..n).contains(&to) {
                self.sel = to as usize;
                self.scroll_to_selection();
                self.ui.sound_button(LIST, "L_CourseBtn", Event::Select);
                self.fill(cx);
            }
        }
        if pressed.has(Buttons::CONFIRM) {
            return self.decide(cx);
        }
        None
    }

    /// Confirm on the selected slot.
    fn decide(&mut self, cx: &mut Cx) -> Option<BotEvent> {
        // saving into an empty slot needs no question [inf]
        if self.mode == BotMode::Save && self.course(self.sel).is_none() {
            return Some(BotEvent::SaveTo(self.sel));
        }
        self.open_window(cx);
        None
    }

    fn switch_tab(&mut self, t: usize, cx: &mut Cx) {
        if t == self.tab || (t == 1 && self.mode != BotMode::Browse) {
            return;
        }
        self.tab = t;
        self.sel = 0;
        self.first = 0;
        self.frame.play("ChangeCategory", self.tick);
        self.ui.sound_button(
            FRAME,
            ["L_MyDataBtn_00", "L_DLDataBtn_00"][t],
            Event::Decide,
        );
        self.fill(cx);
    }

    fn window_step(
        &mut self,
        pressed: Buttons,
        click: Option<[f32; 2]>,
        cx: &mut Cx,
    ) -> Option<BotEvent> {
        let buttons = self.buttons();
        let mut decide = pressed.has(Buttons::CONFIRM);
        if pressed.has(Buttons::BACK) {
            self.ui.sound_back(INFO, "normal");
            self.close_window(cx);
            return None;
        }
        if let Some(p) = click {
            let panes: Vec<String> = buttons
                .iter()
                .map(|b| format!("{}/B_Btn_00", b.0))
                .collect();
            match hit(&mut self.info, self.tick, &panes, p) {
                Some(k) => {
                    self.button = k;
                    decide = true;
                }
                None => {
                    self.close_window(cx);
                    return None;
                }
            }
        }
        let n = buttons.len();
        if pressed.has(Buttons::UP) || pressed.has(Buttons::LEFT) {
            self.button = (self.button + n - 1) % n;
            self.ui
                .sound_button(INFO, buttons[self.button].0, Event::Select);
        } else if pressed.has(Buttons::DOWN) || pressed.has(Buttons::RIGHT) {
            self.button = (self.button + 1) % n;
            self.ui
                .sound_button(INFO, buttons[self.button].0, Event::Select);
        }
        if !decide {
            return None;
        }
        let (pane, what) = buttons[self.button.min(n - 1)];
        self.ui.sound_button(INFO, pane, Event::Decide);
        self.info.play(&format!("{pane}/Decide"), self.tick);
        let e = self.course(self.sel).cloned();
        match (what, e) {
            (Btn::Mode, None) => match self.mode {
                BotMode::Save => Some(BotEvent::SaveTo(self.sel)),
                _ => Some(BotEvent::New),
            },
            (Btn::Mode, Some(e)) => match self.mode {
                BotMode::Save => Some(BotEvent::SaveTo(self.sel)),
                _ => Some(edit(&e, self.tab == 0)),
            },
            (Btn::Make, Some(e)) => Some(edit(&e, self.tab == 0)),
            (Btn::Play, Some(e)) => Some(BotEvent::Play(Box::new(e))),
            (Btn::Upload, Some(e)) => {
                self.start_upload(cx, &e);
                None
            }
            (Btn::Name | Btn::Description, Some(e)) => {
                let description = what == Btn::Description;
                let text = if description { e.description } else { e.name };
                self.typing = Some(Typing { description, text });
                self.show_typing();
                None
            }
            (Btn::Delete, Some(_)) => {
                if cx.host.delete_slot(self.sel) {
                    self.reload(cx);
                }
                self.close_window(cx);
                None
            }
            _ => None,
        }
    }

    fn show_typing(&mut self) {
        let Some(t) = &self.typing else { return };
        let caret = if self.tick / 30 % 2 == 0 { "_" } else { "" };
        let s = format!("{}{caret}", t.text);
        if t.description {
            self.info.wrapped_text("T_CourseInfo_00", &s, 560.0, 2);
        } else {
            self.info.text("T_CourseName_00", &s);
        }
    }

    /// A key typed into the name / description (Return keeps it, Esc drops it).
    fn type_key(&mut self, key: Option<char>, cx: &mut Cx) {
        let Some(t) = &mut self.typing else { return };
        // the header's limits: 32 characters of name, 75 of description [ext: the course
        // format's fields]
        let max = if t.description { 75 } else { 32 };
        match key {
            Some('\u{8}') => {
                t.text.pop();
            }
            Some('\u{1b}') => {
                self.typing = None;
                self.open_window(cx);
                return;
            }
            Some('\n') => {
                let t = self.typing.take().expect("typing");
                self.rename(cx, t);
                return;
            }
            Some(c) if t.text.chars().count() < max => t.text.push(c),
            _ => {}
        }
        self.show_typing();
    }

    /// Write a new name / description into the slot's course.
    fn rename(&mut self, cx: &mut Cx, t: Typing) {
        let slot = self.sel;
        let Some(e) = self.course(slot).cloned() else {
            return;
        };
        let done = cx
            .host
            .course_data(&Source::Library(e.id.clone()))
            .and_then(|b| mmk_editor::Editor::open(&b).map_err(|e| format!("{e:?}")))
            .and_then(|mut ed| {
                if t.description {
                    ed.set_description(&t.text);
                } else {
                    ed.set_name(&t.text);
                }
                let b = ed.save().map_err(|e| format!("{e:?}"))?;
                cx.host.save_slot(slot, &b)
            });
        if let Err(e) = done {
            cx.toast(&format!("Not saved: {e}"));
        }
        self.reload(cx);
        self.open_window(cx);
    }

    fn start_upload(&mut self, cx: &mut Cx, e: &CourseEntry) {
        match cx.host.course_data(&Source::Library(e.id.clone())) {
            Ok(b) => {
                let args = serde_json::json!({ "course": mmk_formats::hex_rle::hex_rle(&b) });
                self.upload = cx.online.call(cx.host, "upload", args);
                if self.upload.is_none() {
                    cx.toast(UPLOAD_NEEDS_SERVER);
                }
            }
            Err(e) => cx.toast(&format!("Not uploaded: {e}")),
        }
    }

    fn upload_answer(&mut self, cx: &mut Cx) {
        let Some(id) = self.upload else { return };
        let Some(a) = cx.online.take(id) else { return };
        self.upload = None;
        match a {
            Ok(v) => cx.toast(&format!("Uploaded: {}", v["code"].as_str().unwrap_or("?"))),
            Err(e) => cx.toast(&format!("Not uploaded: {e}")),
        }
    }

    pub fn draw(&mut self, out: &mut Vec<DrawItem>) {
        let tick = self.tick;
        if self.typing.is_some() && tick % 30 == 0 {
            self.show_typing();
        }
        // the slots under the robot's frame
        push_layout(out, &mut self.list, tick);
        push_layout(out, &mut self.frame, tick);
        if self.window {
            push_layout(out, &mut self.info, tick);
            let b = self.buttons();
            let pane = format!("{}/B_Btn_00", b[self.button.min(b.len() - 1)].0);
            self.cursor.place(&mut self.info, &pane, tick, 1.0);
        } else {
            let pane = format!("{}/B_Btn_00", self.slot_pane(self.sel));
            self.cursor.place(&mut self.list, &pane, tick, 1.0);
        }
        self.cursor.draw(tick, out);
    }
}

/// Opening course `e` in the maker: My Courses save back to their slot; a downloaded course is
/// someone else's (a new course with its credit when it has a code).
fn edit(e: &CourseEntry, mine: bool) -> BotEvent {
    let based_on = (!mine).then(|| match (&e.meta.code, &e.meta.maker) {
        (Some(c), Some(m)) => format!("based on {c} by {m}"),
        (Some(c), None) => format!("based on {c}"),
        _ => format!("based on {}", e.name),
    });
    BotEvent::Edit {
        source: Source::Library(e.id.clone()),
        entry: mine.then(|| e.id.clone()),
        based_on,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Saves;
    use crate::play::tests::{MockHost, with};

    fn press(b: Buttons) -> UiInput {
        UiInput {
            pressed: b,
            ..Default::default()
        }
    }

    #[test]
    fn save_load_make_and_rename() {
        let Some(ui) = crate::testing::smm2() else {
            return;
        };
        let (mut host, mut saves) = (MockHost::default(), Saves::default());
        let course = crate::maker::new_course("M1").save().unwrap();
        // saving: the first free slot, at once
        let mut s = with(&mut host, &mut saves, |cx| {
            CoursebotScene::new(&ui, BotMode::Save, cx).unwrap()
        });
        assert_eq!(s.sel, 0);
        let ev = with(&mut host, &mut saves, |cx| {
            s.step(&press(Buttons::CONFIRM), cx)
        });
        assert_eq!(ev, Some(BotEvent::SaveTo(0)));
        // (the owner saves; the mock lists it in slot 0)
        crate::host::Host::save_slot(&mut host, 0, &course).unwrap();
        // browsing: the course's window, Make opens it as ours
        let mut s = with(&mut host, &mut saves, |cx| {
            CoursebotScene::new(&ui, BotMode::Browse, cx).unwrap()
        });
        assert!(s.course(0).is_some());
        with(&mut host, &mut saves, |cx| {
            s.step(&press(Buttons::CONFIRM), cx)
        });
        assert!(s.window);
        let ev = with(&mut host, &mut saves, |cx| {
            s.step(&press(Buttons::CONFIRM), cx)
        });
        assert!(matches!(
            ev,
            Some(BotEvent::Edit {
                entry: Some(_),
                based_on: None,
                ..
            })
        ));
        // Name: type, Return writes it into the slot
        s.button = MY_BUTTONS.iter().position(|b| b.1 == Btn::Name).unwrap();
        with(&mut host, &mut saves, |cx| {
            s.step(&press(Buttons::CONFIRM), cx)
        });
        assert!(s.typing());
        for c in "\u{8}\u{8}\u{8}\u{8}\u{8}\u{8}\u{8}\u{8}\u{8}\u{8}Bot\n".chars() {
            let i = UiInput {
                text: Some(c),
                ..Default::default()
            };
            with(&mut host, &mut saves, |cx| s.step(&i, cx));
        }
        assert!(!s.typing());
        assert_eq!(s.course(0).unwrap().name, "Bot");
        // an empty slot: Make New Course
        s.window = false;
        s.sel = 1;
        with(&mut host, &mut saves, |cx| {
            s.step(&press(Buttons::CONFIRM), cx)
        });
        let ev = with(&mut host, &mut saves, |cx| {
            s.step(&press(Buttons::CONFIRM), cx)
        });
        assert_eq!(ev, Some(BotEvent::New));
    }
}
