//! Co-op making ("Co-op Making", research/specs/maker.md C16): two players edit one course on
//! one screen, each with a cursor of their own.
//!
//! - The left column's `L_MultiEditBtn_00` ("Co-op Making" / "Solo Making") opens
//!   `Edit_MultiEditWindow_00` ("Start making cooperatively?", Back / Make; Mario and Luigi, each
//!   with a sideways Joy-Con: Mario's "Menu", Erase, Multigrab and Copy, Luigi's "Select parts"
//!   and Erase) [obs data]. Make runs the system's controller set-up for exactly two players,
//!   one Joy-Con each (`State::cStartAppletShareEdit`, 0x7100e363a0: min 2, max 2, style set
//!   Handheld | JoyLeft | JoyRight; fewer than two connected and the maker stays solo,
//!   0x710103e390) [obs v3.0.3]; here the host's controllers stand in for it: player 2 is the
//!   controller [`crate::coop::assign_pads`] gives the second player.
//! - Starting: both cursors go to the middle of the view, Mario's a tile's width left of it,
//!   Luigi's right (±16 px, 0x710103e490 / the cursor's constructor 0x7100f92b90), and the
//!   undo history is emptied (0x710103e490 resets the four histories) [obs v3.0.3]. While
//!   co-op, nothing is recorded for Undodog (every history call is skipped in edit mode 2,
//!   0x710103b2f0 …) [obs v3.0.3].
//! - Player 2's cursor is the green glove `Edit_mainCursor2P` (0x7100f95ab8), pad only (the
//!   cursor of index 1 never takes the touch screen, 0x7100f98000); the attribute and contents
//!   balloons are player 1's (0x7100e90c70); player 2 picks parts with `Edit_PartsMenu_00`
//!   ([`PartsMenu`]) [obs v3.0.3].
//! - A part one cursor holds (`+0x1c0`) can't be picked up by the other (0x7100f99260) and the
//!   eraser leaves it (0x7100fb57f0) [obs v3.0.3].
//! - Test play from co-op making is the co-op test play (play mode 4 instead of 5,
//!   0x710103f540) [obs v3.0.3]: here both players play it (the host's local co-op).

use super::catalog::Tool;
use super::catalog::Tool::Part as P;
use crate::host::buttons as pad;
use crate::layout::{Layout, Ui};
use crate::menu::{Buttons, hit};
use crate::sound::Event;

pub const WINDOW: &str = "Edit_MultiEditWindow_00";
pub const WINDOW_BUTTONS: [&str; 2] = ["L_BtnL_00", "L_BtnR_00"];
/// Player 2's part menu, and the layout whose sound user (`UIEditPartsMenuPage`) has its
/// `PartsMenu2P_*` sounds [obs data].
pub const MENU: &str = "Edit_PartsMenu_00";
const MENU_SOUNDS: &str = "Edit_PartsMenuPage_00";
/// The window's texts [obs data: `LayoutMsg/Edit_MultiEditWindow_00`].
const WINDOW_TEXTS: [(&str, &str); 11] = [
    ("T_Message_00", "T_Message_00"),
    ("L_BtnL_00/T_Btn_00", "L_BtnL_00-T_Btn_00"),
    ("L_BtnR_00/T_Btn_00", "L_BtnR_00-T_Btn_00"),
    ("N_Contents_00/T_P1_00", "T_P1_00"),
    ("N_Contents_00/T_P2_00", "T_P2_00"),
    ("N_Contents_00/N_GuideM_00/T_P1Guide_00", "T_P1Guide_00"),
    ("N_Contents_00/N_GuideL_00/T_P2Guide_00", "T_P2Guide_00"),
    ("N_Contents_00/N_GuideM_00/T_Eraser_00", "T_Eraser_00"),
    (
        "N_Contents_00/N_GuideM_00/T_MultiSelect_00",
        "T_MultiSelect_00",
    ),
    ("N_Contents_00/N_GuideM_00/T_Copy_00", "T_Copy_00"),
    ("N_Contents_00/N_GuideL_00/T_Eraser_01", "T_Eraser_01"),
];

/// The window's answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowEvent {
    Make,
    Back,
}

/// "Start making cooperatively?" (`Edit_MultiEditWindow_00`, `UIEditMultiEditDialogPage`).
pub struct CoopWindow {
    ui: Ui,
    pub layout: Layout,
    /// The button with the cursor: 0 Back, 1 Make.
    pub sel: usize,
    pub tick: i32,
}

impl CoopWindow {
    pub fn new(ui: &Ui) -> Option<Self> {
        let mut layout = ui.layout(WINDOW)?;
        for (pane, label) in WINDOW_TEXTS {
            layout.text_from(pane, label);
        }
        layout.play("In", 0);
        Some(Self {
            ui: ui.clone(),
            layout,
            // (the cursor on Make [inf])
            sel: 1,
            tick: 0,
        })
    }

    pub fn step(&mut self, pressed: Buttons, click: Option<[f32; 2]>) -> Option<WindowEvent> {
        self.tick += 1;
        if let Some(p) = click {
            let panes: Vec<String> = WINDOW_BUTTONS
                .iter()
                .map(|b| format!("{b}/B_Btn_00"))
                .collect();
            self.sel = hit(&mut self.layout, self.tick, &panes, p)?;
            return Some(self.decide());
        }
        if pressed.has(Buttons::LEFT) || pressed.has(Buttons::RIGHT) {
            let k = pressed.has(Buttons::RIGHT) as usize;
            if k != self.sel {
                self.sel = k;
                self.ui
                    .sound_button(WINDOW, WINDOW_BUTTONS[k], Event::Select);
            }
        }
        if pressed.has(Buttons::BACK) {
            self.ui
                .sound_button(WINDOW, WINDOW_BUTTONS[0], Event::Decide);
            return Some(WindowEvent::Back);
        }
        pressed.has(Buttons::CONFIRM).then(|| self.decide())
    }

    fn decide(&mut self) -> WindowEvent {
        self.ui
            .sound_button(WINDOW, WINDOW_BUTTONS[self.sel], Event::Decide);
        if self.sel == 1 {
            WindowEvent::Make
        } else {
            WindowEvent::Back
        }
    }
}

/// Player 2's controller as the maker's buttons, a sideways Joy-Con's in the game: the stick
/// (d-pad) moves the cursor, A (jump) puts down and picks up, SR erases while held (run), and
/// the button the window's "Select parts" points at opens the part menu (+ / start) [inf: which
/// host button stands for which Joy-Con one].
pub fn pad_buttons(b: u32) -> Buttons {
    let mut m = Buttons::NONE;
    for (bit, x) in [
        (pad::UP, Buttons::UP),
        (pad::DOWN, Buttons::DOWN),
        (pad::LEFT, Buttons::LEFT),
        (pad::RIGHT, Buttons::RIGHT),
        (pad::JUMP, Buttons::CONFIRM),
        (pad::RUN, Buttons::BACK),
    ] {
        if b & bit != 0 {
            m = m.union(x);
        }
    }
    m
}

/// Player 2's controller this tick: buttons held and newly pressed, and the part menu's button.
#[derive(Clone, Copy, Debug, Default)]
pub struct Pad2 {
    was: u32,
}

impl Pad2 {
    pub fn step(&mut self, b: u32) -> (Buttons, Buttons, bool) {
        let fresh = b & !self.was;
        self.was = b;
        (pad_buttons(b), pad_buttons(fresh), fresh & pad::START != 0)
    }
}

// ------------------------------------------------------------ the part menu's order

/// The 2P menu's pages in the other styles: the parts' palette ids page by page (0x7101922b20:
/// 12 lists from 0x71023d822c) [obs v3.0.3].
pub const PAGES: [&[u8]; 12] = [
    &[0, 2, 1, 3, 4, 5, 6, 7],
    &[9, 8, 10, 11, 12, 13, 14, 15],
    &[16, 17, 18, 19, 20, 23, 105, 21, 22, 47],
    &[24, 25, 26, 27, 28, 29, 30],
    &[31, 32, 33, 34, 35, 36, 37, 101],
    &[39, 40, 41, 42, 43, 44, 45, 102],
    &[50, 51, 49, 106, 38, 48, 46],
    &[109, 111, 110, 107, 112, 108, 113],
    &[67, 68, 69, 70, 71, 72],
    &[55, 56, 103, 52, 53, 54, 57, 58, 59],
    &[60, 61, 62, 63, 64, 65, 66],
    &[73, 74, 75, 76, 77, 78, 80],
];

/// Super Mario 3D World's pages (0x71019228e0, from 0x71023d839c); its third page has a tenth
/// part (84) once the save data's bit 1 at +0x108 is set (0x71023d84bc instead of
/// 0x71023d84e4) [obs v3.0.3; inf: an unlock].
pub const PAGES_3W: [&[u8]; 11] = [
    &[0, 2, 1, 3, 81, 83],
    &[9, 8, 10, 11, 12, 14, 15],
    &[16, 17, 18, 19, 23, 20, 105, 21, 22],
    &[114, 115, 116, 117, 118],
    &[24, 25, 85, 28, 29, 30, 101],
    &[88, 89, 31, 87, 33, 36, 90],
    &[40, 41, 42, 43, 44, 45, 102],
    &[50, 49, 93, 91, 86, 92],
    &[68, 69, 71, 72, 96, 75, 99, 78, 95],
    &[55, 56, 94, 103, 52, 53, 54, 58, 59],
    &[73, 74, 82, 97, 119, 100, 60, 104, 98],
];

/// The first page of each of the four categories (Terrain, Items, Enemies, Gizmos):
/// 0x7102320710 / 0x7102320b90 [obs v3.0.3].
pub const CATEGORY_PAGE: [usize; 4] = [0, 2, 4, 8];
pub const CATEGORY_PAGE_3W: [usize; 4] = [0, 2, 3, 8];

fn pages(style: &str) -> &'static [&'static [u8]] {
    if style == "3W" { &PAGES_3W } else { &PAGES }
}

fn category_pages(style: &str) -> [usize; 4] {
    if style == "3W" {
        CATEGORY_PAGE_3W
    } else {
        CATEGORY_PAGE
    }
}

/// A place in the menu's order: page and index.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Place {
    pub page: usize,
    pub index: usize,
}

/// Where palette id `id` sits in the style's order (None: not in it).
pub fn find(style: &str, id: u8) -> Option<Place> {
    pages(style).iter().enumerate().find_map(|(page, p)| {
        p.iter()
            .position(|&x| x == id)
            .map(|index| Place { page, index })
    })
}

pub fn id_at(style: &str, at: Place) -> Option<u8> {
    pages(style).get(at.page)?.get(at.index).copied()
}

/// The part at a place can be put in this style (SMCraft has it and the style allows it); the
/// others are stepped over [departure: the game's tables hold no part a style lacks, SMCraft
/// lacks icicles].
pub fn usable(style: &str, at: Place) -> bool {
    id_at(style, at)
        .and_then(tool)
        .is_some_and(|t| match super::catalog::entry(t) {
            Some(e) => super::catalog::available(&e, style),
            None => mmk_editor::rules::style_allows(style, t.part()),
        })
}

/// One step left (back) or right through the parts: off a page's end onto the next page's
/// first part (the previous page's last), round from the last page to the first
/// (0x7101922cd0 / 0x7101923300) [obs v3.0.3]; parts SMCraft can't put are passed over.
pub fn step(style: &str, at: Place, right: bool) -> Place {
    let total: usize = pages(style).iter().map(|p| p.len()).sum();
    let mut p = step_raw(style, at, right);
    for _ in 0..total {
        if usable(style, p) {
            break;
        }
        p = step_raw(style, p, right);
    }
    p
}

fn step_raw(style: &str, at: Place, right: bool) -> Place {
    let ps = pages(style);
    let n = ps.len();
    let page = at.page.min(n - 1);
    if right {
        if at.index + 1 < ps[page].len() {
            Place {
                page,
                index: at.index + 1,
            }
        } else {
            Place {
                page: (page + 1) % n,
                index: 0,
            }
        }
    } else if at.index > 0 {
        Place {
            page,
            index: at.index - 1,
        }
    } else {
        let p = (page + n - 1) % n;
        Place {
            page: p,
            index: ps[p].len() - 1,
        }
    }
}

/// The category a page belongs to.
pub fn category(style: &str, page: usize) -> usize {
    let first = category_pages(style);
    (0..4).rev().find(|&c| page >= first[c]).unwrap_or(0)
}

/// The next / previous category's first part (event 0x1a / 0x1b, 0x710191e1f0) [obs v3.0.3].
pub fn change_category(style: &str, at: Place, forward: bool) -> Place {
    let c = category(style, at.page);
    let c = if forward { (c + 1) % 4 } else { (c + 3) % 4 };
    let first = Place {
        page: category_pages(style)[c],
        index: 0,
    };
    if usable(style, first) {
        first
    } else {
        step(style, first, true)
    }
}

/// The parts either side of `at` in the menu's strip: two before, the part, two after
/// (`L_Icon_00`, `_01`, `_04` in the middle, `_02`, `_03`) [obs data].
pub fn strip(style: &str, at: Place) -> [Place; 5] {
    let l1 = step(style, at, false);
    let l2 = step(style, l1, false);
    let r1 = step(style, at, true);
    let r2 = step(style, r1, true);
    [l2, l1, at, r1, r2]
}

/// The strip's panes in [`strip`]'s order.
pub const STRIP: [&str; 5] = [
    "N_All_00/N_Frame_00/N_Pos_00/N_PartsMenu_00/N_Scissor_00/L_Icon_00",
    "N_All_00/N_Frame_00/N_Pos_00/N_PartsMenu_00/N_Scissor_00/L_Icon_01",
    "N_PartsMenu_01/N_Frame_01/N_SizPos_00/N_Pos_01/N_Size_00/L_Icon_04",
    "N_All_00/N_Frame_00/N_Pos_00/N_PartsMenu_00/N_Scissor_00/L_Icon_02",
    "N_All_00/N_Frame_00/N_Pos_00/N_PartsMenu_00/N_Scissor_00/L_Icon_03",
];

/// What the part menu wants of the maker after a press.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuEvent {
    /// The strip moved (pictures and name to fill again).
    Moved,
    /// Player 2's part is now the one at the middle.
    Decide,
    /// Closed without a change.
    Cancel,
}

/// Player 2's part menu (`Edit_PartsMenu_00`, `UIEditShare2PMenuCtrl`): shut, a small picture
/// of player 2's part by the green glove (`N_PartsMenu_01`); open (`Change`), a strip of five
/// parts with the name over it, the category's dot and the sideways Joy-Con's SL / SR
/// guides; left / right roll it (`SlideL` / `SlideR`), Decide shuts it on the middle part
/// [obs data, 0x710191e1f0].
pub struct PartsMenu {
    pub layout: Option<Layout>,
    pub open: bool,
    /// The middle part's place while open; player 2's part's place when shut.
    pub at: Place,
    /// Where the menu opened (Cancel goes back there).
    from: Place,
}

impl PartsMenu {
    pub fn new(ui: &Ui) -> Self {
        let mut layout = ui.layout(MENU);
        if let Some(l) = &mut layout {
            l.hold("Change", 0.0);
        }
        Self {
            layout,
            open: false,
            at: Place::default(),
            from: Place::default(),
        }
    }

    pub fn open(&mut self, ui: &Ui, tick: i32) {
        self.open = true;
        self.from = self.at;
        ui.sound_key(MENU_SOUNDS, "PartsMenu2P_Open", &[]);
        if let Some(l) = &mut self.layout {
            l.stop("Decide");
            l.play("Change", tick);
        }
    }

    fn close(&mut self, tick: i32) {
        self.open = false;
        if let Some(l) = &mut self.layout {
            l.stop("Change");
            l.play("Decide", tick);
        }
    }

    /// One tick of the open menu with player 2's buttons: left / right step through the
    /// parts, up / down change category (the Joy-Con's SL / SR [inf: which host buttons]),
    /// confirm decides, back or the menu button again cancels.
    pub fn step(
        &mut self,
        ui: &Ui,
        style: &str,
        pressed: Buttons,
        menu_key: bool,
        tick: i32,
    ) -> Option<MenuEvent> {
        if pressed.has(Buttons::CONFIRM) {
            ui.sound_key(MENU_SOUNDS, "PartsMenu2P_Decide", &[]);
            self.close(tick);
            return Some(MenuEvent::Decide);
        }
        if pressed.has(Buttons::BACK) || menu_key {
            ui.sound_key(MENU_SOUNDS, "PartsMenu2P_Cancel", &[]);
            self.at = self.from;
            self.close(tick);
            return Some(MenuEvent::Cancel);
        }
        if pressed.has(Buttons::LEFT) || pressed.has(Buttons::RIGHT) {
            let right = pressed.has(Buttons::RIGHT);
            self.at = step(style, self.at, right);
            ui.sound_key(
                MENU_SOUNDS,
                if right {
                    "PartsMenu2P_SelectR"
                } else {
                    "PartsMenu2P_SelectL"
                },
                &[],
            );
            if let Some(l) = &mut self.layout {
                let (go, other) = if right {
                    ("SlideR", "SlideL")
                } else {
                    ("SlideL", "SlideR")
                };
                l.stop(other);
                l.play(go, tick);
            }
            return Some(MenuEvent::Moved);
        }
        if pressed.has(Buttons::UP) || pressed.has(Buttons::DOWN) {
            self.at = change_category(style, self.at, pressed.has(Buttons::DOWN));
            let c = category(style, self.at.page);
            ui.sound_key(
                MENU_SOUNDS,
                "PartsMenu2P_CtgrChange",
                &[&format!("CtgrChange_{c:02}")],
            );
            return Some(MenuEvent::Moved);
        }
        None
    }

    /// Fill the strip: the five parts' pictures (`PartIcon` keys), the middle one's name, the
    /// category's dot (`Category` frame = category) [obs data].
    pub fn fill(&mut self, style: &str, pictures: &[Option<String>; 5], name: Option<&str>) {
        let c = category(style, self.at.page);
        let Some(l) = &mut self.layout else { return };
        for (pane, picture) in STRIP.iter().zip(pictures) {
            let pic = format!("{pane}/P_Dummy_00");
            match picture {
                Some(n) => {
                    l.show(&pic, true);
                    l.image(&pic, "PartIcon", n);
                }
                None => l.show(&pic, false),
            }
        }
        l.hold("Category", c as f32);
        if let Some(n) = name {
            l.text(
                "N_All_00/N_Frame_00/N_Pos_00/N_PartsMenu_00/T_Balloon_00",
                n,
            );
        }
    }

    /// The menu by the glove at `at` (layout units), turned by `Direction` to stay on the
    /// screen (the attribute balloon's rule [inf]).
    pub fn place(&mut self, at: [f32; 2], direction: u8) {
        let Some(l) = &mut self.layout else { return };
        l.set_translate("N_All_00", [at[0] + 27.0, at[1] - 5.5]);
        l.set_translate("N_PartsMenu_01", [at[0] + 29.0, at[1] - 1.0]);
        l.hold("Direction", direction as f32);
    }
}

/// Palette id → the part as the maker puts it (`catalog`'s tools), None for the ones SMCraft
/// can't put yet (icicles keep their own array; the conveyor slope).
pub fn tool(id: u8) -> Option<Tool> {
    PALETTE.get(id as usize).and_then(|&(_, t, _)| t)
}

/// The palette id's `PartsIconName` label (`<label>`, `<label>_00` or `<style>_<label>`).
pub fn label(id: u8) -> Option<&'static str> {
    PALETTE.get(id as usize).map(|&(_, _, l)| l)
}

/// The palette ids (0–119), their parts and names: the editor's name table (24-byte entries
/// from 0x7102cd5e08, `_INIT_1888` 0x7100be56e0: "Ground", "slope_l30", "slope_l45", "Dokan",
/// …) [obs v3.0.3]; the parts are matched to SMCraft's catalog by name [inf for the ones the
/// catalog names otherwise: Met → 28, MagmaFish (Charvaargh) → 86, the 3D World semisolid
/// GroundBox 83 → 71].
pub const PALETTE: [(u8, Option<Tool>, &str); 120] = [
    (0, Some(Tool::Ground), "Ground"),
    (1, Some(P(87)), "slope_l30"),
    (2, Some(P(88)), "slope_l45"),
    (3, Some(Tool::Pipe), "Dokan"),
    (4, Some(P(43)), "Toge"),
    (5, Some(P(14)), "GroundMushroom"),
    (6, Some(P(16)), "GroundBox"),
    (7, Some(P(17)), "Bridge"),
    (8, Some(P(5)), "HatenaBlock"),
    (9, Some(P(4)), "RengaBlock"),
    (10, Some(P(6)), "HardBlock"),
    (11, Some(P(29)), "ClearBlock"),
    (12, Some(P(21)), "ChikuwaBlock"),
    (13, Some(P(23)), "OnpuBlock"),
    (14, Some(P(22)), "KumoBlock"),
    (15, Some(P(63)), "IceBlock"),
    (16, Some(P(8)), "Coin"),
    (17, Some(P(70)), "10Coin"),
    (18, Some(P(92)), "PinkCoin"),
    (19, Some(P(20)), "SuperKinoko"),
    (20, Some(P(34)), "FireFlower"),
    (21, Some(P(35)), "SuperStar"),
    (22, Some(P(33)), "1upKinoko"),
    (23, Some(P(44)), "SpecialItem"),
    (24, Some(P(0)), "Kuribo"),
    (25, Some(P(1)), "Nokonoko"),
    (26, Some(P(28)), "Met"),
    (27, Some(P(40)), "TogeMet"),
    (28, Some(P(25)), "Togezo"),
    (29, Some(P(48)), "Gesso"),
    (30, Some(P(56)), "Pukupuku"),
    (31, Some(P(2)), "Pakkun"),
    (32, Some(P(57)), "BlackPakkun"),
    (33, Some(P(12)), "Dossun"),
    (34, Some(P(102)), "ChoroPoo"),
    (35, Some(P(58)), "Poo"),
    (36, Some(P(3)), "Bros"),
    (37, Some(P(61)), "Wanwan"),
    (38, Some(P(104)), "SunMoon"),
    (39, Some(P(52)), "Hanachan"),
    (40, Some(P(41)), "Teresa"),
    (41, Some(P(60)), "Bubble"),
    (42, Some(P(15)), "Bombhei"),
    (43, Some(P(46)), "Karon"),
    (44, Some(P(103)), "FishBone"),
    (45, Some(P(39)), "Kameck"),
    (46, Some(P(42)), "KoopaClown"),
    (47, Some(P(45)), "KutsuKuribo"),
    (48, Some(P(30)), "Jugem"),
    (49, Some(P(77)), "Bunbun"),
    (50, Some(P(62)), "Koopa"),
    (51, Some(P(98)), "KoopaJr"),
    (52, Some(P(18)), "PSwitch"),
    (53, Some(P(19)), "PowBlock"),
    (54, Some(P(10)), "JumpStep"),
    (55, Some(P(95)), "Key"),
    (56, Some(Tool::Doors), "Door"),
    (57, Some(P(64)), "TsutaBlock"),
    (58, Some(P(66)), "Yajirushi"),
    (59, Some(P(90)), "MiddleFlag"),
    (60, Some(P(11)), "Lift"),
    (61, Some(P(36)), "YouganLift"),
    (62, Some(P(91)), "Seesaw"),
    (63, Some(P(68)), "Saw"),
    (64, Some(P(82)), "Marumaru"),
    (65, Some(P(83)), "TogeKonbo"),
    (66, Some(P(105)), "BurankoCrane"),
    (67, Some(P(54)), "Burner"),
    (68, Some(P(13)), "KillerHoudai"),
    (69, Some(P(32)), "MagnumKiller"),
    (70, Some(P(47)), "Houdai"),
    (71, None, "Icicle"),
    (72, Some(P(76)), "Tornado"),
    (73, Some(P(99)), "OnOffSwitch"),
    (74, Some(P(100)), "OnOffBlock"),
    (75, Some(P(84)), "SnakeBlock"),
    (76, Some(P(24)), "FireBar"),
    (77, Some(P(67)), "Hanatari"),
    (78, Some(P(53)), "BeltConveyor"),
    (79, None, "SlopeConveyor"),
    (80, Some(Tool::Track), "Rail"),
    (81, Some(Tool::ClearPipe), "ToumeiDokan"),
    (82, Some(P(110)), "TogeBlock"),
    (83, Some(P(71)), "GroundBox"),
    (84, Some(P(116)), "SuperHammer"),
    (85, Some(P(96)), "Arihei"),
    (86, Some(P(114)), "Fugumannen"),
    (87, Some(P(107)), "PackunPipe"),
    (88, Some(P(51)), "Pyonchu"),
    (89, Some(P(65)), "Hacchin"),
    (90, Some(P(50)), "Hopper"),
    (91, Some(P(117)), "Donketsu"),
    (92, Some(P(72)), "KoopaCar"),
    (93, Some(P(86)), "MagmaFish"),
    (94, Some(Tool::WarpBoxes), "WarpBox"),
    (95, Some(P(112)), "WoodBox"),
    (96, Some(P(119)), "BikkuriBlock"),
    (97, Some(P(85)), "OrbitBlock"),
    (98, Some(P(108)), "Chikachika"),
    (99, Some(P(106)), "BellTree"),
    (100, Some(P(113)), "Trampoline"),
    (101, Some(P(74)), "Gabon"),
    (102, Some(P(78)), "Sambo"),
    (103, Some(P(79)), "PBlock"),
    (104, Some(P(80)), "DashBlock"),
    (105, Some(P(81)), "SpecialItemExtra"),
    (106, Some(P(111)), "KoopaMecha"),
    (107, Some(P(120)), "Lemmy"),
    (108, Some(P(121)), "Morton"),
    (109, Some(P(122)), "Larry"),
    (110, Some(P(123)), "Wendy"),
    (111, Some(P(124)), "Iggy"),
    (112, Some(P(125)), "Roy"),
    (113, Some(P(126)), "Ludwig"),
    (114, Some(P(127)), "BoxKiller"),
    (115, Some(P(128)), "BoxPropeller"),
    (116, Some(P(129)), "BoxKuribo"),
    (117, Some(P(130)), "BoxKillerPlayer"),
    (118, Some(P(131)), "BoxPow"),
    (119, Some(P(132)), "OnOffTrampoline"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_run_across_pages_and_round() {
        let s = "M1";
        assert_eq!(
            step(s, Place { page: 0, index: 7 }, true),
            Place { page: 1, index: 0 }
        );
        assert_eq!(
            step(s, Place { page: 1, index: 0 }, false),
            Place { page: 0, index: 7 }
        );
        assert_eq!(
            step(s, Place { page: 0, index: 0 }, false),
            Place { page: 11, index: 6 }
        );
        assert_eq!(
            step(s, Place { page: 11, index: 6 }, true),
            Place { page: 0, index: 0 }
        );
        assert_eq!(
            step("3W", Place { page: 10, index: 8 }, true),
            Place::default()
        );
    }

    #[test]
    fn categories_start_on_their_first_page() {
        assert_eq!(category("M1", 3), 1);
        assert_eq!(category("M1", 7), 2);
        assert_eq!(category("M1", 11), 3);
        assert_eq!(category("3W", 3), 2);
        let at = Place { page: 5, index: 3 };
        assert_eq!(change_category("M1", at, true), Place { page: 8, index: 0 });
        assert_eq!(
            change_category("M1", at, false),
            Place { page: 2, index: 0 }
        );
        assert_eq!(
            change_category("M1", Place { page: 9, index: 1 }, true),
            Place::default()
        );
    }

    #[test]
    fn the_strip_shows_two_either_side() {
        let s = strip("M1", Place::default());
        assert_eq!(s[2], Place::default());
        assert_eq!(s[1], Place { page: 11, index: 6 });
        assert_eq!(s[0], Place { page: 11, index: 5 });
        assert_eq!(s[4], Place { page: 0, index: 2 });
        assert_eq!(find("M1", 105), Some(Place { page: 2, index: 6 }));
        assert_eq!(id_at("3W", Place { page: 3, index: 0 }), Some(114));
    }

    #[test]
    fn player_twos_buttons() {
        let mut p = Pad2::default();
        let (held, pressed, menu) = p.step(pad::JUMP | pad::LEFT);
        assert!(held.has(Buttons::CONFIRM) && pressed.has(Buttons::LEFT) && !menu);
        let (held, pressed, menu) = p.step(pad::JUMP | pad::START);
        assert!(held.has(Buttons::CONFIRM) && !pressed.has(Buttons::CONFIRM) && menu);
        assert!(pad_buttons(pad::RUN).has(Buttons::BACK));
    }
}
