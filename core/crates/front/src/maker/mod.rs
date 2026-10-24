//! The Course Maker with SMM2's own maker layouts (research/specs/maker.md Part C, private) over
//! the `mmk-editor` core, the course drawn by the host as a still view
//! ([`Host::edit_show`] / [`Host::edit_camera`]):
//! - `Edit_MainPage_00`: the palette bar `L_PaletteSet_00` (part slots with the parts' pictures
//!   from [`Host::part_icon`]; the search button opens the part window), the left column
//!   `L_LeftParts_00` (Game Style and Course Theme windows), the right column `L_RightParts_00`
//!   (main menu, Save/Load, eraser, Reset Rocket, Undodog), the clapperboard `L_Play_00` (test
//!   play), each with its own In [obs data];
//! - the part window `L_RingMenu_00` (four categories, pages of up to 13 parts round the ring,
//!   laid out by the ring's own `Type` animation) [obs data; the pages' contents are ours, see
//!   `catalog`];
//! - `Cmn_MainMenu_00` (Back to Title, Coursebot, Story Mode, Course World) and
//!   `Edit_SaveLoadMenu_00` (Save / Save As);
//! - over the course: the grid, the frame round the tile under the cursor and the glove cursor
//!   (the `EditModel` textures of the UI import, else drawn lines) as layout quads.
//!
//! Input: the cursor follows the mouse / touch, or moves by tiles with the d-pad (the view
//! scrolls when it nears an edge); confirm (or a click) puts the selected part down (ground,
//! blocks and coins paint while held), on a part it picks the part up and drops it where the
//! cursor is let go; erase with the eraser, the back button held or the right mouse button;
//! [`keys`] for undo / redo, the part window, the palette slots and test play.
//!
//! Test play: the clapperboard (or the test-play key) plays from the maker's Mario, who stands
//! where the last test play ended (the course start at first); held for its HoldWait it plays
//! from the start ("Play from start"); the play-from-cursor key starts at the cursor. A death, a
//! clear, or the test-play key brings the maker back [inf]. Back in the maker, Mario's Trail
//! shows the pictures of Mario the test play took (`trail`; the left column's
//! `L_ZanzoBtn_00` switches it on / off).
//!
//! Saving: the Coursebot through [`Host::save_course`]; an edited copy of someone else's course
//! (`based_on`) always saves as a new course and carries "based on <code> by <maker>" in its
//! description (owner's rule). The course being made is kept as a draft
//! ([`Host::store_value`] "maker.draft") so the maker reopens on it.

pub mod art;
pub mod attr;
pub mod catalog;
pub mod help;
pub mod liquid;
pub mod mission;
pub mod modes;
pub mod scroll;
pub mod subarea;
pub mod trail;
pub mod view;

use mmk_editor::clearpipe::{CLEAR_PIPE, PipeEnd};
use mmk_editor::parts::Dir;
use mmk_editor::{DEFAULT_FLAGS, EditError, Editor, Rect};

use crate::host::{Host, Source};
use crate::layout::{Layout, Ui};
use crate::menu::{Buttons, MenuCursor, UiInput, hit, nearest};
use crate::screens::{CardInfo, CourseScreens};
use crate::sound::Event;
use crate::{DrawItem, push_layout};
use catalog::{Entry, Tool};

/// Maker keys of [`crate::FrontInput::keys`] (bits).
pub mod keys {
    pub const UNDO: u16 = 1;
    pub const REDO: u16 = 2;
    /// Open / close the part window.
    pub const PARTS: u16 = 4;
    /// Next / previous palette slot.
    pub const NEXT: u16 = 8;
    pub const PREV: u16 = 16;
    /// Test play from the cursor.
    pub const PLAY_HERE: u16 = 32;
    /// Toggle the eraser.
    pub const ERASER: u16 = 64;
    /// Toggle the main / sub area.
    pub const AREA: u16 = 128;
    /// Held: presses drag a multigrab rectangle (the game's ZL / ZR).
    pub const MULTI: u16 = 256;
    /// Copy the multigrab selection to the cursor.
    pub const COPY: u16 = 512;
}

/// The start and goal grounds' edit sound users (`<style>_EditGroundStart` / `…GroundGoal`).
const START_GROUND: i16 = 37;
const GOAL_GROUND: i16 = 26;
/// A clear pipe starts once the cursor is this far (world px) from the corner the press was
/// on, and is taken back while the cursor is within `CLEAR_PIPE_BACK` of it again
/// (0x7100fa9090) [obs].
const CLEAR_PIPE_START: f32 = 24.0;
const CLEAR_PIPE_BACK: f32 = 16.0;
/// What a part keeps when it is put into another: wings, alternate form, size, parachute.
const CHILD_KEEPS: u32 = 0x2 | 0x4 | 0x6000 | 0x8000;
const PAGE: &str = "Edit_MainPage_00";
const MENU: &str = "Cmn_MainMenu_00";
const SAVE: &str = "Edit_SaveLoadMenu_00";
/// Course view: 384 × 216 world px fill the 1280 × 720 layout.
pub const VIEW_W: f32 = 384.0;
pub const VIEW_H: f32 = 216.0;
pub const SCALE: f32 = 1280.0 / VIEW_W;
const TILE: f32 = 16.0;
/// Ring icons per page (`Edit_RingIconSet_00` has 13) [obs data].
const RING: usize = 13;
/// Palette slots (`L_PaletteBtn_00…11`) [obs data].
const SLOTS: usize = 12;
/// Cursor auto-repeat with the d-pad: first step, then one per `REPEAT` ticks after `DELAY`.
const DELAY: i32 = 14;
const REPEAT: i32 = 4;
/// Ticks after a clear / death before test play returns to the maker.
const CLEAR_TICKS: i32 = 200;
/// Mario's Trail shown ("1", the default [?: a new save's]) or not ("0"): the game keeps the
/// choice in the save data (flags word bit 0x20) [obs 0x71018c9280].
pub const TRAIL_KEY: &str = "maker.trail";

/// An edit sound: the part whose sound user plays it, the key, and its `MusicalScale`.
type PutSound = (i16, &'static str, Option<f32>);

/// `Tower_stack`'s `MusicalScale`: the stacked part's tile row mod 28 (the sound's pitch curve
/// rises a semitone a row, 0.5 → 4.0 over 36) [obs 0x7100eb0080].
fn tower_scale(row: i32) -> f32 {
    row.max(0).rem_euclid(28) as f32
}

/// Input for one maker tick.
#[derive(Clone, Copy, Debug, Default)]
pub struct MakerInput {
    pub ui: UiInput,
    /// START held.
    pub start: bool,
    /// The test-play key (the skip key, − / Tab) held.
    pub play: bool,
    /// Pointer (mouse / touch) position, layout units; buttons held.
    pub pointer: Option<[f32; 2]>,
    pub down: bool,
    pub alt: bool,
    /// [`keys`] held.
    pub keys: u16,
}

/// What the maker asks of its owner.
#[derive(Clone, Debug, PartialEq)]
pub enum MakerEvent {
    /// Leave to the title.
    Title,
    /// Leave to a mode of the main menu.
    StoryMode,
    CourseWorld,
    Coursebot,
    /// The Coursebot picks a slot for the course (Save As) / a course to load.
    SaveAs,
    Load,
}

/// How the maker opens.
#[derive(Clone, Debug, PartialEq)]
pub enum MakerStart {
    /// The draft if there is one, else a new course.
    Resume,
    /// A new course in a game style ("M1" …).
    New(String),
    /// A library / server course: its Coursebot id (saving overwrites it) or, for someone
    /// else's course, the credit line its copies carry.
    Open {
        source: Source,
        entry: Option<String>,
        based_on: Option<String>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Edit,
    /// A part's attribute balloon (wings, parachute, big, forms).
    Attr,
    Ring,
    Menu,
    Save,
    /// A left-column window: 0 game style, 1 course theme, 2 timer, 3 autoscroll.
    Window(u8),
    /// The clear-condition window.
    Mission,
    /// The sub-area's orientation window.
    SubArea,
    /// The Water Level mode (`liquid`).
    Water,
    /// The Custom Scroll mode (`scroll`).
    Scroll,
    /// A one-time tip (`help`).
    Help,
    Test,
}

/// A press in progress over the course.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Drag {
    /// Putting down / painting (one checkpoint for the whole press).
    Place {
        last: (i32, i32),
        changed: bool,
    },
    Erase {
        changed: bool,
    },
    /// Carrying a part picked up at `from` (its index, grab tile); `held` counts the ticks it
    /// has been held without moving (−1 once it moved): long enough opens its attribute
    /// balloon.
    Carry {
        index: usize,
        from: (i32, i32),
        held: i32,
    },
    /// Dragging the scroll bar's handle.
    Slider,
    /// Dragging the start up / down (its height), or the goal (the course's length, the goal's
    /// height); `changed` as for `Place`.
    Start {
        changed: bool,
    },
    Goal {
        grab: i32,
        changed: bool,
    },
    /// Drawing a multigrab rectangle from `from`.
    Select {
        from: (i32, i32),
    },
    /// Carrying the multigrab selection, grabbed at `grab`; `held` as for `Carry` (on a part:
    /// the balloon for the whole selection).
    MoveSelection {
        grab: (i32, i32),
        held: i32,
    },
    /// Stretching a part by its edge: its width (right column) and / or height (top row).
    Resize {
        index: usize,
        width: bool,
        height: bool,
        changed: bool,
    },
    /// Drawing a clear pipe: from grid corner `corner` until the cursor is far enough to start
    /// one (`slot` None), then the pipe's `end` follows the cursor; `fresh` while it is still
    /// the new two-tile pipe (taken back near the corner).
    ClearPipe {
        corner: (i32, i32),
        slot: Option<usize>,
        end: PipeEnd,
        fresh: bool,
        changed: bool,
    },
    /// Carrying a clear pipe grabbed at `from`.
    CarryPipe {
        slot: usize,
        from: (i32, i32),
    },
}

struct Test {
    screens: Option<CourseScreens>,
    /// Ticks since the clear / the death began.
    ended: Option<i32>,
    was_dying: bool,
}

/// The ring menu's state: category, page, selection.
#[derive(Clone, Copy, Debug, Default)]
struct Ring {
    category: usize,
    page: usize,
    sel: usize,
}

pub struct MakerFlow {
    ui: Ui,
    pub editor: Editor,
    /// The Coursebot entry this course saves to.
    pub entry: Option<String>,
    /// Someone else's course: "based on <code> by <maker>".
    pub based_on: Option<String>,
    pub page: Option<Layout>,
    menu: Option<Layout>,
    save: Option<Layout>,
    cursor: MenuCursor,
    mode: Mode,
    pub tick: i32,
    /// View bottom-left, world px.
    pub cam: [f32; 2],
    /// The tile under the cursor.
    pub cur: (i32, i32),
    /// Where the glove is drawn (layout units).
    glove: [f32; 2],
    pointer_was: Option<[f32; 2]>,
    pub palette: Vec<Tool>,
    pub slot: usize,
    pub eraser: bool,
    drag: Option<Drag>,
    held: Buttons,
    keys_held: u16,
    down_was: bool,
    alt_was: bool,
    start_was: bool,
    play_was: bool,
    play_hold: i32,
    repeat: i32,
    ring: Ring,
    menu_sel: usize,
    save_sel: usize,
    window_sel: usize,
    /// The timer window's value until OK.
    window_timer: i16,
    /// An upload waiting for its answer (`Online::call` id).
    upload_call: Option<u32>,
    /// Captures: scripted input by maker tick.
    script: Vec<(i32, String)>,
    /// The last tick a pointer was seen.
    pointer_tick: i32,
    /// The multigrab selection.
    pub selection: Option<Rect>,
    /// The clear-condition window while open.
    mission: Option<mission::MissionWindow>,
    subarea: Option<subarea::SubAreaWindow>,
    /// The animation that opened the left window.
    window_tab: &'static str,
    /// The course changed since the host last showed it.
    dirty: bool,
    /// The host draws the course.
    pub shown: bool,
    test: Option<Test>,
    /// Edit sounds to play this tick: (course object id, key, `MusicalScale`)
    /// (research/specs/maker.md C9).
    pub sounds: Vec<(i16, &'static str, Option<f32>)>,
    /// The tile the cursor was on when a carry last sounded `move`.
    move_cur: (i32, i32),
    /// The music last asked of the host (None: nothing asked yet, so ask).
    music_sent: Option<Option<(String, u8)>>,
    /// The maker's Mario: where test play starts (None: the course start); area.
    pub mario: Option<(usize, [f32; 2])>,
    /// Part pictures by (style, part, flags): the UI cache texture name, None when the host had
    /// none.
    icons: std::collections::HashMap<(String, i16, u32), Option<String>>,
    /// The attribute balloon while open.
    attr: Option<attr::Balloon>,
    /// The `EditModel` textures found in the import (name → UI texture id).
    textures: std::collections::HashMap<&'static str, u32>,
    /// Changes since the last save.
    pub unsaved: bool,
    /// Mario's Trail: the last test play's pictures, and whether they are shown.
    pub trail: trail::Trail,
    pub trail_on: bool,
    /// The tick the button's "Mario's Trail" label goes (its Cancel).
    trail_label: Option<i32>,
    /// The Water Level and Custom Scroll modes' presses.
    water: liquid::LiquidEdit,
    scroll: scroll::ScrollEdit,
    /// The one-time tip while it is up, and one to show once the window closes.
    help: Option<help::HelpTip>,
    pending_tip: Option<help::Tip>,
}

impl MakerFlow {
    pub fn new(ui: &Ui, cx: &mut crate::Cx, start: MakerStart) -> Self {
        let (editor, entry, based_on) = match start {
            MakerStart::Resume => match load_draft(cx.host) {
                Some(d) => d,
                None => (new_course("M1"), None, None),
            },
            MakerStart::New(style) => (new_course(&style), None, None),
            MakerStart::Open {
                source,
                entry,
                based_on,
            } => match cx
                .host
                .course_data(&source)
                .and_then(|b| Editor::open(&b).map_err(|e| format!("{e:?}")))
            {
                Ok(e) => (e, entry, based_on),
                Err(e) => {
                    cx.toast(&format!("This course can't be opened: {e}"));
                    (new_course("M1"), None, None)
                }
            },
        };
        // the Mods page's SMM1 rule: a pipe may go across a pipe
        let mut editor = editor;
        editor.smm1_overlap_pipes = crate::mods::overlap_pipes(cx.host);
        let mut f = Self {
            ui: ui.clone(),
            editor,
            entry,
            based_on,
            page: ui.layout(PAGE),
            menu: ui.layout(MENU),
            save: ui.layout(SAVE),
            cursor: MenuCursor::new(ui),
            mode: Mode::Edit,
            tick: 0,
            cam: [0.0, 0.0],
            cur: (8, 2),
            glove: [0.0, 0.0],
            pointer_was: None,
            palette: Vec::new(),
            slot: 0,
            eraser: false,
            drag: None,
            held: Buttons::NONE,
            keys_held: 0,
            down_was: false,
            alt_was: false,
            start_was: true,
            play_was: true,
            play_hold: -1,
            repeat: 0,
            ring: Ring::default(),
            menu_sel: 1,
            save_sel: 0,
            window_sel: 0,
            window_timer: 300,
            upload_call: None,
            script: Vec::new(),
            pointer_tick: -1000,
            selection: None,
            mission: None,
            subarea: None,
            window_tab: "ChangeTab00",
            dirty: true,
            shown: false,
            test: None,
            sounds: Vec::new(),
            move_cur: (0, 0),
            music_sent: None,
            mario: None,
            icons: Default::default(),
            attr: None,
            textures: view::TEXTURES
                .iter()
                .filter_map(|&n| {
                    ui.cache_mut()
                        .texture_file(view::GROUP, n)
                        .map(|t| (n, t.id))
                })
                .collect(),
            unsaved: false,
            trail: Default::default(),
            trail_on: cx.host.load_value(TRAIL_KEY).as_deref() != Some("0"),
            trail_label: None,
            water: Default::default(),
            scroll: Default::default(),
            help: None,
            pending_tip: None,
        };
        f.reset_palette();
        f.setup_page();
        f.glove = f.tile_center(f.cur);
        f.show(cx.host);
        f
    }

    pub fn style(&self) -> String {
        self.editor.course.header.game_style.as_str_lossy()
    }

    fn reset_palette(&mut self) {
        let style = self.style();
        self.palette = catalog::DEFAULT_PALETTE
            .iter()
            .map(|&t| match t {
                Tool::Part(16) if style == "3W" => Tool::Part(71),
                t => t,
            })
            .filter(|&t| catalog::entry(t).is_some_and(|e| catalog::available(&e, &style)))
            .collect();
        self.slot = self.slot.min(self.palette.len().saturating_sub(1));
    }

    /// The page's sub-layouts come in (their In animations) [obs data].
    fn setup_page(&mut self) {
        let Some(p) = &mut self.page else { return };
        for part in [
            "L_PaletteSet_00",
            "L_LeftParts_00",
            "L_RightParts_00",
            "L_Play_00",
            "L_Slider_00",
        ] {
            p.play(&format!("{part}/In"), 0);
        }
        p.play("L_PaletteSet_00/InBtn", 0);
        p.play("L_Slider_00/InSlideBarH", 0);
        // the ring, the bin, the rocket, the guides and the pull tab stay out of sight
        for pane in [
            "L_RingMenu_00",
            "L_TrashCan_00",
            "L_ResetRocket_00",
            "L_EditGuide_00",
            "L_ModeGuide_00",
            "L_PaletteGuide_00",
            "L_PullBtnSlider_00",
            "N_View_00",
            "L_LeftParts_00/N_Content_00",
            "L_LeftParts_00/L_MultiEditBtn_00",
        ] {
            p.show(pane, false);
        }
        // texts from the message table (the layouts carry placeholders)
        for (pane, label) in [
            (
                "L_LeftParts_00/L_SkinBtn_00/T_Btn_00",
                "L_LeftParts_00-L_SkinBtn_00-T_Btn_00",
            ),
            (
                "L_LeftParts_00/L_SceneBtn_00/T_Btn_00",
                "L_LeftParts_00-L_SceneBtn_00-T_Btn_00",
            ),
            (
                "L_LeftParts_00/L_ScrollBtn_00/T_Btn_00",
                "L_LeftParts_00-L_ScrollBtn_00-T_Btn_00",
            ),
            (
                "L_LeftParts_00/L_TimerBtn_00/T_Btn_00",
                "L_LeftParts_00-L_TimerBtn_00-T_Btn_00",
            ),
            (
                "L_LeftParts_00/L_MissionBtn_00/T_Btn_00",
                "L_LeftParts_00-L_MissionBtn_00-T_Btn_00",
            ),
            (
                "L_LeftParts_00/L_ZanzoBtn_00/T_Btn_00",
                "L_LeftParts_00-L_ZanzoBtn_00-T_Btn_00",
            ),
            (
                "L_LeftParts_00/L_WaterLevelBtn_00/T_Btn_00",
                "L_LeftParts_00-L_WaterLevelBtn_00-T_Btn_00",
            ),
            (
                "L_LeftParts_00/L_ScrollEditBtn_00/T_Btn_00",
                "L_LeftParts_00-L_ScrollEditBtn_00-T_Btn_00",
            ),
            (
                "L_RightParts_00/L_SaveBtn_00/T_Btn_00",
                "L_RightParts_00-L_SaveBtn_00-T_Btn_00",
            ),
            (
                "L_RightParts_00/L_ResetBtn_00/T_Btn_00",
                "L_RightParts_00-L_ResetBtn_00-T_Btn_00",
            ),
            (
                "L_RightParts_00/L_ResetBtn_00/T_Reset_00",
                "L_RightParts_00-L_ResetBtn_00-T_Reset_00",
            ),
            (
                "L_RightParts_00/L_UndoBtn_00/T_Balloon_00",
                "L_RightParts_00-L_UndoBtn_00-T_Balloon_00",
            ),
            (
                "L_RightParts_00/L_EraserBtn_00/T_Balloon_00",
                "L_RightParts_00-L_EraserBtn_00-T_Balloon_00",
            ),
            (
                "L_Play_00/L_PlayBtn_00/T_Text_00",
                "L_Play_00-L_PlayBtn_00-T_Text_00",
            ),
            (
                "L_Slider_00/L_BtnS_00/T_Goal_00",
                "L_Slider_00-L_BtnS_00-T_Goal_00",
            ),
            (
                "L_Slider_00/L_BtnG_00/T_Goal_00",
                "L_Slider_00-L_BtnG_00-T_Goal_00",
            ),
        ] {
            p.text_from(pane, label);
        }
        // "Play": the title's clapperboard text (the maker's has none of its own) [inf]
        if let Some(t) = self.ui.message("Play_Title_00", "L_PlayBtn_00-T_Btn_00") {
            p.text("L_Play_00/L_PlayBtn_00/T_Btn_00", &t);
        }
        // the clapperboard's "Play from start" text only while it is held [inf]
        p.show("L_Play_00/L_PlayBtn_00/T_Text_00", false);
        // the buttons' name labels come with Select / Decide only
        for b in ["L_WaterLevelBtn_00", "L_ScrollEditBtn_00"] {
            p.show(&format!("L_LeftParts_00/{b}/N_Text_00"), false);
        }
        self.update_page();
        self.update_timer();
        self.update_mission_button();
        self.update_trail_button();
    }

    // ------------------------------------------------------------ coordinates

    /// Layout units of world point (`x`, `y`) px.
    pub fn to_layout(&self, x: f32, y: f32) -> [f32; 2] {
        [
            (x - self.cam[0]) * SCALE - 640.0,
            (y - self.cam[1]) * SCALE - 360.0,
        ]
    }

    /// World px under layout point `p`.
    pub fn to_world(&self, p: [f32; 2]) -> [f32; 2] {
        [
            (p[0] + 640.0) / SCALE + self.cam[0],
            (p[1] + 360.0) / SCALE + self.cam[1],
        ]
    }

    /// The cursor's point in the world: the glove's when it is on the cursor's tile (the
    /// pointer), else the tile's centre (the d-pad).
    fn cursor_world(&self) -> [f32; 2] {
        if self.tile_at(self.glove) == self.cur {
            self.to_world(self.glove)
        } else {
            [
                (self.cur.0 as f32 + 0.5) * TILE,
                (self.cur.1 as f32 + 0.5) * TILE,
            ]
        }
    }

    fn tile_at(&self, p: [f32; 2]) -> (i32, i32) {
        let w = self.to_world(p);
        ((w[0] / TILE).floor() as i32, (w[1] / TILE).floor() as i32)
    }

    fn tile_center(&self, t: (i32, i32)) -> [f32; 2] {
        self.to_layout((t.0 as f32 + 0.5) * TILE, (t.1 as f32 + 0.5) * TILE)
    }

    /// Keep the view inside the area.
    fn clamp_cam(&mut self) {
        let (w, h) = self.editor.size();
        self.cam[0] = self.cam[0].clamp(0.0, (w as f32 * TILE - VIEW_W).max(0.0));
        self.cam[1] = self.cam[1].clamp(0.0, (h as f32 * TILE - VIEW_H).max(0.0));
    }

    /// Scroll so the cursor tile stays a tile or two inside the view (past the palette bar on
    /// top and the columns at the sides) [inf].
    fn follow_cursor(&mut self) {
        let (tx, ty) = self.cur;
        let (x0, x1) = (tx as f32 * TILE, (tx + 1) as f32 * TILE);
        let (y0, y1) = (ty as f32 * TILE, (ty + 1) as f32 * TILE);
        let (ml, mr, mb, mt) = (4.0 * TILE, 4.0 * TILE, 1.0 * TILE, 3.0 * TILE);
        if x0 < self.cam[0] + ml {
            self.cam[0] = x0 - ml;
        }
        if x1 > self.cam[0] + VIEW_W - mr {
            self.cam[0] = x1 - VIEW_W + mr;
        }
        if y0 < self.cam[1] + mb {
            self.cam[1] = y0 - mb;
        }
        if y1 > self.cam[1] + VIEW_H - mt {
            self.cam[1] = y1 - VIEW_H + mt;
        }
        self.clamp_cam();
    }

    /// A vertical area: the scroll bar is the vertical one (`N_SlideBarV_00`).
    fn vertical(&self) -> bool {
        self.editor.area().header.orientation != 0
    }

    /// Scroll to where pointer `p` is along the scroll bar.
    fn slide_to(&mut self, p: [f32; 2]) {
        let vertical = self.vertical();
        let Some(page) = &mut self.page else { return };
        page.apply(self.tick);
        if vertical {
            let Some(r) = page.rect("L_Slider_00/L_SlideBarV_00/B_ScrollArea_00") else {
                return;
            };
            let travel = (r.size[1] - HANDLE_V).max(1.0);
            let f = ((p[1] - (r.center[1] - travel / 2.0)) / travel).clamp(0.0, 1.0);
            let (_, h) = self.editor.size();
            self.cam[1] = f * (h as f32 * TILE - VIEW_H).max(0.0);
            self.clamp_cam();
            return;
        }
        let Some(r) = page.rect("L_Slider_00/L_SlideBarH_00/B_ScrollArea_00") else {
            return;
        };
        let travel = (r.size[0] - HANDLE).max(1.0);
        let f = ((p[0] - (r.center[0] - travel / 2.0)) / travel).clamp(0.0, 1.0);
        let (w, _) = self.editor.size();
        self.cam[0] = f * (w as f32 * TILE - VIEW_W).max(0.0);
        self.clamp_cam();
    }

    /// The scroll bar's handle where the view is.
    fn place_slider(&mut self) {
        let vertical = self.vertical();
        if let Some(p) = &mut self.page {
            p.show("L_Slider_00/N_SlideBarV_00", vertical);
            p.show("L_Slider_00/N_SlideBarH_00", !vertical);
        }
        if vertical {
            let (_, h) = self.editor.size();
            let span = (h as f32 * TILE - VIEW_H).max(1.0);
            let f = (self.cam[1] / span).clamp(0.0, 1.0);
            if let Some(p) = &mut self.page {
                // the handle inside the scroll area (from −234 up, 334.8 tall) [obs data; its
                // bottom anchor read from where the bar shows]
                let (lo, hi) = (-234.0 + HANDLE_V / 2.0, -234.0 + 334.8 - HANDLE_V / 2.0);
                p.set_translate(
                    "L_Slider_00/L_SlideBarV_00/N_TransRoot_00",
                    [0.0, lo + (hi - lo) * f],
                );
            }
            return;
        }
        let (w, _) = self.editor.size();
        let span = (w as f32 * TILE - VIEW_W).max(1.0);
        let f = (self.cam[0] / span).clamp(0.0, 1.0);
        if let Some(p) = &mut self.page {
            p.set_translate(
                "L_Slider_00/L_SlideBarH_00/N_TransRoot_00",
                // between the ends of the 700-unit bar (−350 … 350), the handle inside it
                [-350.0 + HANDLE / 2.0 + (700.0 - HANDLE) * f, 0.0],
            );
        }
    }

    /// The host shows the course as edited (after changes).
    fn show(&mut self, host: &mut dyn Host) {
        if self.dirty {
            self.dirty = false;
            self.shown = match self.editor.save() {
                Ok(bytes) => host.edit_show(&bytes, self.editor.area),
                Err(_) => false,
            };
        }
        host.edit_camera(self.cam[0], self.cam[1]);
    }

    // ------------------------------------------------------------ the tick

    pub fn step(&mut self, input: &MakerInput, cx: &mut crate::Cx) -> Option<MakerEvent> {
        self.tick += 1;
        let scripted = self.scripted(input);
        let input = &scripted;
        let pressed = input.ui.pressed;
        let keys_pressed = input.keys & !self.keys_held;
        self.keys_held = input.keys;
        // (a key that is both confirm and start, Enter, confirms)
        let start_pressed = input.start && !self.start_was && !input.ui.held.has(Buttons::CONFIRM);
        self.start_was = input.start;
        let play_pressed = input.play && !self.play_was;
        let play_released = !input.play && self.play_was;
        self.play_was = input.play;
        let click = input.ui.click;
        self.upload_answer(cx);
        let ev = match self.mode {
            Mode::Test => {
                self.test_tick(input, play_pressed, cx);
                None
            }
            Mode::Ring => {
                self.ring_tick(pressed, keys_pressed, click, cx);
                None
            }
            Mode::Attr => {
                self.attr_tick(input, pressed, cx);
                None
            }
            Mode::Menu => self.menu_tick(pressed, start_pressed, click, cx),
            Mode::Save => self.save_tick(pressed, click, cx),
            Mode::Window(w) => {
                self.window_tick(w, pressed, click, cx);
                None
            }
            Mode::SubArea => {
                let ev = self.subarea.as_mut().and_then(|w| w.step(pressed, click));
                if let Some(e) = ev {
                    if let subarea::SubAreaEvent::Choose(v) = e
                        && v != (self.editor.area().header.orientation != 0)
                    {
                        self.editor.checkpoint();
                        if self.editor.set_vertical(v).is_ok() {
                            self.changed();
                        } else {
                            self.editor.discard_checkpoint();
                        }
                    }
                    self.subarea = None;
                    self.mode = Mode::Edit;
                    self.dirty = true;
                }
                None
            }
            Mode::Water | Mode::Scroll => {
                self.mode_tick(input, pressed, keys_pressed, start_pressed, cx);
                None
            }
            Mode::Help => {
                let click = input.ui.click.or(if input.down && !self.down_was {
                    input.pointer
                } else {
                    None
                });
                if self.help.as_mut().is_none_or(|h| h.step(pressed, click)) {
                    self.help = None;
                    self.mode = Mode::Edit;
                    // a press that closed the tip doesn't go on into the course
                    self.down_was = true;
                    self.held = input.ui.held;
                    return None;
                }
                None
            }
            Mode::Mission => {
                let tabs = match keys_pressed {
                    k if k & keys::NEXT != 0 => 1,
                    k if k & keys::PREV != 0 => -1,
                    _ => 0,
                };
                let ev = self
                    .mission
                    .as_mut()
                    .and_then(|m| m.step(pressed, tabs, click));
                if let Some(e) = ev {
                    self.mission_done(e);
                }
                None
            }
            Mode::Edit => {
                if start_pressed {
                    self.open_menu();
                    None
                } else {
                    // the clapperboard key: a tap plays from Mario, a hold from the start
                    // (−1: wait for the key to come up first)
                    if input.play {
                        if self.play_hold >= 0 {
                            self.play_hold += 1;
                        }
                        if self.play_hold >= HOLD_START {
                            self.play_hold = -1;
                            self.start_test(cx, Some(None));
                        }
                    } else {
                        if self.play_hold > 0 {
                            self.start_test(cx, None);
                        }
                        self.play_hold = 0;
                    }
                    let _ = (play_pressed, play_released);
                    if self.mode == Mode::Edit {
                        self.edit_tick(input, pressed, keys_pressed, cx);
                    }
                    None
                }
            }
        };
        // a tip waiting for its window to close
        if self.mode == Mode::Edit
            && let Some(t) = self.pending_tip.take()
        {
            self.show_tip(cx.host, t);
        }
        if self.mode != Mode::Test {
            self.trail.tick(self.trail_on);
            if self.trail_label.is_some_and(|t| self.tick >= t)
                && let Some(p) = &mut self.page
            {
                self.trail_label = None;
                p.play("L_LeftParts_00/L_ZanzoBtn_00/Cancel", self.tick);
            }
            self.place_slider();
            self.show(cx.host);
        }
        self.play_sounds(cx.host);
        self.held = input.ui.held;
        self.down_was = input.down;
        self.alt_was = input.alt;
        ev
    }

    fn edit_tick(&mut self, input: &MakerInput, pressed: Buttons, keys: u16, cx: &mut crate::Cx) {
        // keys
        if keys & keys::UNDO != 0 {
            self.undo();
        }
        if keys & keys::REDO != 0 && self.editor.redo() {
            self.changed();
        }
        if keys & keys::PARTS != 0 {
            self.open_ring();
            return;
        }
        if keys & keys::NEXT != 0 && !self.palette.is_empty() {
            self.select_slot((self.slot + 1) % self.palette.len());
        }
        if keys & keys::PREV != 0 && !self.palette.is_empty() {
            self.select_slot((self.slot + self.palette.len() - 1) % self.palette.len());
        }
        if keys & keys::ERASER != 0 {
            self.toggle_eraser();
        }
        if keys & keys::AREA != 0 {
            self.switch_area();
        }
        if keys & keys::COPY != 0
            && let Some(r) = self.selection
        {
            // a copy of the selection with its bottom-left at the cursor
            self.editor.checkpoint();
            self.editor.copy(r);
            match self.editor.paste_clipboard(self.cur.0, self.cur.1) {
                Ok(()) => {
                    self.selection = Some(Rect::new(self.cur.0, self.cur.1, r.w, r.h));
                    self.changed();
                    self.sound(7, "set");
                }
                Err(e) => {
                    self.editor.discard_checkpoint();
                    self.refuse(cx, 7, &e);
                }
            }
        }
        if keys & keys::PLAY_HERE != 0 {
            let (tx, ty) = self.cur;
            let at = [(tx as f32 + 0.5) * TILE, ty as f32 * TILE];
            self.start_test(cx, Some(Some((self.editor.area, at))));
            return;
        }
        // a click / press on the page's buttons
        let pointer_pressed = input.down && !self.down_was;
        // a tap that ends a finger's press arrives after it (the app's touches): the press did
        // the work
        if input.pointer.is_some() {
            self.pointer_tick = self.tick;
        }
        let tap = input
            .ui
            .click
            .filter(|_| pointer_pressed || self.tick - self.pointer_tick > TAP_AFTER);
        let click = tap.or(if pointer_pressed { input.pointer } else { None });
        if let Some(p) = click
            && self.page_click(p, cx)
        {
            // the press is the button's: no course edit
            if self.drag != Some(Drag::Slider) {
                self.drag = None;
            }
            self.down_was = true;
            return;
        }
        if self.drag == Some(Drag::Slider) {
            match (input.down, input.pointer) {
                (true, Some(p)) => self.slide_to(p),
                _ => self.drag = None,
            }
            return;
        }
        // a tap without a pointer (touch): put down / erase / pick up once where it lands
        if input.pointer.is_none()
            && let Some(p) = tap
            && self.drag.is_none()
        {
            self.glove = p;
            self.cur = self.tile_at(p);
            if self.eraser {
                self.editor.checkpoint();
                self.drag = Some(Drag::Erase { changed: false });
                self.erase_here();
                self.end_drag();
            } else if self.editor.object_at(self.cur.0, self.cur.1).is_none() {
                self.press_place(cx);
                self.end_drag();
            }
            return;
        }
        // the cursor: the pointer when it moved, else the d-pad
        if let Some(p) = input.pointer
            && (self.pointer_was != Some(p) || input.down || input.alt)
        {
            self.glove = p;
            self.cur = self.tile_at(p);
            if input.down || input.alt {
                // dragging near an edge scrolls
                let (dx, dy) = edge_push(p);
                if dx != 0.0 || dy != 0.0 {
                    self.cam[0] += dx;
                    self.cam[1] += dy;
                    self.clamp_cam();
                    self.cur = self.tile_at(p);
                }
            }
        }
        self.pointer_was = input.pointer;
        if let Some(d) = input.ui.held.direction() {
            let first = input.ui.pressed.direction().is_some();
            if first {
                self.repeat = 0;
            }
            self.repeat += 1;
            if first || (self.repeat > DELAY && (self.repeat - DELAY) % REPEAT == 0) {
                let (w, h) = self.editor.size();
                self.cur.0 = (self.cur.0 + d[0] as i32).clamp(0, w - 1);
                self.cur.1 = (self.cur.1 + d[1] as i32).clamp(0, h - 1);
                self.follow_cursor();
                self.glove = self.tile_center(self.cur);
            }
        }
        // carrying something: each tile the cursor moves to sounds `move` in the carried thing's
        // edit user [inf: the key every part's user has; when the game plays it not read]
        // (a multigrab rectangle being drawn ticks `cursor` instead [inf])
        if self.cur != self.move_cur {
            let part = match self.drag {
                Some(Drag::Carry { index, .. }) => {
                    self.editor.object(index).map(|o| (o.id, "move"))
                }
                Some(Drag::MoveSelection { .. }) => Some((7, "move")),
                Some(Drag::Select { .. }) => Some((7, "cursor")),
                Some(Drag::Start { .. }) => Some((START_GROUND, "move")),
                Some(Drag::Goal { .. }) => Some((GOAL_GROUND, "move")),
                Some(Drag::Resize { index, .. }) => {
                    self.editor.object(index).map(|o| (o.id, "move"))
                }
                Some(Drag::CarryPipe { .. }) => Some((CLEAR_PIPE, "move")),
                _ => None,
            };
            if let Some((p, key)) = part {
                self.sound(p, key);
            }
            self.move_cur = self.cur;
        }
        // course edits: confirm / primary press puts down (or picks up), back / secondary
        // erases
        let erase_held = (input.ui.held.has(Buttons::BACK) && !input.start) || input.alt;
        let place_held = input.ui.held.has(Buttons::CONFIRM) || input.down;
        let place_pressed = pressed.has(Buttons::CONFIRM) || pointer_pressed;
        let erase_pressed =
            (pressed.has(Buttons::BACK) && !input.start) || (input.alt && !self.alt_was);
        let multi = input.keys & keys::MULTI != 0;
        match self.drag {
            None if place_pressed && multi => {
                self.selection = None;
                self.drag = Some(Drag::Select { from: self.cur });
                self.sound(7, "frame");
            }
            None if self
                .selection
                .is_some_and(|r| r.contains(self.cur.0, self.cur.1))
                && (place_pressed || erase_pressed) =>
            {
                let r = self.selection.expect("selection");
                if erase_pressed || self.eraser {
                    self.editor.checkpoint();
                    if self.editor.delete_rect(r) > 0 {
                        self.changed();
                        self.sound(7, "eraser");
                    } else {
                        self.editor.discard_checkpoint();
                    }
                    self.selection = None;
                } else {
                    let held = if self.editor.object_at(self.cur.0, self.cur.1).is_some() {
                        0
                    } else {
                        -1
                    };
                    self.drag = Some(Drag::MoveSelection {
                        grab: self.cur,
                        held,
                    });
                    self.sound(7, "drag");
                }
            }
            None => {
                if place_pressed || erase_pressed {
                    self.selection = None;
                }
                if erase_pressed || (place_pressed && self.eraser) {
                    self.editor.checkpoint();
                    self.drag = Some(Drag::Erase { changed: false });
                    self.erase_here();
                } else if place_pressed {
                    self.press_place(cx);
                }
            }
            Some(Drag::Erase { .. }) => {
                if erase_held || (place_held && self.eraser) {
                    self.erase_here();
                } else {
                    self.end_drag();
                }
            }
            Some(Drag::Place { last, .. }) => {
                if place_held {
                    if self.cur != last && self.tool().paints() {
                        self.put(cx, false);
                    }
                } else {
                    self.end_drag();
                }
            }
            Some(Drag::Carry { index, from, held }) => {
                if !place_held {
                    self.drop_carried(cx, index, from, held);
                } else if held >= 0 && self.cur == from {
                    // held still on the part: its attribute balloon opens
                    let held = held + 1;
                    self.drag = Some(Drag::Carry { index, from, held });
                    if held >= attr::HOLD && !self.open_attr(index) {
                        self.drag = Some(Drag::Carry {
                            index,
                            from,
                            held: -1,
                        });
                    }
                } else if held >= 0 {
                    self.drag = Some(Drag::Carry {
                        index,
                        from,
                        held: -1,
                    });
                }
            }
            Some(Drag::Slider) => {}
            Some(Drag::Start { .. }) | Some(Drag::Goal { .. }) => {
                if place_held {
                    self.drag_start_goal();
                } else {
                    self.end_drag();
                }
            }
            Some(Drag::Resize { .. }) => {
                if place_held {
                    self.drag_resize();
                } else {
                    self.end_drag();
                }
            }
            Some(Drag::ClearPipe { .. }) => {
                if place_held {
                    self.draw_clear_pipe(cx);
                } else {
                    self.end_drag();
                }
            }
            Some(Drag::CarryPipe { slot, from }) => {
                if !place_held {
                    self.drop_clear_pipe(cx, slot, from);
                }
            }
            Some(Drag::Select { from }) => {
                if !place_held {
                    self.drag = None;
                    let r = Rect::spanning(from, self.cur);
                    // (a press without a drag selects nothing)
                    self.selection =
                        (r.w > 1 || r.h > 1 || self.editor.object_at(from.0, from.1).is_some())
                            .then_some(r);
                }
            }
            Some(Drag::MoveSelection { grab, held }) => {
                if place_held && held >= 0 {
                    // held still on a part of the selection: the balloon for all of it
                    let still = self.cur == grab;
                    let held = if still { held + 1 } else { -1 };
                    self.drag = Some(Drag::MoveSelection { grab, held });
                    if held >= attr::HOLD {
                        // the balloon's undo step (a selection's press made none)
                        self.editor.checkpoint();
                        let opened = self
                            .editor
                            .object_at(grab.0, grab.1)
                            .zip(self.selection)
                            .is_some_and(|(i, r)| {
                                let mut parts = vec![i];
                                parts.extend(
                                    self.editor.objects_in(r).into_iter().filter(|&j| j != i),
                                );
                                self.open_attr_parts(parts)
                            });
                        if !opened {
                            self.editor.discard_checkpoint();
                            self.drag = Some(Drag::MoveSelection { grab, held: -1 });
                        }
                    }
                } else if !place_held {
                    self.drag = None;
                    let (dx, dy) = (self.cur.0 - grab.0, self.cur.1 - grab.1);
                    if let Some(r) = self.selection
                        && (dx, dy) != (0, 0)
                    {
                        self.editor.checkpoint();
                        match self.editor.move_rect(r, dx, dy) {
                            Ok(()) => {
                                self.selection = Some(Rect::new(r.x + dx, r.y + dy, r.w, r.h));
                                self.changed();
                                self.sound(7, "set");
                            }
                            Err(e) => {
                                self.editor.discard_checkpoint();
                                self.refuse(cx, 7, &e);
                            }
                        }
                    }
                }
            }
        }
    }

    /// Whether a press at tile `t` on part `i` stretches it: (width, height) when `t` is on
    /// its right column (resizable width) / top row (resizable height) [inf: the game shows
    /// drag handles on those edges]. Pipes stretch up only when they face up.
    fn resize_edge(&self, i: usize, t: (i32, i32)) -> Option<(bool, bool)> {
        let o = self.editor.object(i)?;
        let (wr, hr) = mmk_editor::parts::resize_range(o.id)?;
        if o.id == 9 {
            // a pipe by its mouth (the two tiles at its open end): stretched and turned
            let r = mmk_editor::parts::footprint(o);
            let mouth = match Dir::from_flags(o.flags) {
                Dir::Up => t.1 == r.y + r.h - 1,
                Dir::Down => t.1 == r.y,
                Dir::Right => t.0 == r.x + r.w - 1,
                Dir::Left => t.0 == r.x,
            };
            return mouth.then_some((false, true));
        }
        let r = mmk_editor::parts::footprint(o);
        let width = wr.start() != wr.end() && t.0 == r.x + r.w - 1;
        let height = hr.start() != hr.end() && t.1 == r.y + r.h - 1;
        (width || height).then_some((width, height))
    }

    /// The stretched part follows the cursor (within the sizes the game allows).
    fn drag_resize(&mut self) {
        let Some(Drag::Resize {
            index,
            width,
            height,
            ..
        }) = self.drag
        else {
            return;
        };
        let Some(o) = self.editor.object(index).cloned() else {
            return;
        };
        let Some((wr, hr)) = mmk_editor::parts::resize_range(o.id) else {
            return;
        };
        let r = mmk_editor::parts::footprint(&o);
        let (tx, ty) = self.cur;
        if o.id == 9 {
            // the mouth follows the cursor round the pipe's base: the longer axis from the base
            // turns it, the distance is its length [inf: the game's gesture]
            let dir = Dir::from_flags(o.flags);
            let (ax, ay) = mmk_editor::parts::pipe_anchor(r.x, r.y, o.height.max(1) as i32, dir);
            let (dx, dy) = (tx - ax, ty - ay);
            let (to, len) = if dy.abs() >= dx.abs() {
                (if dy >= 0 { Dir::Up } else { Dir::Down }, dy.abs() + 1)
            } else {
                (if dx > 0 { Dir::Right } else { Dir::Left }, dx.abs() + 1)
            };
            let len = len.clamp(*hr.start() as i32, *hr.end() as i32) as u8;
            if (to, len) != (dir, o.height) && self.editor.turn_pipe(index, to, len).is_ok() {
                if let Some(Drag::Resize { changed, .. }) = &mut self.drag {
                    *changed = true;
                }
                self.changed();
            }
            return;
        }
        let w = if width {
            (tx - r.x + 1).clamp(*wr.start() as i32, *wr.end() as i32) as u8
        } else {
            o.width
        };
        let h = if height {
            (ty - r.y + 1).clamp(*hr.start() as i32, *hr.end() as i32) as u8
        } else {
            o.height
        };
        if (w, h) == (o.width, o.height) {
            return;
        }
        if self.editor.resize(index, w, h).is_ok() {
            if let Some(Drag::Resize { changed, .. }) = &mut self.drag {
                *changed = true;
            }
            self.changed();
        }
    }

    /// The start / goal follow the cursor: the start's height, the goal's height and the
    /// course's length (the goal at the right edge).
    fn drag_start_goal(&mut self) {
        let (tx, ty) = self.cur;
        let h = &self.editor.course.header;
        let (start_y, goal_y) = (h.start_y, h.goal_y);
        let mut did = false;
        match self.drag {
            Some(Drag::Start { .. }) => {
                let y = ty.clamp(0, 25) as u8;
                if y != start_y {
                    self.editor.set_start_goal(y, goal_y);
                    did = self.editor.course.header.start_y != start_y;
                }
            }
            Some(Drag::Goal { grab, .. }) => {
                let y = ty.clamp(0, 15) as u8;
                if y != goal_y {
                    self.editor.set_start_goal(start_y, y);
                    did = self.editor.course.header.goal_y != goal_y;
                }
                let dx = tx - grab;
                if dx != 0 {
                    let (w, _) = self.editor.size();
                    // shortening takes the ground past the new edge with it (parts there stop
                    // the goal) [inf]
                    let to = (w + dx).clamp(mmk_editor::settings::WIDTH_MIN, w);
                    let beyond: Vec<(i32, i32)> = self
                        .editor
                        .area()
                        .ground
                        .iter()
                        .map(|g| (g.x as i32, g.y as i32))
                        .filter(|&(x, _)| x >= to)
                        .collect();
                    let parts_beyond = self
                        .editor
                        .objects_in(Rect::new(to, 0, (w - to).max(0), 256))
                        .is_empty();
                    if dx < 0 && parts_beyond {
                        for (x, y) in beyond {
                            self.editor.erase_ground(x, y);
                        }
                    }
                    if self.editor.set_area_width(w + dx).is_ok() && self.editor.size().0 != w {
                        did = true;
                        if let Some(Drag::Goal { grab, .. }) = &mut self.drag {
                            *grab = tx;
                        }
                    }
                }
            }
            _ => {}
        }
        if did {
            if let Some(Drag::Start { changed } | Drag::Goal { changed, .. }) = &mut self.drag {
                *changed = true;
            }
            self.changed();
        }
    }

    /// The tool of the selected palette slot.
    pub fn tool(&self) -> Tool {
        self.palette.get(self.slot).copied().unwrap_or(Tool::Ground)
    }

    fn changed(&mut self) {
        self.dirty = true;
        self.unsaved = true;
        self.update_page();
        self.update_timer();
    }

    /// Part `part` makes edit sound `key` this tick.
    fn sound(&mut self, part: i16, key: &'static str) {
        self.sounds.push((part, key, None));
    }

    /// The sound of a part `put_into` changed: its part, key and pitch.
    fn put_sound(&mut self, (part, key, scale): PutSound) {
        self.sounds.push((part, key, scale));
    }

    /// The tick's edit sounds, and the music the maker wants: its track of the style and the
    /// area's theme while editing (a theme or style switch changes it), none in test play
    /// (research/specs/maker.md C9).
    fn play_sounds(&mut self, host: &mut dyn Host) {
        let style = self.style();
        for (part, key, scale) in std::mem::take(&mut self.sounds) {
            let user = format!("{style}_Edit{}", catalog::edit_user(part));
            host.edit_sound(&style, &user, key, scale);
        }
        let want = (self.mode != Mode::Test).then(|| (style, self.editor.theme().0));
        if self.music_sent.as_ref() != Some(&want) {
            host.edit_music(want.as_ref().map(|(s, t)| (s.as_str(), *t)));
            self.music_sent = Some(want);
        }
    }

    /// The maker is left (another screen comes): its music stops; it starts again when the
    /// maker comes back.
    pub fn leave(&mut self, host: &mut dyn Host) {
        if self.music_sent.as_ref().is_some_and(|m| m.is_some()) {
            host.edit_music(None);
        }
        self.music_sent = None;
    }

    /// A refused edit: the toast for a full course or a part at its limit, and the refusal's
    /// sound.
    fn refuse(&mut self, cx: &mut crate::Cx, part: i16, e: &EditError) {
        refuse(cx, &self.ui, e);
        let key = if matches!(e, EditError::Limit(_) | EditError::Full) {
            "partsMax"
        } else {
            "invalid"
        };
        self.sound(part, key);
    }

    fn undo(&mut self) {
        if self.editor.undo() {
            self.changed();
            if let Some(p) = &mut self.page {
                p.play("L_RightParts_00/L_UndoBtn_00/Decide", self.tick);
            }
            self.ui
                .sound_button(PAGE, "L_RightParts_00/L_UndoBtn_00", Event::Decide);
        }
    }

    fn press_place(&mut self, cx: &mut crate::Cx) {
        let (tx, ty) = self.cur;
        // the start and the goal are dragged, not built on (main area) [inf: as the game's
        // start sign and goal pole are]
        if self.editor.area == 0 && self.editor.object_at(tx, ty).is_none() {
            let h = &self.editor.course.header;
            let (start, goal) = self.editor.zones();
            if start.contains(tx, ty) && ty <= h.start_y as i32 + 2 {
                self.editor.checkpoint();
                self.drag = Some(Drag::Start { changed: false });
                self.sound(START_GROUND, "drag");
                return;
            }
            if goal.contains(tx, ty) && ty <= h.goal_y as i32 + 10 {
                self.editor.checkpoint();
                self.drag = Some(Drag::Goal {
                    grab: tx,
                    changed: false,
                });
                self.sound(GOAL_GROUND, "drag");
                return;
            }
        }
        // a part under the cursor takes the selected part in (a block's item, a pipe's
        // contents, a Super Mushroom makes it big), is stretched by its edge, else it is picked
        // up (ground isn't a part)
        if let Some(i) = self.editor.object_at(tx, ty) {
            self.editor.checkpoint();
            if let Some((width, height)) = self.resize_edge(i, (tx, ty)) {
                self.drag = Some(Drag::Resize {
                    index: i,
                    width,
                    height,
                    changed: false,
                });
                if let Some(id) = self.editor.object(i).map(|o| o.id) {
                    self.sound(id, "drag");
                }
                return;
            }
            if let Tool::Part(p) = self.tool()
                && let Some(snd) = self.put_into(i, p, DEFAULT_FLAGS, false)
            {
                self.changed();
                self.put_sound(snd);
                self.drag = Some(Drag::Place {
                    last: self.cur,
                    changed: true,
                });
                return;
            }
            self.drag = Some(Drag::Carry {
                index: i,
                from: (tx, ty),
                held: 0,
            });
            let id = self.editor.object(i).map_or(7, |o| o.id);
            self.sound(id, "drag");
            return;
        }
        // a clear pipe: by its mouth it is drawn on from that end, elsewhere it is carried
        // (0x7100ec1e90 tests the press against the mouths) [obs]
        if let Some((slot, end)) = self.editor.clear_pipe_end_at(tx, ty) {
            self.editor.checkpoint();
            self.drag = Some(Drag::ClearPipe {
                corner: (tx, ty),
                slot: Some(slot),
                end,
                fresh: false,
                changed: false,
            });
            self.sound(CLEAR_PIPE, "drag");
            return;
        }
        if let Some(slot) = self.editor.clear_pipe_at(tx, ty) {
            self.editor.checkpoint();
            self.drag = Some(Drag::CarryPipe {
                slot,
                from: (tx, ty),
            });
            self.sound(CLEAR_PIPE, "drag");
            return;
        }
        self.editor.checkpoint();
        if self.tool() == Tool::ClearPipe {
            // the drawing starts at the grid corner nearest the press (the game's guide point)
            let w = self.cursor_world();
            let corner = ((w[0] / TILE).round() as i32, (w[1] / TILE).round() as i32);
            self.drag = Some(Drag::ClearPipe {
                corner,
                slot: None,
                end: PipeEnd::End,
                fresh: false,
                changed: false,
            });
            return;
        }
        self.drag = Some(Drag::Place {
            last: self.cur,
            changed: false,
        });
        self.put(cx, true);
    }

    /// The clear pipe being drawn follows the cursor (`mmk_editor::clearpipe`): started once
    /// the cursor is far enough from its corner, taken back while it is fresh and the cursor
    /// comes back, else its end goes on, back or round a bend toward the cursor's tile.
    fn draw_clear_pipe(&mut self, cx: &mut crate::Cx) {
        let Some(Drag::ClearPipe {
            corner,
            slot,
            end,
            fresh,
            changed,
        }) = self.drag
        else {
            return;
        };
        let w = self.cursor_world();
        let (dx, dy) = (w[0] - corner.0 as f32 * TILE, w[1] - corner.1 as f32 * TILE);
        let dist = dx.hypot(dy);
        let (mut slot, mut fresh, mut changed) = (slot, fresh, changed);
        match slot {
            None if dist >= CLEAR_PIPE_START => {
                let dir = mmk_editor::clearpipe::drag_dir(dx, dy);
                match self.editor.place_clear_pipe(corner, dir) {
                    Ok(k) => {
                        (slot, fresh, changed) = (Some(k), true, true);
                        self.changed();
                        self.sound(CLEAR_PIPE, "set");
                    }
                    Err(e) => {
                        // refused: the press puts nothing down
                        self.refuse(cx, CLEAR_PIPE, &e);
                        self.drag = Some(Drag::Place {
                            last: self.cur,
                            changed: false,
                        });
                        return;
                    }
                }
            }
            None => {}
            Some(k) if fresh && dist <= CLEAR_PIPE_BACK => {
                self.editor.erase_clear_pipe(k);
                self.changed();
                (slot, fresh, changed) = (None, false, false);
            }
            Some(k)
                if fresh
                    && dist >= CLEAR_PIPE_START
                    && self.editor.clear_pipe(k).map(|s| s.runs[0].0)
                        != Some(mmk_editor::clearpipe::drag_dir(dx, dy)) =>
            {
                // the new pipe turns round its corner to the cursor's quarter [obs 0x7100fa9090]
                let old = self.editor.clear_pipe(k);
                self.editor.erase_clear_pipe(k);
                let dir = mmk_editor::clearpipe::drag_dir(dx, dy);
                slot = match self.editor.place_clear_pipe(corner, dir) {
                    Ok(n) => {
                        self.changed();
                        Some(n)
                    }
                    Err(_) => {
                        // it can't turn there: it stays as it was
                        let back = old.map(|s| s.runs[0].0).unwrap_or(dir);
                        self.editor.place_clear_pipe(corner, back).ok()
                    }
                };
            }
            Some(k) => {
                if let Ok(true) = self.editor.draw_clear_pipe(k, end, self.cur) {
                    (fresh, changed) = (false, true);
                    self.changed();
                    self.sound(CLEAR_PIPE, "move");
                }
            }
        }
        self.drag = Some(Drag::ClearPipe {
            corner,
            slot,
            end,
            fresh,
            changed,
        });
    }

    /// Drop a carried clear pipe: moved by the cursor's travel, or left where it was.
    fn drop_clear_pipe(&mut self, cx: &mut crate::Cx, slot: usize, from: (i32, i32)) {
        self.drag = None;
        let (dx, dy) = (self.cur.0 - from.0, self.cur.1 - from.1);
        if (dx, dy) == (0, 0) {
            self.editor.discard_checkpoint();
            self.sound(CLEAR_PIPE, "set");
            return;
        }
        match self.editor.move_clear_pipe(slot, dx, dy) {
            Ok(()) => {
                self.changed();
                self.sound(CLEAR_PIPE, "set");
            }
            Err(e) => {
                self.editor.discard_checkpoint();
                self.refuse(cx, CLEAR_PIPE, &e);
            }
        }
    }

    /// Put the selected part down at the cursor.
    fn put(&mut self, cx: &mut crate::Cx, first: bool) {
        let (tx, ty) = self.cur;
        let tool = self.tool();
        if tool == Tool::Track {
            return self.put_track(cx, first);
        }
        let r = match tool {
            Tool::Ground => self.editor.paint_ground(tx, ty).map(|_| ()),
            Tool::Part(p) => self.editor.place(p, tx, ty).map(|_| ()),
            Tool::Pipe => self.editor.place_pipe(tx, ty, 2, Dir::Up, 0).map(|_| ()),
            Tool::Doors => self
                .editor
                .place_doors((tx, ty), (tx + 4, ty), mmk_editor::links::DoorKind::Normal)
                .map(|_| ()),
            Tool::WarpBoxes => self
                .editor
                .place_warp_boxes((tx, ty), (tx + 4, ty), false)
                .map(|_| ()),
            Tool::Track | Tool::ClearPipe => Ok(()),
        };
        // a part put down on a track rides it (flag 0x400, the piece's link)
        if let (Ok(()), Tool::Part(p)) = (&r, tool)
            && mmk_editor::parts::caps(p).track
            && self.editor.track_at(tx, ty).is_some()
            && let Some(i) = self.editor.object_at(tx, ty)
        {
            let _ = self.editor.attach_to_track(i);
        }
        let ok = r.is_ok();
        if let Some(Drag::Place { last, changed }) = &mut self.drag {
            *last = (tx, ty);
            *changed |= ok;
        }
        match r {
            Ok(()) => {
                self.changed();
                self.sound(tool.part(), "set");
                // the part's voice as it lands [inf: the database's `voice` of each part]
                if first {
                    self.sound(tool.part(), "voice");
                }
            }
            Err(e) if first => self.refuse(cx, tool.part(), &e),
            Err(_) => {}
        }
    }

    /// Tracks: a press lays a horizontal piece at the cursor; dragging draws on from the last
    /// end, 2 tiles a piece, in the cursor's direction (8 ways).
    fn put_track(&mut self, cx: &mut crate::Cx, first: bool) {
        let (tx, ty) = self.cur;
        // a press on a track's end opens or closes it [inf: as tapping an end does in the game]
        if first && let Some(open) = self.editor.track_end_open((tx, ty)) {
            if self.editor.set_track_end((tx, ty), !open).is_ok() {
                if let Some(Drag::Place { changed, .. }) = &mut self.drag {
                    *changed = true;
                }
                self.changed();
                self.sound(59, "p_AttributeChange");
            }
            return;
        }
        let (r, end) = if first {
            (self.editor.place_track(tx, ty, 0).map(|_| ()), (tx + 1, ty))
        } else {
            let Some(Drag::Place { last, .. }) = self.drag else {
                return;
            };
            let (dx, dy) = (tx - last.0, ty - last.1);
            let m = dx.abs().max(dy.abs());
            if m < 2 {
                return;
            }
            let step = |d: i32| if d.abs() * 2 >= m { d.signum() } else { 0 };
            let end = (last.0 + 2 * step(dx), last.1 + 2 * step(dy));
            (self.editor.draw_track(last, end).map(|_| ()), end)
        };
        match r {
            Ok(()) => {
                if let Some(Drag::Place { last, changed }) = &mut self.drag {
                    *last = end;
                    *changed = true;
                }
                self.changed();
                self.sound(59, "set");
            }
            Err(e) if first => self.refuse(cx, 59, &e),
            Err(_) => {}
        }
    }

    fn erase_here(&mut self) {
        let (tx, ty) = self.cur;
        let part = match self.editor.object_at(tx, ty) {
            Some(i) => self.editor.object(i).map_or(7, |o| o.id),
            None if self.editor.clear_pipe_at(tx, ty).is_some() => CLEAR_PIPE,
            None => 7,
        };
        if self.editor.erase(tx, ty) {
            self.sound(part, "eraser");
            if let Some(Drag::Erase { changed }) = &mut self.drag {
                *changed = true;
            }
            self.changed();
        }
    }

    /// The press ended: keep its checkpoint only when something changed.
    fn end_drag(&mut self) {
        // the start / goal put down where they were dragged
        match self.drag {
            Some(Drag::Start { changed: true }) => self.sound(START_GROUND, "set"),
            Some(Drag::Goal { changed: true, .. }) => self.sound(GOAL_GROUND, "set"),
            // a stretched part, as one put down [inf]
            Some(Drag::Resize {
                index,
                changed: true,
                ..
            }) => {
                if let Some(id) = self.editor.object(index).map(|o| o.id) {
                    self.sound(id, "set");
                }
            }
            _ => {}
        }
        let changed = match self.drag.take() {
            Some(Drag::Place { changed, .. })
            | Some(Drag::Erase { changed })
            | Some(Drag::Start { changed })
            | Some(Drag::Goal { changed, .. })
            | Some(Drag::Resize { changed, .. })
            | Some(Drag::ClearPipe { changed, .. }) => changed,
            _ => false,
        };
        if !changed {
            self.editor.discard_checkpoint();
        }
        self.update_page();
    }

    /// Drop a carried part at the cursor: the same part, moved by the cursor's travel; where it
    /// can't go, it stays.
    fn drop_carried(&mut self, cx: &mut crate::Cx, index: usize, from: (i32, i32), held: i32) {
        self.drag = None;
        let (dx, dy) = (self.cur.0 - from.0, self.cur.1 - from.1);
        let Some(o) = self.editor.object(index).cloned() else {
            self.editor.discard_checkpoint();
            return;
        };
        // dropped on another part that takes it in: it goes inside
        if let Some(t) = self
            .editor
            .object_at(self.cur.0, self.cur.1)
            .filter(|&t| t != index)
            // it goes in as it is: its wings, parachute, form and size (the contents
            // balloon draws a child's wings and parachute, maker spec C10)
            && let Some(snd) = self.put_into(t, o.id, DEFAULT_FLAGS | (o.flags & CHILD_KEEPS), true)
        {
            self.editor.erase_object(index);
            self.changed();
            self.put_sound(snd);
            return;
        }
        if dx == 0 && dy == 0 && held < attr::HOLD {
            // a press without a move: the part's next form, as a tap on a part does [inf]
            if let Ok(true) = self.editor.next_form(index) {
                self.changed();
                self.sound(o.id, "p_AttributeChange");
            } else {
                self.editor.discard_checkpoint();
                self.sound(o.id, "set");
            }
            return;
        }
        let r = mmk_editor::parts::footprint(&o);
        match self.editor.move_rect(r, dx, dy) {
            Ok(()) => {
                self.changed();
                self.sound(o.id, "set");
            }
            Err(e) => {
                self.editor.discard_checkpoint();
                self.refuse(cx, o.id, &e);
            }
        }
    }

    /// Part `part` goes into (or onto) the placed part `i`: a container's contents (blocks,
    /// Bill Blasters, Lakitu, trees, checkpoints, a pipe's), or a Super Mushroom makes it big
    /// [inf: as dropping an item on a part does in the game]; with `stack` (a carried part
    /// dropped on it) parts that stack make a column. The sound if something changed: the
    /// container's `Block_in`, `Dokan_in` …, `scaleChange`, or the stacked part's `Tower_stack`
    /// pitched by its tile row.
    fn put_into(&mut self, i: usize, part: i16, child_flags: u32, stack: bool) -> Option<PutSound> {
        let o = self.editor.object(i).cloned()?;
        let key = catalog::put_in_key(o.id, part);
        let into = |k: &'static str| (o.id, k, None);
        if o.id == 9 {
            return self
                .editor
                .put_in_pipe(i, part)
                .is_ok()
                .then_some(into(key));
        }
        if o.child_id != part && mmk_editor::rules::check_child(o.id, part).is_ok() {
            return self
                .editor
                .try_set_child(i, part, child_flags)
                .is_ok()
                .then_some(into(key));
        }
        if part == 20 && mmk_editor::parts::caps(o.id).big && !o.is_big() {
            return self
                .editor
                .toggle_big(i)
                .is_ok()
                .then_some(into("scaleChange"));
        }
        // parts that stack: it goes on top of the column, sounding `Tower_stack` with
        // `MusicalScale` = its tile row mod 28 (0x7100eb0080) [obs]; `Combine` belongs to the
        // parts' 0xC0000 bits, not to stacking (objects/stacks.md §6)
        if stack && mmk_editor::parts::caps(o.id).stack && mmk_editor::parts::caps(part).stack {
            let new = self.editor.stack_onto(i, part, child_flags).ok()?;
            let row = self
                .editor
                .object(new)
                .map_or(0, |n| mmk_editor::parts::footprint(n).y);
            return Some((part, "Tower_stack", Some(tower_scale(row))));
        }
        None
    }

    /// The rectangle the multigrab shows: the one being drawn, else the selection (moved with
    /// the cursor while carried).
    pub fn selection_shown(&self) -> Option<Rect> {
        match self.drag {
            Some(Drag::Select { from }) => Some(Rect::spanning(from, self.cur)),
            Some(Drag::MoveSelection { grab, .. }) => self.selection.map(|r| {
                Rect::new(
                    r.x + self.cur.0 - grab.0,
                    r.y + self.cur.1 - grab.1,
                    r.w,
                    r.h,
                )
            }),
            _ => self.selection,
        }
    }

    /// What is being carried (its part id), for the glove's picture.
    pub fn carried_part(&self) -> Option<i16> {
        match self.drag {
            Some(Drag::Carry { index, .. }) => self.editor.object(index).map(|o| o.id),
            _ => None,
        }
    }

    /// The part picture's UI texture (the palette's), if loaded.
    pub fn part_texture(&self, part: i16) -> Option<u32> {
        let name = self.icon_key(&self.style(), part)?;
        self.ui
            .cache_mut()
            .texture_file("PartIcon", &name)
            .map(|t| t.id)
    }

    fn toggle_eraser(&mut self) {
        self.eraser = !self.eraser;
        if let Some(p) = &mut self.page {
            p.hold(
                "L_RightParts_00/L_EraserBtn_00/Toggle",
                if self.eraser { 1.0 } else { 0.0 },
            );
        }
        self.ui
            .sound_button(PAGE, "L_RightParts_00/L_EraserBtn_00", Event::Decide);
    }

    fn switch_area(&mut self) {
        let to = 1 - self.editor.area;
        self.editor.set_area(to);
        // an area change empties the trail (`cState_AreaChange`) [obs 0x71012d2f10]
        self.trail.clear();
        self.cam = [0.0, 0.0];
        self.cur = (8, 2);
        self.glove = self.tile_center(self.cur);
        self.dirty = true;
        // an empty sub-area: its orientation first (Edit_SubAreaWindow_00)
        let a = self.editor.area();
        if to == 1 && a.objects.items.is_empty() && a.ground.is_empty() && a.tracks.is_empty() {
            let vertical = a.header.orientation != 0;
            self.subarea = subarea::SubAreaWindow::new(&self.ui, vertical);
            if self.subarea.is_some() {
                self.mode = Mode::SubArea;
            }
        }
    }

    fn select_slot(&mut self, i: usize) {
        if i == self.slot {
            return;
        }
        self.slot = i;
        self.eraser = false;
        self.ui.sound_button(
            PAGE,
            &format!("L_PaletteSet_00/{}", slot_part(i)),
            Event::Select,
        );
        if let Some(p) = &mut self.page {
            p.hold("L_RightParts_00/L_EraserBtn_00/Toggle", 0.0);
        }
        self.update_page();
    }

    pub fn part_name(&self, e: &Entry) -> String {
        let style = self.style();
        catalog::labels(e, &style)
            .iter()
            .find_map(|l| self.ui.message("PartsIconName", l))
            .unwrap_or_else(|| e.label.to_string())
    }

    /// A click on the page's buttons (true: it hit one).
    fn page_click(&mut self, p: [f32; 2], cx: &mut crate::Cx) -> bool {
        let (water_ok, scroll_ok) = (self.water_available(), self.scroll_available());
        let Some(page) = &mut self.page else {
            return false;
        };
        let slots: Vec<String> = (0..self.palette.len())
            .map(|i| format!("L_PaletteSet_00/{}/B_Btn_00", slot_part(i)))
            .collect();
        if let Some(i) = hit(page, self.tick, &slots, p) {
            self.select_slot(i);
            return true;
        }
        let buttons: [(&str, u8); 14] = [
            ("L_PaletteSet_00/L_SearchBtn_00/B_Btn_00", 0),
            ("L_Play_00/L_PlayBtn_00/B_Btn_00", 1),
            ("L_RightParts_00/L_UndoBtn_00/B_Btn_00", 2),
            ("L_RightParts_00/L_EraserBtn_00/B_Btn_00", 3),
            ("L_RightParts_00/L_MenuBtn_00/B_Btn_00", 4),
            ("L_RightParts_00/L_SaveBtn_00/B_Btn_00", 5),
            ("L_LeftParts_00/L_SkinBtn_00/B_Btn_00", 6),
            ("L_LeftParts_00/L_SceneBtn_00/B_Btn_00", 7),
            ("L_LeftParts_00/L_TimerBtn_00/B_Btn_00", 8),
            ("L_LeftParts_00/L_ScrollBtn_00/B_Btn_00", 9),
            ("L_LeftParts_00/L_MissionBtn_00/B_Btn_00", 10),
            ("L_LeftParts_00/L_ZanzoBtn_00/B_Btn_00", 11),
            ("L_LeftParts_00/L_WaterLevelBtn_00/B_Btn_00", 12),
            ("L_LeftParts_00/L_ScrollEditBtn_00/B_Btn_00", 13),
        ];
        let panes: Vec<String> = buttons.iter().map(|b| b.0.to_string()).collect();
        let Some(i) = hit(page, self.tick, &panes, p) else {
            // the scroll bar: S / G jump to the start / the goal, the area button switches
            // area, the bar itself scrolls [inf: as the game's handle]
            let bar = "L_Slider_00";
            let slider = [
                format!("{bar}/L_BtnS_00/B_Btn_00"),
                format!("{bar}/L_BtnG_00/B_Btn_00"),
                format!("{bar}/L_AreaBtn_00/B_Btn_00"),
                format!("{bar}/L_SlideBarH_00/B_ScrollArea_00"),
                format!("{bar}/L_VerticalBtn_00/B_Btn_00"),
                format!("{bar}/L_SlideBarV_00/B_ScrollArea_00"),
            ];
            if let Some(k) = hit(page, self.tick, &slider, p) {
                match k {
                    0 => self.cam[0] = 0.0,
                    1 => self.cam[0] = f32::MAX,
                    2 => self.switch_area(),
                    3 | 5 => {
                        self.drag = Some(Drag::Slider);
                        self.slide_to(p);
                    }
                    // the sub-area turns vertical / horizontal [inf: the arrow button]
                    _ if self.editor.area == 1 => {
                        let v = self.editor.area().header.orientation == 0;
                        self.editor.checkpoint();
                        match self.editor.set_vertical(v) {
                            Ok(()) => {
                                self.cam = [0.0, 0.0];
                                self.changed();
                            }
                            Err(e) => {
                                self.editor.discard_checkpoint();
                                refuse(cx, &self.ui, &e);
                            }
                        }
                    }
                    _ => {}
                }
                self.clamp_cam();
                return true;
            }
            // the Reset Rocket: a new course (undo brings the old one back)
            let reset = ["L_RightParts_00/L_ResetBtn_00/B_Btn_00".to_string()];
            if hit(page, self.tick, &reset, p).is_some() {
                self.reset_course();
                return true;
            }
            return false;
        };
        let pane = buttons[i].0.trim_end_matches("/B_Btn_00").to_string();
        // the mode buttons do nothing while greyed (no liquid / no custom autoscroll)
        let greyed = match buttons[i].1 {
            12 => !water_ok,
            13 => !scroll_ok,
            _ => false,
        };
        if greyed {
            self.sound(7, "invalid");
            return true;
        }
        self.ui.sound_button(PAGE, &pane, Event::Decide);
        page.play(&format!("{pane}/Decide"), self.tick);
        match buttons[i].1 {
            12 => self.enter_water(),
            13 => self.enter_scroll(),
            0 => self.open_ring(),
            1 => self.start_test(cx, None),
            2 => self.undo(),
            3 => self.toggle_eraser(),
            4 => self.open_menu(),
            5 => self.open_save(),
            10 => self.open_mission(),
            11 => self.toggle_trail(cx),
            w => self.open_window(w - 6),
        }
        true
    }

    fn reset_course(&mut self) {
        self.editor.checkpoint();
        let style = self.style();
        let fresh = new_course(&style);
        self.editor.course = fresh.course;
        self.editor.set_area(0);
        self.editor.area = 0;
        self.mario = None;
        // a new course: a new scene (its style / theme set-up empties the trail) [inf]
        self.trail.clear();
        self.cam = [0.0, 0.0];
        self.changed();
        if let Some(p) = &mut self.page {
            p.show("L_ResetRocket_00", true);
            p.play("L_ResetRocket_00/Reset", self.tick);
            self.ui.sound_key(PAGE, "Anm_Reset_ResetRocket", &[]);
            p.play("L_RightParts_00/L_ResetBtn_00/Decide", self.tick);
        }
        self.ui
            .sound_button(PAGE, "L_RightParts_00/L_ResetBtn_00", Event::Decide);
    }

    /// Palette slots, Undodog, the area theme text … after a change.
    fn update_page(&mut self) {
        let style = self.style();
        let palette = self.palette.clone();
        let mut pictures: Vec<Option<String>> = Vec::new();
        for t in &palette {
            pictures.push(self.icon_key(&style, t.part()));
        }
        let can_undo = self.editor.can_undo();
        let (theme, night) = self.editor.theme();
        let scroll = self.editor.area().header.autoscroll_type;
        let Some(p) = &mut self.page else { return };
        // the left column's pictures: the style's logo, the theme, the autoscroll [obs data:
        // `GameSkin` logos, `Lyt_Edit_Icon`]
        let left = "L_LeftParts_00";
        p.image(
            &format!("{left}/L_SkinBtn_00/P_Sample_00"),
            "GameSkin",
            &format!("{style}_Lyt_Logo_00"),
        );
        let scene = if night {
            "night"
        } else {
            THEME_TEXTURES[theme as usize % 10]
        };
        p.image(
            &format!("{left}/L_SceneBtn_00/P_Sample_00"),
            view::GROUP,
            &format!("Lyt_E_SceneSmall_{scene}_00"),
        );
        p.image(
            &format!("{left}/L_ScrollBtn_00/P_Sample_00"),
            view::GROUP,
            &format!("Lyt_E_ScrollIcon_{:02}", scroll.min(7)),
        );
        for i in 0..SLOTS {
            let part = format!("L_PaletteSet_00/{}", slot_part(i));
            match palette.get(i).zip(pictures.get(i)) {
                Some((t, picture)) => {
                    p.show(&part, true);
                    p.hold(&format!("{part}/Invisible"), 0.0);
                    p.hold(
                        &format!("{part}/Check"),
                        if i == self.slot && !self.eraser {
                            1.0
                        } else {
                            0.0
                        },
                    );
                    p.hold(
                        &format!("{part}/Select"),
                        if i == self.slot { 2.0 } else { 0.0 },
                    );
                    let cat = catalog::entry(*t).map_or(0, |e| e.category.index());
                    p.hold(&format!("{part}/Category"), cat as f32);
                    let icon = format!("{part}/P_Icon_00");
                    match picture {
                        Some(name) => {
                            p.show(&icon, true);
                            p.image(&icon, "PartIcon", name);
                        }
                        None => p.show(&icon, false),
                    }
                }
                None => {
                    p.hold(&format!("{part}/Invisible"), 1.0);
                    p.show(&format!("{part}/P_Icon_00"), false);
                }
            }
        }
        self.update_mode_buttons();
        let Some(p) = &mut self.page else { return };
        // the Undodog comes in once there is something to undo [inf]
        if can_undo {
            if !p.is_playing("L_RightParts_00/InUndoBtn") {
                p.stop("L_RightParts_00/OutUndoBtn");
                p.play("L_RightParts_00/InUndoBtn", self.tick);
            }
        } else if !p.is_playing("L_RightParts_00/OutUndoBtn") {
            p.stop("L_RightParts_00/InUndoBtn");
            p.hold_end("L_RightParts_00/OutUndoBtn");
        }
    }

    /// The UI-cache texture name of the part's picture ([`MakerFlow::load_icons`] asks the
    /// host for it).
    fn icon_key(&self, style: &str, part: i16) -> Option<String> {
        self.icon_key_flags(style, part, DEFAULT_FLAGS)
    }

    /// The picture of `part` placed with `flags` (wings, a parachute, its other form …).
    fn icon_key_flags(&self, style: &str, part: i16, flags: u32) -> Option<String> {
        self.icons
            .get(&(style.to_string(), part, flags))
            .cloned()
            .flatten()
    }

    // ------------------------------------------------------------ test play

    /// Test play: `from` None = from the maker's Mario, Some(None) = from the start,
    /// Some(Some(area, px)) = from there.
    fn start_test(&mut self, cx: &mut crate::Cx, from: Option<Option<(usize, [f32; 2])>>) {
        self.end_drag_if_any();
        let start = match from {
            None => self.mario,
            Some(s) => s,
        };
        let Ok(bytes) = self.editor.save() else {
            return;
        };
        let (area, at) = match start {
            Some((a, p)) => (a, Some(p)),
            None => (0, None),
        };
        // the maker's music stops before the test play's own starts
        if self.music_sent.as_ref().is_some_and(|m| m.is_some()) {
            cx.host.edit_music(None);
            self.music_sent = Some(None);
        }
        if let Err(e) = cx.host.test_play(&bytes, area, at) {
            cx.toast(&e);
            return;
        }
        // the trail starts again with each test play (the snapshot manager's draw state goes
        // to cState_ResetHistory when the game leaves the editor) [obs 0x7101333660]
        self.trail.clear();
        self.ui
            .sound_button("Edit_Clapperboard_00", "PlayBtn", Event::Decide);
        let mut screens = CourseScreens::new(&self.ui);
        let status = cx.host.course_status();
        if let Some(s) = &mut screens {
            s.info = CardInfo {
                name: self.editor.course.header.name.to_string_lossy(),
                ..Default::default()
            };
            s.apply_rules();
            s.update(&status);
            s.start_playing();
        }
        self.test = Some(Test {
            screens,
            ended: None,
            was_dying: false,
        });
        self.mode = Mode::Test;
        self.shown = false;
    }

    fn end_drag_if_any(&mut self) {
        if self.drag.is_some() {
            if let Some(Drag::Carry { .. } | Drag::CarryPipe { .. }) = self.drag {
                self.editor.discard_checkpoint();
                self.drag = None;
            } else {
                self.end_drag();
            }
        }
    }

    fn test_tick(&mut self, _input: &MakerInput, play_pressed: bool, cx: &mut crate::Cx) {
        let Some(t) = &mut self.test else {
            self.mode = Mode::Edit;
            return;
        };
        cx.host.step_course(None);
        let style = self.editor.course.header.game_style.as_str_lossy();
        self.trail.record(cx.host, &self.ui, &style);
        let s = cx.host.course_status();
        if let Some(sc) = &mut t.screens {
            sc.update(&s);
            sc.step();
        }
        let mut back = play_pressed;
        let mut keep_mario = !s.dying;
        if s.cleared || (t.was_dying && !s.dying) {
            // the clear's goal walk / the death: a moment, then back [inf]
            let e = t.ended.get_or_insert(0);
            *e += 1;
            if *e >= CLEAR_TICKS || (t.was_dying && !s.dying) {
                back = true;
                keep_mario = false;
            }
        }
        t.was_dying = s.dying;
        if s.dying && s.restart_in == 0 {
            back = true;
            keep_mario = false;
        }
        if back {
            if keep_mario && !s.cleared {
                // Mario stays where the test play ended (its view px → world)
                self.mario = cx.host.player_position();
            }
            self.end_test(cx);
        }
    }

    fn end_test(&mut self, cx: &mut crate::Cx) {
        self.test = None;
        self.trail.enter();
        self.play_hold = -1;
        self.mode = Mode::Edit;
        self.dirty = true;
        cx.host.set_paused(false);
        self.show(cx.host);
    }

    // ------------------------------------------------------------ the part window

    fn ring_pages(&self, category: usize) -> Vec<Vec<Entry>> {
        catalog::pages(
            &catalog::of(catalog::CATEGORIES[category], &self.style()),
            RING,
        )
    }

    fn open_ring(&mut self) {
        // open on the selected part's category and page
        let tool = self.tool();
        let mut ring = Ring::default();
        if let Some(e) = catalog::entry(tool) {
            ring.category = e.category.index();
            for (pi, pg) in self.ring_pages(ring.category).iter().enumerate() {
                if let Some(si) = pg.iter().position(|x| x.tool == tool) {
                    ring.page = pi;
                    ring.sel = si;
                }
            }
        }
        self.ring = ring;
        self.mode = Mode::Ring;
        self.ui
            .sound_button(PAGE, "L_PaletteSet_00/L_SearchBtn_00", Event::Decide);
        if let Some(p) = &mut self.page {
            p.show("L_RingMenu_00", true);
            // (the blurred capture behind the ring: a dimmed course instead, see `view`)
            p.show("L_RingMenu_00/PF_Base_00", false);
            p.stop("L_RingMenu_00/Out");
            p.play("L_RingMenu_00/In", self.tick);
            // the palette bar makes room for the category bar [inf]
            p.stop("L_PaletteSet_00/In");
            p.play("L_PaletteSet_00/Out", self.tick);
        }
        self.layout_ring(true);
    }

    fn close_ring(&mut self) {
        self.mode = Mode::Edit;
        if let Some(p) = &mut self.page {
            p.stop("L_RingMenu_00/In");
            p.play("L_RingMenu_00/Out", self.tick);
            p.stop("L_PaletteSet_00/Out");
            p.play("L_PaletteSet_00/In", self.tick);
        }
        self.ui.sound_back(PAGE, "normal");
    }

    /// Fill the ring with the current page: pictures, how many, which is selected, the
    /// category bar.
    fn layout_ring(&mut self, change: bool) {
        let pages = self.ring_pages(self.ring.category);
        let page = pages.get(self.ring.page).cloned().unwrap_or_default();
        let n = page.len().max(1);
        self.ring.sel = self.ring.sel.min(n - 1);
        let style = self.style();
        let mut pictures = Vec::new();
        for e in &page {
            pictures.push(self.icon_key(&style, e.tool.part()));
        }
        let name = page.get(self.ring.sel).map(|e| self.part_name(e));
        let title = self.ui.message(
            PAGE,
            &format!(
                "CategoryName_{}",
                catalog::CATEGORIES[self.ring.category].key()
            ),
        );
        let tick = self.tick;
        let (cat, sel, pg) = (self.ring.category, self.ring.sel, self.ring.page);
        let Some(p) = &mut self.page else { return };
        let set = "L_RingMenu_00/L_RingIconSet_00";
        p.hold(&format!("{set}/Type"), (n.max(2) - 1) as f32);
        p.hold(&format!("{set}/Select"), sel as f32);
        p.hold(&format!("{set}/Category"), cat as f32);
        for k in 0..RING {
            let icon = format!("{set}/L_RingIcon_{:02}", k + 1);
            if let Some(picture) = pictures.get(k) {
                p.visible(&icon, None);
                let pic = format!("{icon}/P_Dummy_00");
                match picture {
                    Some(name) => {
                        p.show(&pic, true);
                        p.image(&pic, "PartIcon", name);
                    }
                    None => p.show(&pic, false),
                }
                p.show(&format!("{icon}/L_NewIcon_00"), false);
            } else {
                p.show(&icon, false);
            }
        }
        // the centre: the part under the selector
        let centre = format!("{set}/L_RingIcon_00");
        p.show(&format!("{centre}/L_NewIcon_00"), false);
        match pictures.get(sel).cloned().flatten() {
            Some(name) => {
                p.show(&format!("{centre}/P_Dummy_00"), true);
                p.image(&format!("{centre}/P_Dummy_00"), "PartIcon", &name);
            }
            None => p.show(&format!("{centre}/P_Dummy_00"), false),
        }
        // the category bar: the wide tab on this category, the page dots
        let bar = "L_RingMenu_00/L_Category_00";
        p.show("L_RingMenu_00/L_Category_01", false);
        for c in 0..4 {
            let a = format!("{bar}/ChangeCategory{c:02}");
            if c == cat {
                if change {
                    p.play(&a, tick);
                } else {
                    p.hold_end(&a);
                }
            } else {
                p.stop(&a);
            }
        }
        p.hold(&format!("{bar}/TypeColor"), cat.min(2) as f32);
        p.hold(
            &format!("{bar}/PageVolume"),
            (pages.len().max(1) - 1) as f32,
        );
        for d in 0..6 {
            let dot = format!("{bar}/P_Page_{d:02}");
            p.show(&dot, d < pages.len() && pages.len() > 1);
            p.set_pane(
                &dot,
                mmk_ui::PaneOverride {
                    scale: Some(if d == pg { [1.0, 1.0] } else { [0.6, 0.6] }),
                    ..Default::default()
                },
            );
        }
        if let Some(t) = title {
            p.text(&format!("{bar}/T_Category_00"), &t);
        }
        let balloon = "L_RingMenu_00/L_Balloon_00";
        p.show(&format!("{balloon}/T_Balloon_01"), false);
        if let Some(n) = name {
            p.text(&format!("{balloon}/T_Balloon_00"), &n);
        }
    }

    fn ring_tick(
        &mut self,
        pressed: Buttons,
        keys: u16,
        click: Option<[f32; 2]>,
        cx: &mut crate::Cx,
    ) {
        let _ = cx;
        let pages = self.ring_pages(self.ring.category);
        let n = pages.get(self.ring.page).map_or(1, |p| p.len().max(1));
        if keys & keys::PARTS != 0 || pressed.has(Buttons::BACK) {
            self.close_ring();
            return;
        }
        if let Some(p) = click {
            self.ring_click(p);
            return;
        }
        if pressed.has(Buttons::RIGHT) || pressed.has(Buttons::LEFT) {
            let step = if pressed.has(Buttons::RIGHT) {
                1
            } else {
                n - 1
            };
            self.ring.sel = (self.ring.sel + step) % n;
            self.ui.sound_key(PAGE, "p_MoveRightRingMenu", &[]);
            self.layout_ring(false);
        } else if pressed.has(Buttons::UP) || pressed.has(Buttons::DOWN) {
            self.flip_page(pressed.has(Buttons::DOWN));
        } else if pressed.has(Buttons::CONFIRM) {
            self.choose_ring();
        }
    }

    /// Next / previous page, then the next / previous category.
    fn flip_page(&mut self, forward: bool) {
        let pages = self.ring_pages(self.ring.category).len().max(1);
        let cats = catalog::CATEGORIES.len();
        let mut change = false;
        if forward {
            if self.ring.page + 1 < pages {
                self.ring.page += 1;
            } else {
                self.ring.category = (self.ring.category + 1) % cats;
                self.ring.page = 0;
                change = true;
            }
        } else if self.ring.page > 0 {
            self.ring.page -= 1;
        } else {
            self.ring.category = (self.ring.category + cats - 1) % cats;
            self.ring.page = self.ring_pages(self.ring.category).len().max(1) - 1;
            change = true;
        }
        self.ring.sel = 0;
        self.ui.sound_key(PAGE, "p_MoveLeftRingMenu", &[]);
        self.layout_ring(change);
    }

    fn ring_click(&mut self, p: [f32; 2]) {
        let tick = self.tick;
        let Some(page) = &mut self.page else { return };
        let set = "L_RingMenu_00/L_RingIconSet_00";
        let icons: Vec<String> = (0..RING)
            .map(|k| format!("{set}/L_RingIcon_{:02}/W_Square_00", k + 1))
            .collect();
        if let Some(k) = hit(page, tick, &icons, p) {
            self.ring.sel = k;
            self.layout_ring(false);
            self.choose_ring();
            return;
        }
        let tabs: Vec<String> = (0..4)
            .map(|c| format!("L_RingMenu_00/L_Category_00/L_BdBase_{c:02}"))
            .collect();
        if let Some(c) = hit(page, tick, &tabs, p) {
            if c != self.ring.category {
                self.ring = Ring {
                    category: c,
                    ..Default::default()
                };
                self.layout_ring(true);
            }
            return;
        }
        let close = ["L_RingMenu_00/L_CloseBtn_00".to_string()];
        if hit(page, tick, &close, p).is_some() {
            self.close_ring();
            return;
        }
        let sides = [
            "L_RingMenu_00/L_BdRingIconSet_01".to_string(),
            "L_RingMenu_00/L_BdRingIconSet_02".to_string(),
        ];
        if let Some(s) = hit(page, tick, &sides, p) {
            self.flip_page(s == 1);
        }
    }

    /// The selected ring part goes to the front of the palette (the palette is a history of the
    /// parts used [inf: as in the game]) and becomes the tool.
    fn choose_ring(&mut self) {
        let pages = self.ring_pages(self.ring.category);
        let Some(e) = pages.get(self.ring.page).and_then(|p| p.get(self.ring.sel)) else {
            return;
        };
        let tool = e.tool;
        self.palette.retain(|&t| t != tool);
        self.palette.insert(0, tool);
        self.palette.truncate(SLOTS);
        self.slot = 0;
        self.eraser = false;
        if let Some(p) = &mut self.page {
            p.play("L_PaletteSet_00/Sort", self.tick);
            p.hold("L_RightParts_00/L_EraserBtn_00/Toggle", 0.0);
        }
        self.ui
            .sound_button(PAGE, "L_PaletteSet_00/L_PaletteBtn_", Event::Decide);
        self.close_ring();
        self.update_page();
    }

    // ------------------------------------------------------------ the main menu

    fn menu_buttons() -> [&'static str; 7] {
        [
            "L_TitleBackBtn_00",
            "L_EditBtn_00",
            "L_LclBtn_00",
            "L_QuestBtn_00",
            "L_NetworkBtn_00",
            "L_CloseBtn_00",
            "L_MyWEditBtn_00",
        ]
    }

    fn open_menu(&mut self) {
        self.end_drag_if_any();
        self.mode = Mode::Menu;
        self.menu_sel = 1;
        self.ui
            .sound_button(PAGE, "L_RightParts_00/L_MenuBtn_00", Event::Decide);
        let tick = self.tick;
        let Some(m) = &mut self.menu else { return };
        for (pane, label) in [
            ("T_Index_00", "T_Index_00"),
            ("L_TitleBackBtn_00/T_Btn_00", "L_TitleBackBtn_00-T_Btn_00"),
            ("L_EditBtn_00/T_Btn_00", "L_EditBtn_00-T_Btn_00"),
            ("L_LclBtn_00/T_Btn_00", "L_LclBtn_00-T_Btn_00"),
            ("L_MyWEditBtn_00/T_Btn_00", "L_MyWEditBtn_00-T_Btn_00"),
            ("L_MyWLclBtn_00/T_Btn_00", "L_MyWLclBtn_00-T_Btn_00"),
            ("L_QuestBtn_00/T_Btn_00", "L_QuestBtn_00-T_Btn_00"),
            ("L_NetworkBtn_00/T_Btn_00", "L_NetworkBtn_00-T_Btn_00"),
            ("L_Lesson_00/T_Btn_00", "L_Lesson_00-T_Btn_00"),
            ("L_InfoBtn_00/T_Btn_00", "L_InfoBtn_00-T_Btn_00"),
            ("L_OptionBtn_00/T_Btn_00", "L_OptionBtn_00-T_Btn_00"),
            ("L_ControllerBtn_00/T_Btn_00", "L_ControllerBtn_00-T_Btn_00"),
            ("L_HelpBtn_00/T_Btn_00", "L_HelpBtn_00-T_Btn_00"),
        ] {
            m.text_from(pane, label);
        }
        // the Course Maker / World Maker icons, the clear condition strip: as for a course
        // without a condition [inf]
        m.show("N_Mission_00", false);
        m.show("N_CapAll_00", false);
        // nothing is new; the small buttons' name balloons only show on selection [inf]
        for b in [
            "L_TitleBackBtn_00",
            "L_EditBtn_00",
            "L_LclBtn_00",
            "L_MyWEditBtn_00",
            "L_MyWLclBtn_00",
            "L_QuestBtn_00",
            "L_NetworkBtn_00",
            "L_Lesson_00",
            "L_InfoBtn_00",
            "L_OptionBtn_00",
            "L_ControllerBtn_00",
            "L_HelpBtn_00",
        ] {
            m.show(&format!("{b}/L_NewIcon_00"), false);
        }
        for b in [
            "L_InfoBtn_00",
            "L_OptionBtn_00",
            "L_ControllerBtn_00",
            "L_HelpBtn_00",
        ] {
            m.show(&format!("{b}/N_Text_00"), false);
        }
        m.stop("Out");
        m.play("In", tick);
        self.ui.sound_key(MENU, "In_Layout", &[]);
        for b in Self::menu_buttons() {
            m.hold(&format!("{b}/Select"), 0.0);
        }
        m.hold(&format!("{}/Select", Self::menu_buttons()[1]), 2.0);
    }

    fn close_menu(&mut self) {
        self.mode = Mode::Edit;
        self.ui.sound_key(MENU, "Out_Layout", &[]);
        if let Some(m) = &mut self.menu {
            m.stop("In");
            m.play("Out", self.tick);
        }
    }

    fn menu_tick(
        &mut self,
        pressed: Buttons,
        start_pressed: bool,
        click: Option<[f32; 2]>,
        cx: &mut crate::Cx,
    ) -> Option<MakerEvent> {
        let buttons = Self::menu_buttons();
        let mut decide = false;
        if let Some(p) = click
            && let Some(m) = &mut self.menu
        {
            let panes: Vec<String> = buttons.iter().map(|b| format!("{b}/B_Btn_00")).collect();
            match hit(m, self.tick, &panes, p) {
                Some(i) => {
                    self.menu_sel = i;
                    decide = true;
                }
                None => {
                    let close = ["L_CloseArea_00".to_string()];
                    if hit(m, self.tick, &close, p).is_some() {
                        self.close_menu();
                        return None;
                    }
                }
            }
        }
        if pressed.has(Buttons::BACK) || start_pressed {
            self.ui.sound_back(MENU, "normal");
            self.close_menu();
            return None;
        }
        if let Some(d) = pressed.direction()
            && let Some(m) = &mut self.menu
        {
            m.apply(self.tick);
            let centre = |m: &Layout, b: &str| m.rect(&format!("{b}/B_Btn_00")).map(|r| r.center);
            if let Some(from) = centre(m, buttons[self.menu_sel]) {
                let targets: Vec<(usize, [f32; 2])> = buttons
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| *i != self.menu_sel)
                    .filter_map(|(i, b)| centre(m, b).map(|c| (i, c)))
                    .collect();
                if let Some(i) = nearest(from, d, &targets) {
                    m.hold(&format!("{}/Select", buttons[self.menu_sel]), 0.0);
                    self.menu_sel = i;
                    m.play(&format!("{}/Select", buttons[i]), self.tick);
                    self.ui.sound_button(MENU, buttons[i], Event::Select);
                }
            }
        }
        if !(decide || pressed.has(Buttons::CONFIRM)) {
            return None;
        }
        let b = buttons[self.menu_sel];
        self.ui.sound_button(MENU, b, Event::Decide);
        if let Some(m) = &mut self.menu {
            m.play(&format!("{b}/Decide"), self.tick);
        }
        let ev = match b {
            "L_TitleBackBtn_00" => Some(MakerEvent::Title),
            "L_LclBtn_00" => Some(MakerEvent::Coursebot),
            "L_QuestBtn_00" => Some(MakerEvent::StoryMode),
            "L_NetworkBtn_00" => Some(MakerEvent::CourseWorld),
            "L_EditBtn_00" | "L_CloseBtn_00" => {
                self.close_menu();
                None
            }
            _ => {
                cx.toast("Super Worlds aren't made here yet.");
                None
            }
        };
        if ev.is_some() {
            self.save_draft(cx.host);
        }
        ev
    }

    // ------------------------------------------------------------ Save / Load

    /// The window's buttons: Save (a saved course of ours) and Save As.
    /// The window's buttons: a course saved before has Save, Save As, Upload and Load, a new
    /// one (or someone else's) Save As and Load [obs data: the InSaved / InNew panes].
    fn save_buttons(&self) -> Vec<&'static str> {
        if self.saved() {
            vec![
                "L_Save_00",
                "L_NSaveBtn_00",
                "L_UpLoadBtn_00",
                "L_LoadBtn_00",
            ]
        } else {
            vec!["L_NSaveBtn_00", "L_LoadBtn_00"]
        }
    }

    /// The course is one of ours in the Coursebot.
    fn saved(&self) -> bool {
        self.entry.is_some() && self.based_on.is_none()
    }

    fn open_save(&mut self) {
        self.end_drag_if_any();
        self.mode = Mode::Save;
        self.save_sel = 0;
        let saved = self.saved();
        let tick = self.tick;
        let Some(s) = &mut self.save else { return };
        for (pane, label) in [
            ("T_Title_00", "T_Title_00"),
            ("L_NSaveBtn_00/T_Btn_00", "L_NSaveBtn_00-T_Btn_00"),
            ("L_Save_00/T_Btn_00", "L_Save_00-T_Btn_00"),
            ("L_UpLoadBtn_00/T_Btn_00", "L_UpLoadBtn_00-T_Btn_00"),
            ("L_LoadBtn_00/T_Btn_00", "L_LoadBtn_00-T_Btn_00"),
        ] {
            s.text_from(pane, label);
        }
        // (no upload counter: our server has no limit)
        s.show("L_UpLoadInfo_00", false);
        s.show("L_UpLoadBtn_00", saved);
        for a in ["OutNew", "OutSaved", "InNew", "InSaved"] {
            s.stop(a);
        }
        s.play(if saved { "InSaved" } else { "InNew" }, tick);
        s.show("L_Save_00", saved);
    }

    fn close_save(&mut self) {
        self.mode = Mode::Edit;
        let saved = self.saved();
        if let Some(s) = &mut self.save {
            s.stop("InNew");
            s.stop("InSaved");
            s.play(if saved { "OutSaved" } else { "OutNew" }, self.tick);
        }
    }

    fn save_tick(
        &mut self,
        pressed: Buttons,
        click: Option<[f32; 2]>,
        cx: &mut crate::Cx,
    ) -> Option<MakerEvent> {
        let buttons = self.save_buttons();
        let mut decide = false;
        if let Some(p) = click
            && let Some(s) = &mut self.save
        {
            let panes: Vec<String> = buttons.iter().map(|b| format!("{b}/B_Btn_00")).collect();
            if let Some(i) = hit(s, self.tick, &panes, p) {
                self.save_sel = i;
                decide = true;
            } else {
                let close = ["L_CloseBtn_00".to_string(), "L_BdSaveWin_00".to_string()];
                if hit(s, self.tick, &close[..1], p).is_some()
                    || hit(s, self.tick, &close[1..], p).is_none()
                {
                    self.close_save();
                    return None;
                }
            }
        }
        if pressed.has(Buttons::BACK) {
            self.ui.sound_back(SAVE, "normal");
            self.close_save();
            return None;
        }
        let n = buttons.len();
        if pressed.has(Buttons::UP) || pressed.has(Buttons::DOWN) {
            self.save_sel = (self.save_sel + if pressed.has(Buttons::UP) { n - 1 } else { 1 }) % n;
            if let Some(s) = &mut self.save {
                s.play(&format!("{}/Select", buttons[self.save_sel]), self.tick);
            }
            self.ui
                .sound_button(SAVE, buttons[self.save_sel], Event::Select);
        }
        if !(decide || pressed.has(Buttons::CONFIRM)) {
            return None;
        }
        let b = buttons[self.save_sel.min(n - 1)];
        self.ui.sound_button(SAVE, b, Event::Decide);
        if let Some(s) = &mut self.save {
            s.play(&format!("{b}/Save"), self.tick);
        }
        match b {
            // Load and Save As: the Coursebot chooses the course / the slot [obs: the editor's
            // save page requests the Coursebot's load and save sequences, coursebot.md A.1]
            "L_LoadBtn_00" => {
                self.save_draft(cx.host);
                self.close_save();
                return Some(MakerEvent::Load);
            }
            "L_NSaveBtn_00" => {
                self.save_draft(cx.host);
                self.close_save();
                return Some(MakerEvent::SaveAs);
            }
            "L_UpLoadBtn_00" => self.upload(cx),
            _ => match self.save_to_coursebot(cx.host, false) {
                Ok(()) => cx.toast("Saved to the Coursebot."),
                Err(e) => cx.toast(&format!("Not saved: {e}")),
            },
        }
        self.close_save();
        None
    }

    /// Upload the course (as saved, with its credit) to the course server as the signed-in
    /// account; the answer comes in a later tick.
    fn upload(&mut self, cx: &mut crate::Cx) {
        if let Err(e) = self.save_to_coursebot(cx.host, false) {
            cx.toast(&format!("Not saved: {e}"));
            return;
        }
        let Ok(bytes) = self.editor.save() else {
            return;
        };
        let args = serde_json::json!({ "course": hex_rle(&bytes) });
        self.upload_call = cx.online.call(cx.host, "upload", args);
        if self.upload_call.is_none() {
            cx.toast(UPLOAD_NEEDS_SERVER);
        }
    }

    /// An upload's answer: the new course's code.
    fn upload_answer(&mut self, cx: &mut crate::Cx) {
        let Some(id) = self.upload_call else { return };
        let Some(a) = cx.online.take(id) else { return };
        self.upload_call = None;
        match a {
            Ok(v) => {
                let code = v["code"].as_str().unwrap_or("?").to_string();
                cx.toast(&format!("Uploaded: {code}"));
            }
            Err(e) => cx.toast(&format!("Not uploaded: {e}")),
        }
    }

    /// Save into the Coursebot: over this course's entry, or as a new one (always for someone
    /// else's course, with the credit in the description).
    pub fn save_to_coursebot(&mut self, host: &mut dyn Host, as_new: bool) -> Result<(), String> {
        self.save_into(host, if as_new { Some(None) } else { None })
    }

    /// Save into Coursebot slot `slot` (Save As's choice).
    pub fn save_to_slot(&mut self, host: &mut dyn Host, slot: usize) -> Result<(), String> {
        self.save_into(host, Some(Some(slot)))
    }

    /// Save with the credit of someone else's course in the description: `to` None = over this
    /// course's entry (a new one when it has none or is someone else's), Some(None) = a new
    /// entry, Some(Some(slot)) = that slot.
    fn save_into(&mut self, host: &mut dyn Host, to: Option<Option<usize>>) -> Result<(), String> {
        let mut e = Editor::open(&self.editor.save().map_err(|e| format!("{e:?}"))?)
            .map_err(|e| format!("{e:?}"))?;
        if let Some(credit) = &self.based_on {
            let d = e.course.header.description.to_string_lossy();
            if !d.contains(credit.as_str()) {
                let text = if d.is_empty() {
                    credit.clone()
                } else {
                    format!("{d} ({credit})")
                };
                e.set_description(&text);
            }
        }
        let bytes = e.save().map_err(|e| format!("{e:?}"))?;
        let id = match to {
            Some(Some(slot)) => host.save_slot(slot, &bytes)?,
            Some(None) => host.save_course(None, &bytes)?,
            None if self.based_on.is_some() => host.save_course(None, &bytes)?,
            None => host.save_course(self.entry.as_deref(), &bytes)?,
        };
        self.entry = Some(id);
        self.based_on = None;
        self.editor.course = e.course;
        self.unsaved = false;
        self.save_draft(host);
        Ok(())
    }

    /// Keep the course being made (it reopens with the maker).
    pub fn save_draft(&self, host: &mut dyn Host) {
        if let Ok(b) = self.editor.save() {
            let doc = serde_json::json!({
                "course": hex_rle(&b),
                "entry": self.entry,
                "based_on": self.based_on,
            });
            host.store_value("maker.draft", &doc.to_string());
        }
    }

    // ------------------------------------------------------------ the left column windows

    /// The window's choices: (button part, value). Window 0 game style, 1 course theme, 3
    /// autoscroll; the timer window (2) has a selector instead.
    fn window_choices(w: u8, main: bool) -> Vec<(String, u8)> {
        match w {
            0 => (0..5)
                .map(|i| (format!("L_SkinListBtn_{i:02}"), i as u8))
                .collect(),
            // theme of each scene button: its layout's number − 1 (Edit_SceneListBtn_01 …
            // _10 sit in the left column then the right) [inf]
            1 => [0u8, 5, 1, 6, 2, 7, 3, 8, 4, 9]
                .iter()
                .enumerate()
                .map(|(i, &t)| (format!("L_SceneListBtn_{i:02}"), t))
                .collect(),
            // none, slow, normal, fast, custom (the main area's buttons)
            3 => [
                ("N_AutoScroll_00/L_NoAutoScrollBtn_00", 0u8),
                ("N_AutoScroll_00/N_MainBtn_00/L_AutoScrollBtn_00", 1),
                ("N_AutoScroll_00/N_MainBtn_00/L_AutoScrollBtn_01", 2),
                ("N_AutoScroll_00/N_MainBtn_00/L_AutoScrollBtn_02", 3),
                ("N_AutoScroll_00/N_MainBtn_00/L_AutoScrollBtn_06", 4),
            ]
            .iter()
            // custom autoscroll only in the main area
            .filter(|(_, v)| main || *v < 4)
            .map(|(b, v)| (b.rsplit('/').next().unwrap_or(b).to_string(), *v))
            .collect(),
            _ => vec![("L_TimerDecideBtn_00".to_string(), 0)],
        }
    }

    fn window_value(&self, w: u8) -> u8 {
        match w {
            0 => {
                let style = self.style();
                STYLES.iter().position(|s| *s == style).unwrap_or(0) as u8
            }
            1 => self.editor.theme().0,
            3 => self.editor.area().header.autoscroll_type.min(4),
            _ => 0,
        }
    }

    fn open_window(&mut self, w: u8) {
        self.mode = Mode::Window(w);
        let current = self.window_value(w);
        let choices = Self::window_choices(w, self.editor.area == 0);
        self.window_sel = choices.iter().position(|c| c.1 == current).unwrap_or(0);
        self.window_timer = self.editor.course.header.timer;
        let tick = self.tick;
        let ui = self.ui.clone();
        let Some(p) = &mut self.page else { return };
        let left = "L_LeftParts_00";
        p.show(&format!("{left}/N_Content_00"), true);
        p.hold(&format!("{left}/Type"), w as f32);
        // the window opens wide for autoscroll (ChangeTab01), else normal (ChangeTab00)
        // [obs data: the two frame widths]
        let tab = if w == 3 { "ChangeTab01" } else { "ChangeTab00" };
        self.window_tab = tab;
        p.play(&format!("{left}/{tab}"), tick);
        p.play(&format!("{left}/InContent"), tick);
        let title = match w {
            0 => "ContentName_GameSkin",
            1 => "ContentName_SceneSkin",
            2 => "ContentName_Timer",
            _ => "ContentName_AutoScrl",
        };
        if let Some(t) = ui.message(PAGE, title) {
            p.text(&format!("{left}/T_ContentName_00"), &t);
        }
        for (k, (b, v)) in choices.iter().enumerate() {
            let pane = format!("{left}/{b}");
            p.hold(
                &format!("{pane}/Check"),
                if *v == current { 1.0 } else { 0.0 },
            );
            p.hold(
                &format!("{pane}/Select"),
                if k == self.window_sel { 2.0 } else { 0.0 },
            );
            let sample = format!("{pane}/P_Sample_00");
            match w {
                0 => {
                    if let Some(t) = ui.message(PAGE, &format!("{left}-{b}-T_Balloon_00")) {
                        p.text(&format!("{pane}/T_Balloon_00"), &t);
                    }
                    p.image(
                        &sample,
                        "GameSkin",
                        &format!("{}_Lyt_Logo_00", STYLES[*v as usize]),
                    );
                }
                1 => {
                    let label = format!("SceneName_{}", SCENE_KEYS[*v as usize]);
                    if let Some(t) = ui.message(PAGE, &label) {
                        p.text(&format!("{pane}/T_Balloon_00"), &t);
                    }
                    p.image(
                        &sample,
                        view::GROUP,
                        &format!("Lyt_E_Scene_{}_00", THEME_TEXTURES[*v as usize]),
                    );
                }
                _ => {}
            }
        }
        match w {
            2 => {
                p.text_from(
                    &format!("{left}/L_TimerDecideBtn_00/T_Btn_00"),
                    "L_LeftParts_00-L_TimerDecideBtn_00-T_Btn_00",
                );
                p.show(&format!("{left}/L_TimerDecideBtn_00/T_KeyBtn_00"), false);
            }
            3 => {
                for (pane, label) in [
                    (
                        "L_NoAutoScrollBtn_00/T_NoAuto_00",
                        "L_NoAutoScrollBtn_00-T_NoAuto_00",
                    ),
                    (
                        "L_AutoScrollBtn_00/T_Slow_00",
                        "L_AutoScrollBtn_00-T_Slow_00",
                    ),
                    (
                        "L_AutoScrollBtn_01/T_Normal_00",
                        "L_AutoScrollBtn_01-T_Normal_00",
                    ),
                    (
                        "L_AutoScrollBtn_02/T_Fast_00",
                        "L_AutoScrollBtn_02-T_Fast_00",
                    ),
                    (
                        "L_ScrollDecideBtn_00/T_Btn_00",
                        "L_ScrollDecideBtn_00-T_Btn_00",
                    ),
                    (
                        "L_AutoScrollBtn_06/T_AutoEdit_00",
                        "L_AutoScrollBtn_06-T_AutoEdit_00",
                    ),
                ] {
                    p.text_from(&format!("{left}/{pane}"), &format!("{left}-{label}"));
                }
                p.show(&format!("{left}/L_AutoScrollBtn_06"), self.editor.area == 0);
            }
            _ => {}
        }
        let button = [
            "L_SkinBtn_00",
            "L_SceneBtn_00",
            "L_TimerBtn_00",
            "L_ScrollBtn_00",
        ][w.min(3) as usize];
        p.show(&format!("{left}/{button}/N_Text_00"), false);
        self.ui
            .sound_button(PAGE, &format!("{left}/{button}"), Event::Decide);
        self.update_timer();
    }

    /// The timer window's number and the timer button's.
    fn update_timer(&mut self) {
        let t = if self.mode == Mode::Window(2) {
            self.window_timer
        } else {
            self.editor.course.header.timer
        };
        let Some(p) = &mut self.page else { return };
        let left = "L_LeftParts_00";
        p.text(&format!("{left}/L_TimerBtn_00/T_Timer_00"), &t.to_string());
        p.text(
            &format!("{left}/L_Timer_00/L_CenterButton_00/T_Timer_00"),
            &t.to_string(),
        );
    }

    fn close_window(&mut self) {
        self.mode = Mode::Edit;
        if let Some(p) = &mut self.page {
            p.stop(&format!("L_LeftParts_00/{}", self.window_tab));
            p.stop("L_LeftParts_00/InContent");
            p.show("L_LeftParts_00/N_Content_00", false);
        }
        self.update_timer();
        self.update_mode_buttons();
    }

    fn window_tick(
        &mut self,
        w: u8,
        pressed: Buttons,
        click: Option<[f32; 2]>,
        cx: &mut crate::Cx,
    ) {
        let _ = cx;
        let left = "L_LeftParts_00";
        let choices = Self::window_choices(w, self.editor.area == 0);
        let mut decide = false;
        let mut timer_step = 0i16;
        if pressed.has(Buttons::BACK) {
            self.ui.sound_back(PAGE, "normal");
            self.close_window();
            return;
        }
        if let Some(p) = click
            && let Some(page) = &mut self.page
        {
            let panes: Vec<String> = choices
                .iter()
                .map(|c| format!("{left}/{}/B_Btn_00", c.0))
                .collect();
            let arrows = [
                format!("{left}/L_Timer_00/L_UpBtn_00"),
                format!("{left}/L_Timer_00/L_DownBtn_00"),
            ];
            if let Some(i) = hit(page, self.tick, &panes, p) {
                self.window_sel = i;
                decide = true;
            } else if w == 2
                && let Some(a) = hit(page, self.tick, &arrows, p)
            {
                timer_step = if a == 0 { 10 } else { -10 };
            } else {
                let window = [format!("{left}/N_Content_00/N_Scissor_00")];
                if hit(page, self.tick, &window, p).is_none() {
                    self.close_window();
                    return;
                }
            }
        }
        if w == 2 {
            // the timer: up / down by 10 s (left / right by 100) within 10–500 [obs: the
            // course validator's range, Part A §3]
            timer_step += match pressed {
                b if b.has(Buttons::UP) => 10,
                b if b.has(Buttons::DOWN) => -10,
                b if b.has(Buttons::RIGHT) => 100,
                b if b.has(Buttons::LEFT) => -100,
                _ => 0,
            };
            if timer_step != 0 {
                self.window_timer = (self.window_timer + timer_step).clamp(10, 500);
                if let Some(p) = &mut self.page {
                    let a = if timer_step > 0 { "Up" } else { "Down" };
                    p.play(
                        &format!("{left}/L_Timer_00/L_CenterButton_00/{a}"),
                        self.tick,
                    );
                }
                self.update_timer();
            }
            if decide || pressed.has(Buttons::CONFIRM) {
                if self.window_timer != self.editor.course.header.timer {
                    self.editor.checkpoint();
                    self.editor.set_timer(self.window_timer);
                    self.changed();
                }
                self.ui
                    .sound_button(PAGE, &format!("{left}/L_TimerDecideBtn_00"), Event::Decide);
                self.close_window();
            }
            return;
        }
        let n = choices.len();
        // the style and theme windows are two buttons wide, autoscroll one [obs data]
        let cols = if w == 3 { 1 } else { 2 };
        let step = match pressed {
            b if b.has(Buttons::RIGHT) && cols > 1 => Some(1),
            b if b.has(Buttons::LEFT) && cols > 1 => Some(n - 1),
            b if b.has(Buttons::DOWN) => Some(cols),
            b if b.has(Buttons::UP) => Some(n - cols),
            _ => None,
        };
        if let Some(s) = step {
            let old = self.window_sel;
            self.window_sel = (self.window_sel + s) % n;
            if let Some(p) = &mut self.page {
                p.hold(&format!("{left}/{}/Select", choices[old].0), 0.0);
                p.play(
                    &format!("{left}/{}/Select", choices[self.window_sel].0),
                    self.tick,
                );
            }
        }
        if !(decide || pressed.has(Buttons::CONFIRM)) {
            return;
        }
        let v = choices[self.window_sel].1;
        if v != self.window_value(w) {
            self.editor.checkpoint();
            match w {
                0 => {
                    self.editor.set_style(STYLES[v as usize % 5]);
                    self.icons.clear();
                    self.reset_palette_for_style();
                }
                1 => self.editor.set_theme(v),
                _ => self.editor.set_autoscroll(v),
            }
            // the first liquid theme / the first custom autoscroll: its tip [obs: the
            // reference frames]
            let tip = match w {
                1 if self.water_available() => Some(help::Tip::WaterLevel),
                3 if self.scroll_available() => Some(help::Tip::CustomScroll),
                _ => None,
            };
            if let Some(t) = tip {
                self.pending_tip = Some(t);
            }
            // the scene's style / theme set-up empties the trail [obs 0x7100e3e470]
            if w < 2 {
                self.trail.clear();
            }
            self.changed();
        }
        self.ui.sound_button(
            PAGE,
            &format!("{left}/{}", choices[self.window_sel].0),
            Event::Decide,
        );
        self.close_window();
    }

    // ------------------------------------------------------------ the clear condition

    /// The course's clear condition: (category, condition-list index, amount).
    pub fn clear_condition(&self) -> (u8, u8, i16) {
        let h = &self.editor.course.header;
        let index = mmk_formats::smm2::clear::index(h.clear_condition).unwrap_or(0);
        (
            h.clear_condition_category,
            index,
            h.clear_condition_magnitude,
        )
    }

    fn open_mission(&mut self) {
        let current = self.clear_condition();
        self.mission = mission::MissionWindow::new(&self.ui, &self.style(), current);
        if self.mission.is_some() {
            self.mode = Mode::Mission;
        }
    }

    fn mission_done(&mut self, e: mission::MissionEvent) {
        use mission::MissionEvent as M;
        let before = self.clear_condition();
        let change = match e {
            M::Set(c, i, n) => Some((c, i, n)),
            M::Remove => Some((0, 0, 0)),
            M::Close => None,
        };
        if let Some((c, i, n)) = change
            && (c, i, n) != before
        {
            self.editor.checkpoint();
            if self.editor.set_clear_condition(c, i, n).is_ok() {
                self.changed();
            } else {
                self.editor.discard_checkpoint();
            }
        }
        if let Some(m) = &mut self.mission {
            m.layout.play("Out", m.tick);
        }
        self.mission = None;
        self.mode = Mode::Edit;
        self.update_mission_button();
    }

    /// The left column's clear-condition button: the condition's icon and amount.
    fn update_mission_button(&mut self) {
        let (c, i, n) = self.clear_condition();
        let cond = mission::condition(c, i, n).filter(|_| c != 0);
        let style = STYLES.iter().position(|s| *s == self.style()).unwrap_or(0);
        let Some(p) = &mut self.page else { return };
        let b = "L_LeftParts_00/L_MissionBtn_00";
        p.show(&format!("{b}/P_None_00"), cond.is_none());
        p.show(&format!("{b}/P_CountBase_00"), cond.is_some() && n > 0);
        p.show(&format!("{b}/T_Mission_00"), cond.is_some() && n > 0);
        p.text(&format!("{b}/T_Mission_00"), &n.to_string());
        match &cond {
            Some(k) => {
                p.show(&format!("{b}/P_MissionIcon_00"), true);
                crate::screens::set_icon(p, k, &format!("{b}/P_MissionIcon_00"), style);
            }
            None => {
                p.picture(&format!("{b}/P_MissionIcon_00"), "MissionIcon", None);
            }
        }
    }

    /// Mario's Trail's button: its Check frame 1 (`EditBtn_Zanzo_01`) while the trail is shown
    /// [obs data]; always there and usable [inf: no code hides or greys button 0x3e].
    fn update_trail_button(&mut self) {
        let on = self.trail_on;
        if let Some(p) = &mut self.page {
            p.hold(
                "L_LeftParts_00/L_ZanzoBtn_00/Check",
                if on { 1.0 } else { 0.0 },
            );
        }
    }

    /// The button pressed: the trail on / off, kept for next time (the game: the save data's
    /// flags word) [obs 0x71018c9280 case 0x3e]; switched on, it comes in again from its first
    /// picture.
    fn toggle_trail(&mut self, cx: &mut crate::Cx) {
        self.trail_on = !self.trail_on;
        cx.host
            .store_value(TRAIL_KEY, if self.trail_on { "1" } else { "0" });
        if self.trail_on {
            // the Check animation forward has a sound (yr_Delay_01) [obs data]
            self.ui.sound_key(PAGE, "Anm_Check_zanzoBtn_Forward", &[]);
        }
        self.update_trail_button();
        // the label Decide shows goes with the button's Cancel (the game's, when the cursor
        // leaves it) a second later [inf]
        self.trail_label = Some(self.tick + 60);
    }

    /// Mario's Trail's pictures over the edit view: (layout rectangle x0, y0, x1, y1, texture,
    /// alpha), oldest first.
    pub fn trail_quads(&self) -> Vec<([f32; 4], u32, f32)> {
        if self.mode == Mode::Test || !self.trail_on {
            return Vec::new();
        }
        let mut cache = self.ui.cache_mut();
        self.trail
            .visible(self.editor.area)
            .into_iter()
            .filter_map(|(r, name, alpha)| {
                let t = cache.texture_file(trail::GROUP, name)?;
                let [x0, y0] = self.to_layout(r[0], r[1]);
                let [x1, y1] = self.to_layout(r[2], r[3]);
                Some(([x0, y0, x1, y1], t.id, alpha))
            })
            .collect()
    }

    /// After a style change: parts the style lacks leave the palette.
    fn reset_palette_for_style(&mut self) {
        let style = self.style();
        self.palette = self
            .palette
            .iter()
            .map(|&t| match t {
                Tool::Part(16) if style == "3W" => Tool::Part(71),
                Tool::Part(71) if style != "3W" => Tool::Part(16),
                t => t,
            })
            .filter(|&t| catalog::entry(t).is_some_and(|e| catalog::available(&e, &style)))
            .collect();
        if self.palette.is_empty() {
            self.reset_palette();
        }
        self.slot = self.slot.min(self.palette.len() - 1);
    }

    // ------------------------------------------------------------ drawing

    pub fn draw(&mut self, out: &mut Vec<DrawItem>) {
        if self.mode == Mode::Test {
            out.push(DrawItem::Course);
            if let Some(s) = self.test.as_mut().and_then(|t| t.screens.as_mut()) {
                s.draw(out);
            }
            return;
        }
        if self.shown {
            out.push(DrawItem::Course);
        }
        let over = view::overlay(self);
        out.push(over);
        let tick = self.tick;
        if let Some(p) = &mut self.page {
            push_layout(out, p, tick);
        }
        if let Some(l) = self.speed_balloon_layout() {
            push_layout(out, l, tick);
        }
        if self.mode == Mode::Help
            && let Some(h) = &mut self.help
        {
            // the button where the page has it now (it may still be coming in)
            let target = self
                .page
                .as_ref()
                .and_then(|p| p.rect(h.button()))
                .map(|r| (r.center, r.size));
            out.push(DrawItem::Layout {
                ui: crate::layout::UiKind::Smm2,
                size: [1280.0, 720.0],
                quads: h.quads(target),
            });
            if let Some(l) = &mut h.layout {
                push_layout(out, l, h.tick);
            }
        }
        if self.mode == Mode::Attr
            && let Some(b) = &mut self.attr
            && let Some(l) = &mut b.layout
        {
            push_layout(out, l, tick);
            let pane = format!("{}/B_Btn_00", attr::button(b.sel));
            self.cursor.place(l, &pane, tick, 1.0);
            self.cursor.draw(tick, out);
        }
        if self.mode == Mode::Menu
            && let Some(m) = &mut self.menu
        {
            push_layout(out, m, tick);
            let pane = format!("{}/B_Btn_00", Self::menu_buttons()[self.menu_sel]);
            self.cursor.place(m, &pane, tick, 1.0);
            self.cursor.draw(tick, out);
        } else if self.mode == Mode::Mission
            && let Some(m) = &mut self.mission
        {
            push_layout(out, &mut m.layout, m.tick);
        } else if self.mode == Mode::SubArea
            && let Some(w) = &mut self.subarea
        {
            push_layout(out, &mut w.layout, w.tick);
            let pane = format!("{}/B_Btn_00", subarea::BUTTONS[w.sel]);
            self.cursor.place(&mut w.layout, &pane, tick, 1.0);
            self.cursor.draw(tick, out);
        } else if self.mode == Mode::Save {
            let b = self.save_buttons();
            let pane = format!("{}/B_Btn_00", b[self.save_sel.min(b.len() - 1)]);
            let Some(s) = &mut self.save else { return };
            push_layout(out, s, tick);
            self.cursor.place(s, &pane, tick, 1.0);
            self.cursor.draw(tick, out);
        }
        if let Some(c) = view::glove(self) {
            out.push(c);
        }
    }

    /// Captures: the scripted input of this tick added to the real one. Items `tick:what`,
    /// what = a menu button (confirm, back, up, down, left, right, pressed and held for that
    /// tick; with a `~` only held, as a press that goes on), start,
    /// play (the test-play key), a maker key (undo, redo, parts, next, prev, here, eraser,
    /// area), `cursor=x;y` (put the cursor on a tile) or `slot=n`.
    fn scripted(&mut self, input: &MakerInput) -> MakerInput {
        let mut i = *input;
        let due: Vec<String> = self
            .script
            .iter()
            .filter(|(t, _)| *t == self.tick)
            .map(|(_, w)| w.clone())
            .collect();
        for what in due {
            if let Some(b) = Buttons::from_name(&what) {
                i.ui.pressed = i.ui.pressed | b;
                i.ui.held = i.ui.held | b;
                continue;
            }
            // `confirm~`: held on from an earlier tick (a press that lasts)
            if let Some(b) = what.strip_suffix('~').and_then(Buttons::from_name) {
                i.ui.held = i.ui.held | b;
                continue;
            }
            if let Some(xy) = what.strip_prefix("cursor=")
                && let Some((x, y)) = xy.split_once(';')
                && let (Ok(x), Ok(y)) = (x.parse(), y.parse())
            {
                self.cur = (x, y);
                self.follow_cursor();
                self.glove = self.tile_center(self.cur);
                continue;
            }
            if let Some(n) = what
                .strip_prefix("slot=")
                .and_then(|n| n.parse::<usize>().ok())
            {
                self.select_slot(n.min(self.palette.len().saturating_sub(1)));
                continue;
            }
            match what.as_str() {
                "start" => i.start = true,
                "play" => i.play = true,
                "undo" => i.keys |= keys::UNDO,
                "redo" => i.keys |= keys::REDO,
                "parts" => i.keys |= keys::PARTS,
                "next" => i.keys |= keys::NEXT,
                "prev" => i.keys |= keys::PREV,
                "here" => i.keys |= keys::PLAY_HERE,
                "eraser" => i.keys |= keys::ERASER,
                "area" => i.keys |= keys::AREA,
                // a tap on Mario's Trail's button
                "trail" => {
                    if let Some(p) = &mut self.page {
                        p.apply(self.tick);
                        i.ui.click = p
                            .rect("L_LeftParts_00/L_ZanzoBtn_00/B_Btn_00")
                            .map(|r| r.center);
                    }
                }
                _ => {}
            }
        }
        i
    }

    /// Captures: set the script (see [`MakerFlow::scripted`]).
    pub fn set_script(&mut self, s: &str) {
        self.script = s
            .split(',')
            .filter_map(|item| {
                let (t, w) = item.split_once(':')?;
                Some((t.trim().parse().ok()?, w.trim().to_string()))
            })
            .collect();
    }

    /// Show the course again (coming back from the Coursebot: the host drew other things).
    pub fn redraw(&mut self) {
        self.dirty = true;
        self.mode = Mode::Edit;
        self.update_page();
    }

    /// Hand the course to the host now.
    pub fn show_now(&mut self, cx: &mut crate::Cx) {
        self.show(cx.host);
        self.load_icons(cx.host);
    }

    /// The Water Level / Custom Scroll mode's lines and handles (view overlay).
    pub fn mode_quads(&self, q: &mut Vec<mmk_ui::Quad>) {
        match self.mode {
            Mode::Water => self.liquid_quads(q, true),
            Mode::Scroll => self.scroll_quads(q, true),
            _ => {}
        }
    }

    /// Outside the modes, what the game still shows of them, faint: a moving liquid's two
    /// lines (in the themes with a liquid), the custom scroll's birds (custom autoscroll)
    /// [obs 0x7100effd00, 0x7100f01ce0, 0x7100eb27e0: const_color0's alpha 0.4].
    pub fn faint_mode_quads(&self, q: &mut Vec<mmk_ui::Quad>) {
        if self.water_available() {
            self.liquid_quads(q, false);
        }
        if self.scroll_available() {
            self.scroll_quads(q, false);
        }
    }

    /// An `EditModel` texture's id, if the import has it.
    pub fn ui_texture(&self, name: &str) -> Option<u32> {
        self.textures.get(name).copied()
    }

    /// The glove: hidden while a menu is up.
    pub fn glove_shown(&self) -> bool {
        matches!(self.mode, Mode::Edit | Mode::Water | Mode::Scroll)
    }

    pub fn glove_at(&self) -> [f32; 2] {
        self.glove
    }

    pub fn carrying(&self) -> bool {
        matches!(self.drag, Some(Drag::Carry { .. }))
    }

    /// The part window is open (the course is dimmed behind it).
    pub fn ring_open(&self) -> bool {
        self.mode == Mode::Ring
    }

    pub fn testing(&self) -> bool {
        self.mode == Mode::Test
    }

    /// Captures: open a window ("parts", "menu", "save", "style", "theme", "play", "subarea";
    /// "vertical": a vertical sub-area; "water", "water-rising", "water-both", "water-speed",
    /// "water-tip": the Water Level mode in the forest; "scroll", "scroll-nodes", "scroll-tip":
    /// Custom Scroll).
    pub fn debug_open(&mut self, cx: &mut crate::Cx, what: &str) -> bool {
        match what {
            "parts" => self.open_ring(),
            "menu" => self.open_menu(),
            "save" => self.open_save(),
            "style" => self.open_window(0),
            "theme" => self.open_window(1),
            "timer" => self.open_window(2),
            "autoscroll" => self.open_window(3),
            "mission" => self.open_mission(),
            "play" => self.start_test(cx, Some(None)),
            // the sub-area's orientation window, and a vertical sub-area (its scroll bar)
            "subarea" => self.switch_area(),
            "vertical" => {
                self.editor.set_area(1);
                let _ = self.editor.set_vertical(true);
                self.cam = [0.0, 0.0];
                self.dirty = true;
            }
            // clear pipes drawn on the first screen (a 3D World course: --style 3W): up, round
            // to the right and down, and a straight one; the Clear Pipe in the first slot
            "clearpipe" => {
                use mmk_formats::smm2::clear_pipe::Dir as P;
                let Ok(k) = self.editor.place_clear_pipe((12, 4), P::Up) else {
                    return false;
                };
                for t in [(12, 8), (16, 9), (17, 3)] {
                    let _ = self.editor.draw_clear_pipe(k, PipeEnd::End, t);
                }
                if let Ok(j) = self.editor.place_clear_pipe((19, 10), P::Right) {
                    let _ = self.editor.draw_clear_pipe(j, PipeEnd::End, (22, 9));
                }
                self.palette[0] = Tool::ClearPipe;
                self.slot = 0;
                self.dirty = true;
                self.update_page();
            }
            // the Water Level mode in the forest: still water, rising (the button, its path),
            // rising and falling, the speed balloon; its tip
            "water" | "water-rising" | "water-both" | "water-speed" | "water-tip" => {
                self.editor.set_theme(9);
                let moving = what != "water" && what != "water-tip";
                self.editor.set_liquid(mmk_editor::settings::Liquid {
                    mode: if what == "water-both" { 2 } else { 1 },
                    speed: 1,
                    start: 1,
                    end: if moving { 8 } else { 1 },
                });
                self.dirty = true;
                self.update_page();
                if what == "water-tip" {
                    cx.host.store_value(help::Tip::WaterLevel.key(), "0");
                    self.show_tip(cx.host, help::Tip::WaterLevel);
                    return true;
                }
                self.enter_water();
                if what == "water-speed" {
                    self.editor.checkpoint();
                    self.water.drag = Some(liquid::Handle::Target);
                    self.water.held = liquid::HOLD;
                    let at = self.to_layout(self.liquid_anchor() - 16.0, liquid::handle_y(8));
                    self.water_hold(at);
                }
            }
            // the Custom Scroll mode: the start alone, then with nodes at the three speeds;
            // its tip
            "scroll" | "scroll-nodes" | "scroll-tip" => {
                self.editor.set_autoscroll(4);
                if what == "scroll-nodes" {
                    self.editor.set_custom_scroll_speed(1);
                    // (on the first screen: the numbered birds and the turns)
                    let _ = self.editor.add_scroll_node(17, 10, 2);
                    let _ = self.editor.add_scroll_node(21, 5, 0);
                }
                self.dirty = true;
                self.update_page();
                if what == "scroll-tip" {
                    cx.host.store_value(help::Tip::CustomScroll.key(), "0");
                    self.show_tip(cx.host, help::Tip::CustomScroll);
                    return true;
                }
                self.enter_scroll();
            }
            _ => return false,
        }
        true
    }

    /// Hand the host-made part pictures to the UI cache (call before a tick's layout).
    pub fn load_icons(&mut self, host: &mut dyn Host) {
        let style = self.style();
        let mut wanted: Vec<(i16, u32)> = self
            .palette
            .iter()
            .map(|t| (t.part(), DEFAULT_FLAGS))
            .collect();
        if self.mode == Mode::Ring {
            for pg in self.ring_pages(self.ring.category) {
                wanted.extend(pg.iter().map(|e| (e.tool.part(), DEFAULT_FLAGS)));
            }
        }
        // the balloon's pictures: the part as each choice would make it
        if let Some(b) = &self.attr
            && let Some(o) = self.editor.object(b.index)
        {
            wanted.extend((0..b.options.len()).map(|k| (b.part, b.flags_after(k, o.flags))));
        }
        let mut any = false;
        for (part, flags) in wanted {
            let k = (style.clone(), part, flags);
            if self.icons.contains_key(&k) {
                continue;
            }
            let icon = host.part_icon(&style, part, flags);
            if icon.is_none() && host.art_loading() {
                continue; // (asked for again once the art is read)
            }
            let name = icon.map(|icon| {
                let name = if flags == DEFAULT_FLAGS {
                    format!("{style}_{part}")
                } else {
                    format!("{style}_{part}_{flags:x}")
                };
                let icon = fit_icon(&icon);
                let tex = mmk_assets::Mtex {
                    format: mmk_assets::MtexFormat::Rgba8,
                    variant: mmk_assets::MtexVariant::Srgb,
                    width: icon.width,
                    height: icon.height,
                    channel_select: [2, 3, 4, 5],
                    data: icon.rgba,
                };
                self.ui.cache_mut().add_texture_file("PartIcon", &name, tex);
                name
            });
            self.icons.insert(k, name);
            any = true;
        }
        if any {
            self.update_page();
            if self.mode == Mode::Ring {
                self.layout_ring(false);
            }
            if self.mode == Mode::Attr {
                self.layout_attr(false);
            }
        }
    }
}

/// The scroll bar handle's width (`N_TransRoot_00` of `Edit_SlideAreaBtnH_00`) [obs data].
const HANDLE: f32 = 110.0;
/// The vertical bar's handle height (`N_TransRoot_00` of `Edit_SlideAreaBtnV_00`) [obs data].
const HANDLE_V: f32 = 41.6;

/// Why an upload went nowhere: uploads go to the own course server (the host sends them there
/// whichever Course World is shown); TGRCode only reads.
pub const UPLOAD_NEEDS_SERVER: &str = "Uploading needs your own course server (TGRCode is read-only): set it in Settings or with --server.";

/// Ticks after a pointer press within which a tap is the press's own.
const TAP_AFTER: i32 = 30;

/// Holding the test-play key this long plays from the start (the clapperboard's HoldWait 70)
/// [obs data].
const HOLD_START: i32 = 70;

pub const STYLES: [&str; 5] = ["M1", "M3", "MW", "WU", "3W"];
/// Theme names of the `Lyt_E_Scene*` pictures, by theme.
const THEME_TEXTURES: [&str; 10] = [
    "plain",
    "underground",
    "castle",
    "airship",
    "water",
    "hauntedhouse",
    "snow",
    "desert",
    "athletic",
    "woods",
];

/// `SceneName_*` keys by theme.
const SCENE_KEYS: [&str; 10] = [
    "Plain",
    "UnderGround",
    "Castle",
    "AirShip",
    "Water",
    "HauntedHouse",
    "Snow",
    "Desert",
    "Athletic",
    "Wood",
];

/// The pane of palette slot `i` (left to right).
pub fn slot_part(i: usize) -> String {
    format!("L_PaletteBtn_{:02}", SLOTS - 1 - i.min(SLOTS - 1))
}

/// A new course: `style`, Ground, 240 tiles wide, one row of ground [inf].
pub fn new_course(style: &str) -> Editor {
    let mut e = Editor::new(style);
    let (w, _) = e.size();
    for x in 0..w {
        if !e.protected(x, 0) {
            let _ = e.paint_ground(x, 0);
        }
    }
    e
}

/// The draft kept by [`MakerFlow::save_draft`].
fn load_draft(host: &mut dyn Host) -> Option<(Editor, Option<String>, Option<String>)> {
    let s = host.load_value("maker.draft")?;
    let v: serde_json::Value = serde_json::from_str(&s).ok()?;
    let bytes = un_hex_rle(v["course"].as_str()?)?;
    let e = Editor::open(&bytes).ok()?;
    let entry = v["entry"].as_str().map(str::to_string);
    let based_on = v["based_on"].as_str().map(str::to_string);
    Some((e, entry, based_on))
}

pub use mmk_formats::hex_rle::{hex_rle, un_hex_rle};

/// A part picture padded to the icon panes' shape (66 × 82: `P_Icon_00`, `P_Dummy_00`
/// [obs data]), the part in the middle at most 80 % of the width, a little below the centre.
pub fn fit_icon(icon: &crate::host::Icon) -> crate::host::Icon {
    let (w, h) = (icon.width.max(1) as f32, icon.height.max(1) as f32);
    // canvas: wide enough for the part at 80 % of the width, tall enough for 66:82
    let cw = (w / 0.8).max(h / 0.8 * 66.0 / 82.0).ceil();
    let ch = (cw * 82.0 / 66.0).ceil();
    let (cw, ch) = (cw as u32, ch as u32);
    let ox = (cw - icon.width) / 2;
    let oy = ((ch - icon.height) as f32 * 0.58) as u32;
    let mut rgba = vec![0u8; (cw * ch * 4) as usize];
    for y in 0..icon.height {
        let s = (y * icon.width * 4) as usize;
        let d = (((y + oy) * cw + ox) * 4) as usize;
        let n = (icon.width * 4) as usize;
        rgba[d..d + n].copy_from_slice(&icon.rgba[s..s + n]);
    }
    crate::host::Icon {
        width: cw,
        height: ch,
        rgba,
    }
}

/// Scroll speed (world px per tick) when a drag nears the view's edge.
fn edge_push(p: [f32; 2]) -> (f32, f32) {
    let band = 60.0;
    let speed = 4.0;
    let dx = if p[0] > 640.0 - band {
        speed
    } else if p[0] < -640.0 + band {
        -speed
    } else {
        0.0
    };
    let dy = if p[1] > 360.0 - band {
        speed
    } else if p[1] < -360.0 + band {
        -speed
    } else {
        0.0
    };
    (dx, dy)
}

/// A refused edit: "Part limit reached." when a limit stopped it [obs text].
fn refuse(cx: &mut crate::Cx, ui: &Ui, e: &EditError) {
    if matches!(e, EditError::Limit(_) | EditError::Full)
        && let Some(t) = ui.message(PAGE, "L_RingMenu_00-L_Balloon_00-T_Balloon_01")
    {
        cx.toast(&t);
    }
}

/// The rectangle the cursor frame shows: the part under the cursor's, else the tile.
pub fn frame_rect(f: &MakerFlow) -> Rect {
    let (tx, ty) = f.cur;
    match f.editor.object_at(tx, ty).and_then(|i| f.editor.object(i)) {
        Some(o) => mmk_editor::parts::footprint(o),
        None => Rect::new(tx, ty, 1, 1),
    }
}

#[cfg(test)]
mod tests;
