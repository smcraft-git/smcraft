//! Network Play (G6) with SMM2's own screens, over Nextendo Network's matchmaking and SMCraft's
//! rollback netcode, which the host provides as a [`Network`] ([`crate::Host::network`]):
//! - the mode select `Nwk_MatchingModeSelect_00` over `Nwk_MatchingBG_00` [obs layout]:
//!   Multiplayer Co-op, Multiplayer Versus; Play with Friends, Nearby Play, Nearby Play Help and
//!   Message Settings say "not yet"; the Nextendo account (sign in / out) sits in Standings's
//!   place [departure: SMCraft has no console account];
//! - the wait screen `Nwk_MatchingWait_00`: "Finding players for Multiplayer Co-op / Versus",
//!   the players found so far ("n/m"), Quit (cancels the search) [obs: InternetMatching];
//! - the lobby `Nwk_Matching_00`: "Meet your team…" / "Meet the competition…", a card per player
//!   (`Nwk_MatchingPlayer_00`; the character by position, as DecidePosition: Mario, Luigi, Toad,
//!   Toadette) for the lobby's beat of three seconds after its In [obs: MatchResult's 3 × 60];
//! - "Getting course data from the server..." while the host loads the course and starts the
//!   rollback session, then the course: the HUD, the 3-2-1-GO countdown (`Play_CountDownStart_00`;
//!   the course waits for it), the multiplayer menu `Play_BattleMenu_00` on START (Give Up; the
//!   course goes on underneath: the other players can't wait);
//! - the clear (`Play_CourseClear_00`, the network banner; versus: the win / lose banner) and the
//!   result `Play_BattleResult_00`: Exit (back to the mode select) / Continue playing (a new
//!   search, as the ReMatching states).
//!
//! The course [departure]: no difficulty vote. With Course World courses at hand (TGRCode's
//! Endless pool the host keeps, [`crate::Host::challenge_courses`]) every player searches with a
//! pick of their own, and the pick of whoever opens the session is everyone's course
//! ([`MatchInfo::course`]; the others download it from TGRCode). Without, every player draws the
//! same course from the pool all SMCraft players share — the Story Mode courses of their import
//! ([`course_pool`]) — by the session's gathering ID ([`match_course`]). The pool's fingerprint
//! ([`pool_id`], or [`WORLD_POOL`]) goes into the session's attributes, so players whose pools
//! differ never meet.
//!
//! A peer that stops sending ([`LinkState::Lost`]) ends the match with a message, as the game's
//! EndBattleDisconnected; giving up leaves the match. Spec: research `specs/network-play.md`.

use crate::fade::FadeKind;
use crate::host::{CourseEntry, CourseStatus, LoadedCourse, Source};
use crate::layout::{Layout, Ui};
use crate::menu::{Buttons, MenuCursor, Script, UiInput, hit, nearest};
use crate::play::PlayInput;
use crate::screens::{CardInfo, CourseScreens, digits};
use crate::sound::Event;
use crate::{Cx, DrawItem, Go, push_layout};

/// The account Network Play signs in with (Nextendo's, or a local test server's).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NetAccount {
    /// Signed in: the account's name.
    SignedIn(String),
    /// The sign-in is under way (e.g. the browser is open).
    SigningIn,
    SignedOut,
    /// The last sign-in failed (why).
    Failed(String),
}

/// What to search for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MatchRequest {
    pub versus: bool,
    /// Players in the match (2–4).
    pub players: usize,
    /// The course pool's fingerprint ([`pool_id`], [`WORLD_POOL`]): only players with the same
    /// pool meet.
    pub pool: u64,
    /// The Course World course (its code) everyone plays if we open the session.
    pub pick: Option<String>,
}

/// The pool fingerprint of matches on Course World courses picked by the session's opener.
pub const WORLD_POOL: u64 = u64::from_le_bytes(*b"TGRCode1");

/// A match found: everyone is connected.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MatchInfo {
    /// Our position (0 = the session's owner) and the number of players.
    pub me: usize,
    pub players: usize,
    /// The session's gathering ID, the same for every player.
    pub gid: u32,
    /// The Course World course the session's opener picked (None: the shared pool's draw).
    pub course: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MatchState {
    Idle,
    /// Searching: players found so far (us included) and players wanted.
    Searching {
        found: usize,
        wanted: usize,
    },
    Ready(MatchInfo),
    Failed(String),
}

/// The other players during a match.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkState {
    Up,
    /// A peer left or stopped sending: the match is over.
    Lost,
}

/// Network Play on the host's side: the account, the search (on a thread; polled every tick),
/// and the match's course (loaded into the engine, which then runs the rollback session).
pub trait Network {
    fn account(&mut self) -> NetAccount;
    fn sign_in(&mut self);
    fn sign_out(&mut self);
    /// Start searching (a search already running is cancelled first).
    fn find_match(&mut self, request: &MatchRequest);
    /// Give the search up (it leaves its session).
    fn cancel_match(&mut self);
    fn poll_match(&mut self) -> MatchState;
    /// Load the match's course and play it online with the other players from the next
    /// [`crate::Host::step_course`] (the player's character by position).
    fn play_match(&mut self, course: &Source) -> Result<LoadedCourse, String>;
    fn link(&mut self) -> LinkState;
    /// Leave the match (the other players see us go).
    fn leave_match(&mut self);
}

/// The courses every SMCraft player has: the import's Story Mode courses the engine plays, in
/// content-key order.
pub fn course_pool(library: &[CourseEntry]) -> Vec<CourseEntry> {
    let mut pool: Vec<CourseEntry> = library
        .iter()
        .filter(|e| e.built_in && e.playable && e.collection.as_deref() == Some("Story Mode"))
        .cloned()
        .collect();
    pool.sort_by(|a, b| a.key.cmp(&b.key));
    pool.dedup_by(|a, b| a.key == b.key);
    pool
}

/// The pool's fingerprint (FNV-1a over its keys), for the session's attributes.
pub fn pool_id(pool: &[CourseEntry]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for e in pool {
        for b in e.key.bytes().chain(std::iter::once(b'\n')) {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x100_0000_01b3);
        }
    }
    h
}

/// The match's course: the same pick from the same pool for every player of gathering `gid`.
pub fn match_course(pool: &[CourseEntry], gid: u32) -> Option<&CourseEntry> {
    if pool.is_empty() {
        return None;
    }
    let mixed = u64::from(gid).wrapping_mul(0x9e37_79b9_7f4a_7c15) >> 17;
    pool.get((mixed % pool.len() as u64) as usize)
}

/// What a step tells the owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NetplayEvent {
    /// Back to Course World.
    Exit,
}

/// Deferred changes (run behind the black fade).
#[derive(Clone, Debug, PartialEq)]
pub enum NetGo {
    /// Into the course.
    Play,
    /// Back to the mode select (after a match, or when it failed).
    Modes,
    /// A new search after a result (Continue playing).
    Again,
}

const BG: &str = "Nwk_MatchingBG_00";
const MODES: &str = "Nwk_MatchingModeSelect_00";
const WAIT: &str = "Nwk_MatchingWait_00";
const LOBBY: &str = "Nwk_Matching_00";
const COUNTDOWN: &str = "Play_CountDownStart_00";
const MENU: &str = "Play_BattleMenu_00";
const CLEAR: &str = "Play_CourseClear_00";
const RESULT: &str = "Play_BattleResult_00";

/// Players per match [inf: SMM2's are four; two until 3–4 player matches are tested across
/// NATs].
pub const PLAYERS: usize = 2;
/// The lobby stays this long after its In [obs: MatchResult's 3 × 60-frame timer].
const LOBBY_BEAT: i32 = 180;
/// "Getting course data…" is shown at least this long [inf].
const LOADING_TICKS: i32 = 30;
/// The countdown's numbers (60 frames each) before GO! [obs layout: Number3/2/1, Start].
const COUNT_TICKS: i32 = 60;
/// The clear banner before the result [inf].
const CLEAR_TICKS: i32 = 240;

/// The mode select's buttons, in the layout's reading order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModeChoice {
    Coop,
    Versus,
    Friends,
    Nearby,
    Account,
    Help,
    Messages,
}

const MODE_BUTTONS: [(&str, ModeChoice); 7] = [
    ("L_NetBtn_01", ModeChoice::Coop),
    ("L_NetBtn_00", ModeChoice::Versus),
    ("L_FriendBtn_00", ModeChoice::Friends),
    ("L_LocalBtn_00", ModeChoice::Nearby),
    ("L_BtlResultBtn_00", ModeChoice::Account),
    ("L_HelpBtn_00", ModeChoice::Help),
    ("L_MessageSelectBtn_00", ModeChoice::Messages),
];

fn bound(part: &str) -> String {
    format!("{part}/B_Btn_00")
}

/// A row of the game's buttons (`<part>/B_Btn_00`, Select / Decide) with the cursor frame.
struct ButtonSet {
    layout_name: &'static str,
    parts: Vec<&'static str>,
    selection: usize,
    /// A decided button: its index and the tick its Decide ends.
    pending: Option<(usize, i32)>,
}

enum ButtonEvent {
    Decided(usize),
    Back,
}

impl ButtonSet {
    fn new(layout_name: &'static str, parts: Vec<&'static str>, l: &mut Layout, at: usize) -> Self {
        let mut s = Self {
            layout_name,
            parts,
            selection: at,
            pending: None,
        };
        s.select(l, at, 0, false);
        s
    }

    fn select(&mut self, l: &mut Layout, i: usize, tick: i32, animate: bool) {
        for (k, p) in self.parts.iter().enumerate() {
            if k != i {
                l.hold(&format!("{p}/Select"), 0.0);
            }
        }
        self.selection = i;
        let a = format!("{}/Select", self.parts[i]);
        if animate {
            l.play(&a, tick);
        } else {
            l.hold_end(&a);
        }
    }

    fn step(
        &mut self,
        ui: &Ui,
        l: &mut Layout,
        mut pressed: Buttons,
        click: Option<[f32; 2]>,
        tick: i32,
    ) -> Option<ButtonEvent> {
        if let Some((i, at)) = self.pending {
            if tick >= at {
                self.pending = None;
                return Some(ButtonEvent::Decided(i));
            }
            return None;
        }
        if let Some(p) = click {
            let panes: Vec<String> = self.parts.iter().map(|p| bound(p)).collect();
            if let Some(i) = hit(l, tick, &panes, p) {
                self.select(l, i, tick, false);
                pressed = pressed | Buttons::CONFIRM;
            }
        }
        if let Some(dir) = pressed.direction() {
            l.apply(tick);
            let here = l
                .rect(&bound(self.parts[self.selection]))
                .map_or([0.0, 0.0], |r| r.center);
            let targets: Vec<(usize, [f32; 2])> = self
                .parts
                .iter()
                .enumerate()
                .filter_map(|(i, p)| {
                    l.rect(&bound(p))
                        .filter(|r| r.visible)
                        .map(|r| (i, r.center))
                })
                .collect();
            if let Some(n) = nearest(here, dir, &targets) {
                ui.sound_button(self.layout_name, self.parts[n], Event::Select);
                self.select(l, n, tick, true);
            }
        } else if pressed.has(Buttons::CONFIRM) {
            let p = self.parts[self.selection];
            ui.sound_button(self.layout_name, p, Event::Decide);
            let a = format!("{p}/Decide");
            l.play(&a, tick);
            self.pending = Some((self.selection, tick + l.ticks(&a, 1).min(12)));
        } else if pressed.has(Buttons::BACK) {
            ui.sound_back(self.layout_name, "normal");
            return Some(ButtonEvent::Back);
        }
        None
    }

    fn place_cursor(&self, cursor: &mut MenuCursor, l: &mut Layout, tick: i32) {
        let alpha = if self.pending.is_none() { 1.0 } else { 0.0 };
        cursor.place(l, &bound(self.parts[self.selection]), tick, alpha);
    }
}

/// The mode select.
struct Modes {
    layout: Layout,
    buttons: ButtonSet,
}

impl Modes {
    fn new(ui: &Ui, account: &NetAccount, at: usize, tick: i32) -> Option<Self> {
        let mut l = ui.layout(MODES)?;
        l.play("In", tick);
        // no versus rank yet: the plain button [departure]
        l.hold("L_NetBtn_00/TypeRank", 0.0);
        let buttons = ButtonSet::new(
            MODES,
            MODE_BUTTONS.iter().map(|b| b.0).collect(),
            &mut l,
            at,
        );
        let mut m = Self { layout: l, buttons };
        m.set_account(account);
        Some(m)
    }

    /// The account button's label (our own words: the game has no such button) [departure].
    fn set_account(&mut self, account: &NetAccount) {
        let text = match account {
            NetAccount::SignedIn(name) => format!("{name} (sign out)"),
            NetAccount::SigningIn => "Signing in…".to_string(),
            NetAccount::SignedOut | NetAccount::Failed(_) => "Sign in to Nextendo".to_string(),
        };
        self.layout.text("L_BtlResultBtn_00/T_Btn_00", &text);
    }
}

/// What the wait screen waits for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WaitFor {
    /// The search (Quit cancels it).
    Players,
    /// The course: "Getting course data from the server...".
    Course,
}

struct Wait {
    layout: Layout,
    what: WaitFor,
    since: i32,
    found: usize,
    wanted: usize,
}

impl Wait {
    fn new(ui: &Ui, what: WaitFor, versus: bool, tick: i32) -> Option<Self> {
        let mut l = ui.layout(WAIT)?;
        l.play("In", tick);
        l.play_loop("L_LoacingIcon_00/Loop", tick);
        l.show("L_LoacingIcon_00/N_NoData_00", false);
        l.show("L_LoacingIcon_00/T_LoadText_00", false);
        l.show("L_Signal_00", false);
        l.show("N_CountDownl_00", false);
        let text = match what {
            WaitFor::Players if versus => "0007",
            WaitFor::Players => "0008",
            WaitFor::Course => "0005",
        };
        l.text_from("T_Text_00", text);
        // Quit while searching; nothing to quit once the match is made
        l.show("L_DecideBtn_00", what == WaitFor::Players);
        l.show("N_Num_00", false);
        Some(Self {
            layout: l,
            what,
            since: tick,
            found: 0,
            wanted: 0,
        })
    }

    fn set_count(&mut self, found: usize, wanted: usize) {
        if (found, wanted) == (self.found, self.wanted) {
            return;
        }
        self.found = found;
        self.wanted = wanted;
        let text = self
            .layout
            .format("0006", &[found.to_string(), wanted.to_string()])
            .unwrap_or_else(|| format!("{found}/{wanted}"));
        self.layout.text("N_Num_00/T_MatchingNum_00", &text);
        self.layout.text("T_MatchingNum_00", &text);
        self.layout.visible("N_Num_00", Some(found > 0));
    }
}

/// The lobby.
struct Lobby {
    layout: Layout,
    since: i32,
}

/// The local player's name and the others' (peers' names aren't exchanged yet: "Player n").
fn player_name(me: usize, k: usize, name: &str) -> String {
    if k == me {
        name.to_string()
    } else {
        format!("Player {}", k + 1)
    }
}

impl Lobby {
    fn new(ui: &Ui, versus: bool, m: &MatchInfo, name: &str, tick: i32) -> Option<Self> {
        let mut l = ui.layout(LOBBY)?;
        l.play("In", tick);
        l.text_from(
            "T_Matching_00",
            if versus {
                "T_Matching_InetBattleMode"
            } else {
                "T_Matching_InetMultiClear"
            },
        );
        // the rule's icon: frame 0 versus, 1 co-op [obs: the frames' textures], left of the
        // line [inf: the game sizes N_Align_00 to the text]
        l.hold("TypeRule", if versus { 0.0 } else { 1.0 });
        if let (Some(w), Some(p)) = (l.text_width("T_Matching_00"), l.pane("P_RuleIcon_00")) {
            l.set_translate("P_RuleIcon_00", [-w / 2.0 - 48.0, p.translate[1]]);
        }
        // four cards (frame 0), three (1) or two (2) [obs: the frames hide cards 4 and 3]
        l.hold("TypeMember", (4 - m.players.clamp(2, 4)) as f32);
        for k in 0..4 {
            let card = format!("L_Player_{k:02}");
            if k >= m.players {
                l.show(&card, false);
                continue;
            }
            l.hold(&format!("{card}/TypeCharacter"), k as f32);
            l.hold(
                &format!("{card}/TypeBattleMode"),
                if versus { 1.0 } else { 0.0 },
            );
            l.hold(
                &format!("{card}/TypePlayer"),
                if k == m.me { 1.0 } else { 0.0 },
            );
            // no Miis, no rank yet [departure]
            l.show(&format!("{card}/L_MiiBody_00"), false);
            l.show(&format!("{card}/W_Rate_00"), false);
            l.text(
                &format!("{card}/T_PlayerName_00"),
                &player_name(m.me, k, name),
            );
            if let Some(c) = l.message(&format!("{:04}", k + 1)) {
                l.text(&format!("{card}/T_Chara_00"), &c);
            }
        }
        Some(Self {
            layout: l,
            since: tick,
        })
    }

    fn done(&mut self, tick: i32) -> bool {
        tick - self.since >= self.layout.ticks("In", 1) + LOBBY_BEAT
    }
}

/// The course of the match.
struct NetCourse {
    screens: Option<CourseScreens>,
    countdown: Option<Layout>,
    menu: Option<Layout>,
    menu_open: bool,
    clear: Option<Layout>,
    since: i32,
    cleared_at: Option<i32>,
    /// Versus: the winner's position.
    winner: Option<usize>,
    /// START was held last tick (a press opens or closes the menu).
    held_start: bool,
}

impl NetCourse {
    fn new(ui: &Ui, name: &str, m: &MatchInfo, player: &str, status: &CourseStatus) -> Self {
        let mut screens = CourseScreens::new(ui);
        if let Some(s) = &mut screens {
            s.info = CardInfo {
                name: name.to_string(),
                built_in: true,
                mode_label: Some("T_Mode_Network".into()),
                ..Default::default()
            };
            s.apply_rules();
            s.update(status);
            s.start_playing();
        }
        let mut countdown = ui.layout(COUNTDOWN);
        if let Some(c) = &mut countdown {
            c.show("N_Guide_00", false);
            c.show("N_Start_00", false);
            c.hold("Number3", 0.0);
        }
        let mut menu = ui.layout(MENU);
        if let Some(l) = &mut menu {
            l.text_from("L_Btn_00/T_Btn_00", "0000");
            for p in ["L_Btn_01", "L_RestartBtn_00", "N_Mission_00"] {
                l.show(p, false);
            }
            l.hold("TypeRetire", 0.0);
            for k in 0..4 {
                let info = format!("L_Info_{k:02}");
                if k >= m.players {
                    l.show(&info, false);
                    continue;
                }
                l.text(
                    &format!("{info}/T_MiiName_00"),
                    &player_name(m.me, k, player),
                );
                l.show(&format!("{info}/T_Rank_00"), false);
                l.show(&format!("{info}/L_MiiIcon_00"), false);
                l.show(&format!("{info}/L_MiiIcon_01"), false);
                l.hold(&format!("{info}/L_Chara_00/TypeCharacter"), k as f32);
            }
        }
        Self {
            screens,
            countdown,
            menu,
            menu_open: false,
            clear: None,
            since: 0,
            cleared_at: None,
            winner: None,
            held_start: true,
        }
    }

    /// Ticks since the course came up (the countdown runs first).
    fn countdown_left(&self, tick: i32) -> i32 {
        3 * COUNT_TICKS - (tick - self.since)
    }
}

/// The result.
struct Results {
    layout: Layout,
    buttons: ButtonSet,
    /// The course dimmed under it [inf: as the pause menu's veil].
    veil: Option<Layout>,
}

impl Results {
    fn new(
        ui: &Ui,
        versus: bool,
        m: &MatchInfo,
        winner: Option<usize>,
        name: &str,
        tick: i32,
    ) -> Option<Self> {
        let mut l = ui.layout(RESULT)?;
        // no rating yet [departure]
        l.show("N_Rate_00", false);
        l.show("L_Cap_00", false);
        l.hold("TypeMember", (m.players.clamp(1, 4) - 1) as f32);
        // versus: the winner first [inf]
        let mut order: Vec<usize> = (0..m.players).collect();
        if versus && let Some(w) = winner {
            order.retain(|&k| k != w);
            order.insert(0, w);
        }
        for slot in 0..4 {
            let card = format!("L_ResultPlayer_{slot:02}");
            let Some(&k) = order.get(slot) else {
                l.show(&card, false);
                continue;
            };
            l.visible(&card, Some(true));
            l.hold(&format!("{card}/TypePlayer"), k as f32);
            l.hold(
                &format!("{card}/TypeOwn"),
                if k == m.me { 1.0 } else { 0.0 },
            );
            l.hold(
                &format!("{card}/TypeBattleMode"),
                if versus { 1.0 } else { 0.0 },
            );
            l.show(&format!("{card}/L_MiiBody_00"), false);
            l.show(&format!("{card}/L_BtlPayerInfo_00"), false);
            l.text(
                &format!("{card}/N_PlayerName_00/T_PlayerName_00"),
                &player_name(m.me, k, name),
            );
            l.text(
                &format!("{card}/T_PlayerName_00"),
                &player_name(m.me, k, name),
            );
        }
        l.play("InPlayer", tick);
        let next = tick + l.ticks("InPlayer", 1);
        l.play("InNext", next);
        // Continue playing first [inf: the game's focus after a match]
        let buttons = ButtonSet::new(RESULT, vec!["L_NextBtn_00", "L_ContinueBtn_00"], &mut l, 1);
        let mut veil = ui.layout("Cmn_Fade_00");
        if let Some(f) = &mut veil {
            f.hold_end("FadeOutB");
            for p in ["P_FadeW_00", "N_Pos_00", "N_Block_00", "N_CaptureFade_00"] {
                f.show(p, false);
            }
            f.set_alpha("P_FadeB_00", 160.0);
        }
        Some(Self {
            layout: l,
            buttons,
            veil,
        })
    }
}

enum Stage {
    Modes(Box<Modes>),
    Wait(Box<Wait>),
    Lobby(Box<Lobby>),
    Playing(Box<NetCourse>),
    Result(Box<Results>),
    /// Behind a fade, between two stages.
    Blank,
}

/// The whole Network Play flow, from the mode select to the result.
pub struct NetplayFlow {
    ui: Ui,
    bg: Option<Layout>,
    stage: Stage,
    pub tick: i32,
    cursor: MenuCursor,
    /// Captures: menu presses at global UI ticks.
    pub script: Script,
    pool: Vec<CourseEntry>,
    versus: bool,
    mode_selection: usize,
    current: Option<MatchInfo>,
    course_name: String,
    name: String,
    leaving: bool,
}

impl NetplayFlow {
    /// The mode select. `library`: the host's courses (the pool is drawn from them).
    pub fn new(ui: &Ui, library: &[CourseEntry], cx: &mut Cx) -> Option<Self> {
        let account = cx
            .host
            .network()
            .map_or(NetAccount::SignedOut, |n| n.account());
        let mut bg = ui.layout(BG);
        if let Some(b) = &mut bg {
            b.play("In", 0);
        }
        let modes = Modes::new(ui, &account, 0, 0)?;
        Some(Self {
            ui: ui.clone(),
            bg,
            stage: Stage::Modes(Box::new(modes)),
            tick: 0,
            cursor: MenuCursor::new(ui),
            script: Script::default(),
            pool: course_pool(library),
            versus: false,
            mode_selection: 0,
            current: None,
            course_name: String::new(),
            name: account_name(&account),
            leaving: false,
        })
    }

    fn show_modes(&mut self, cx: &mut Cx) {
        let account = cx
            .host
            .network()
            .map_or(NetAccount::SignedOut, |n| n.account());
        self.name = account_name(&account);
        if let Some(m) = Modes::new(&self.ui, &account, self.mode_selection, self.tick) {
            self.stage = Stage::Modes(Box::new(m));
        }
        if let Some(b) = &mut self.bg {
            b.play("In", self.tick);
        }
    }

    /// Start the search for `versus` (signed in, with a pool).
    fn search(&mut self, cx: &mut Cx, versus: bool) {
        self.versus = versus;
        let picks: Vec<String> = cx
            .host
            .challenge_courses()
            .into_iter()
            .filter_map(|e| e.meta.code)
            .collect();
        let (pool, pick) = if picks.is_empty() {
            if self.pool.is_empty() {
                cx.toast(
                    "Network Play needs Course World courses or the Story Mode courses of your import.",
                );
                return;
            }
            (pool_id(&self.pool), None)
        } else {
            let k = (self.tick as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15) >> 17;
            (
                WORLD_POOL,
                Some(picks[(k % picks.len() as u64) as usize].clone()),
            )
        };
        let Some(n) = cx.host.network() else {
            cx.toast("Network Play isn't set up: it needs a Nextendo game server.");
            return;
        };
        match n.account() {
            NetAccount::SignedIn(name) => self.name = name,
            NetAccount::SigningIn => {
                cx.toast("Finish signing in to Nextendo first.");
                return;
            }
            _ => {
                cx.toast("Sign in to Nextendo first (the button at the bottom right).");
                return;
            }
        }
        n.find_match(&MatchRequest {
            versus,
            players: PLAYERS,
            pool,
            pick,
        });
        if let Some(w) = Wait::new(&self.ui, WaitFor::Players, versus, self.tick) {
            self.stage = Stage::Wait(Box::new(w));
        }
    }

    /// The match is over for us: leave it, back to the mode select (with a message).
    fn end_match(&mut self, cx: &mut Cx, message: Option<&str>) {
        if let Some(n) = cx.host.network() {
            n.leave_match();
        }
        self.current = None;
        if let Some(m) = message {
            cx.toast(m);
        }
        if matches!(self.stage, Stage::Playing(_) | Stage::Result(_)) {
            self.leaving = true;
            cx.fade_to(FadeKind::Black, Go::Netplay(NetGo::Modes));
        } else {
            self.show_modes(cx);
        }
    }

    /// Run a deferred change (behind the fade).
    pub fn go(&mut self, cx: &mut Cx, g: NetGo) {
        self.leaving = false;
        match g {
            NetGo::Play => self.start_course(cx),
            NetGo::Modes => {
                cx.host.unload_course();
                self.show_modes(cx);
            }
            NetGo::Again => {
                cx.host.unload_course();
                self.show_modes(cx);
                let v = self.versus;
                self.search(cx, v);
            }
        }
    }

    /// Load the match's course and start it online.
    fn start_course(&mut self, cx: &mut Cx) {
        let Some(m) = self.current.clone() else {
            return self.show_modes(cx);
        };
        let (source, name) = match &m.course {
            Some(code) => (Source::Server(code.clone()), code.clone()),
            None => match match_course(&self.pool, m.gid) {
                Some(c) => (Source::Library(c.id.clone()), c.name.clone()),
                None => return self.end_match(cx, Some("No course to play.")),
            },
        };
        let loaded = match cx.host.network() {
            Some(n) => n.play_match(&source),
            None => Err("Network Play isn't set up.".into()),
        };
        match loaded {
            Ok(l) => {
                self.course_name = if l.name.is_empty() { name } else { l.name };
                let status = cx.host.course_status();
                let mut c = NetCourse::new(&self.ui, &self.course_name, &m, &self.name, &status);
                c.since = self.tick;
                self.stage = Stage::Playing(Box::new(c));
            }
            Err(e) => {
                self.stage = Stage::Blank;
                self.end_match(cx, Some(&format!("Couldn't start the course: {e}")));
            }
        }
    }

    pub fn step(&mut self, input: &PlayInput, cx: &mut Cx) -> Option<NetplayEvent> {
        self.tick += 1;
        let tick = self.tick;
        let mut ui = input.ui;
        ui.pressed = ui.pressed | self.script.step();
        let ui = &ui;
        if self.leaving {
            // (the course keeps running under the fade: the session needs its ticks)
            if let Stage::Playing(_) = self.stage {
                cx.host.step_course(Some(0));
            }
            return None;
        }
        match &mut self.stage {
            Stage::Blank => {}
            Stage::Modes(m) => {
                let ev = m
                    .buttons
                    .step(&self.ui, &mut m.layout, ui.pressed, ui.click, tick);
                m.buttons
                    .place_cursor(&mut self.cursor, &mut m.layout, tick);
                match ev {
                    Some(ButtonEvent::Back) => return Some(NetplayEvent::Exit),
                    Some(ButtonEvent::Decided(i)) => {
                        self.mode_selection = i;
                        self.choose(cx, MODE_BUTTONS[i].1);
                    }
                    None => {
                        // the account's label follows a sign-in under way
                        if let Some(n) = cx.host.network() {
                            let a = n.account();
                            m.set_account(&a);
                        }
                    }
                }
            }
            Stage::Wait(w) if w.what == WaitFor::Players => {
                let quit = ui.pressed.has(Buttons::BACK)
                    || ui.pressed.has(Buttons::CONFIRM)
                    || ui.click.is_some_and(|p| {
                        hit(&mut w.layout, tick, &[bound("L_DecideBtn_00")], p).is_some()
                    });
                // (captures show the screen without a network)
                let n = cx.host.network()?;
                if quit {
                    n.cancel_match();
                    self.ui.sound_back(WAIT, "normal");
                    self.show_modes(cx);
                    return None;
                }
                match n.poll_match() {
                    MatchState::Searching { found, wanted } => w.set_count(found, wanted),
                    MatchState::Ready(info) => {
                        self.current = Some(info.clone());
                        if let Some(l) = Lobby::new(&self.ui, self.versus, &info, &self.name, tick)
                        {
                            self.stage = Stage::Lobby(Box::new(l));
                        }
                    }
                    MatchState::Failed(e) => {
                        cx.toast(&format!("Couldn't find other players: {e}"));
                        self.show_modes(cx);
                    }
                    MatchState::Idle => self.show_modes(cx),
                }
            }
            Stage::Wait(w) => {
                if tick - w.since >= LOADING_TICKS {
                    self.stage = Stage::Blank;
                    cx.fade_to(FadeKind::Black, Go::Netplay(NetGo::Play));
                }
            }
            Stage::Lobby(l) => {
                if l.done(tick)
                    && let Some(w) = Wait::new(&self.ui, WaitFor::Course, self.versus, tick)
                {
                    self.stage = Stage::Wait(Box::new(w));
                }
            }
            Stage::Playing(_) => return self.step_course(ui, input.start, cx),
            Stage::Result(r) => {
                let ev = r
                    .buttons
                    .step(&self.ui, &mut r.layout, ui.pressed, ui.click, tick);
                r.buttons
                    .place_cursor(&mut self.cursor, &mut r.layout, tick);
                match ev {
                    Some(ButtonEvent::Decided(0)) | Some(ButtonEvent::Back) => {
                        self.end_match(cx, None);
                    }
                    Some(ButtonEvent::Decided(_)) => {
                        if let Some(n) = cx.host.network() {
                            n.leave_match();
                        }
                        self.current = None;
                        self.leaving = true;
                        cx.fade_to(FadeKind::Black, Go::Netplay(NetGo::Again));
                    }
                    None => {}
                }
            }
        }
        None
    }

    fn choose(&mut self, cx: &mut Cx, c: ModeChoice) {
        match c {
            ModeChoice::Coop => self.search(cx, false),
            ModeChoice::Versus => self.search(cx, true),
            ModeChoice::Account => {
                let Some(n) = cx.host.network() else {
                    cx.toast("Network Play isn't set up: it needs a Nextendo game server.");
                    return;
                };
                let said = match n.account() {
                    NetAccount::SignedIn(name) => {
                        n.sign_out();
                        Some(format!("Signed out of Nextendo ({name})."))
                    }
                    NetAccount::SigningIn => Some("Signing in to Nextendo…".to_string()),
                    _ => {
                        n.sign_in();
                        None
                    }
                };
                let a = n.account();
                if let Some(t) = said {
                    cx.toast(&t);
                } else if let NetAccount::SignedIn(name) = &a {
                    cx.toast(&format!("Signed in to Nextendo as {name}."));
                }
                self.name = account_name(&a);
                if let Stage::Modes(m) = &mut self.stage {
                    m.set_account(&a);
                }
            }
            ModeChoice::Friends => cx.toast("Play with Friends (rooms) is coming later."),
            ModeChoice::Nearby | ModeChoice::Help => {
                cx.toast("Nearby Play needs local wireless; it isn't in SMCraft.")
            }
            ModeChoice::Messages => cx.toast("Multiplayer messages are coming later."),
        }
    }

    /// One tick of the course: the countdown, play, the menu, the clear, the link.
    fn step_course(&mut self, ui: &UiInput, start: bool, cx: &mut Cx) -> Option<NetplayEvent> {
        let tick = self.tick;
        let Stage::Playing(c) = &mut self.stage else {
            return None;
        };
        let m = self.current.clone()?;
        let offline = m.gid == 1 && cx.host.network().is_none();
        if !offline
            && c.cleared_at.is_none()
            && cx.host.network().map(|n| n.link()) != Some(LinkState::Up)
        {
            self.end_match(cx, Some("The connection to the other players was lost."));
            return None;
        }
        let left = c.countdown_left(tick);
        if let Some(cd) = &mut c.countdown {
            let into = 3 * COUNT_TICKS - left;
            if into >= 0 && into % COUNT_TICKS == 0 && into < 3 * COUNT_TICKS {
                let n = 3 - into / COUNT_TICKS;
                for k in 1..=3 {
                    cd.stop(&format!("Number{k}"));
                }
                cd.play(&format!("Number{n}"), tick);
                self.ui.sound_anim(COUNTDOWN, &format!("Number{n}"), None);
            }
            if left == 0 {
                for k in 1..=3 {
                    cd.stop(&format!("Number{k}"));
                }
                cd.show("N_Number_00", false);
                cd.visible("N_Start_00", Some(true));
                cd.play("Start", tick);
                self.ui.sound_anim(COUNTDOWN, "Start", None);
            }
        }
        if left > 0 {
            // the course waits for GO! (the other players' sessions wait for ours)
            if let Some(s) = &mut c.screens {
                s.step();
            }
            return None;
        }
        // START: the multiplayer menu over the running course
        let start_pressed = start && !c.held_start;
        c.held_start = start;
        if start_pressed
            && !c.menu_open
            && c.cleared_at.is_none()
            && let Some(l) = &mut c.menu
        {
            c.menu_open = true;
            l.stop("Out");
            l.play("In", tick);
            l.hold("L_Btn_00/Select", 1.0);
            self.ui.sound_anim(MENU, "In", None);
        } else if c.menu_open
            && let Some(l) = &mut c.menu
        {
            let give_up = ui.pressed.has(Buttons::CONFIRM)
                || ui
                    .click
                    .is_some_and(|p| hit(l, tick, &[bound("L_Btn_00")], p).is_some());
            if give_up {
                self.ui.sound_button(MENU, "L_Btn_00", Event::Decide);
                self.end_match(cx, Some("You gave up."));
                return None;
            }
            if ui.pressed.has(Buttons::BACK) || start_pressed {
                c.menu_open = false;
                l.play("Out", tick);
                self.ui.sound_back(MENU, "normal");
            }
        }
        // the course: the player's input, none while the menu is up
        cx.host
            .step_course(if c.menu_open { Some(0) } else { None });
        let status = cx.host.course_status();
        if let Some(s) = &mut c.screens {
            s.step();
            if c.cleared_at.is_none() {
                s.update(&status);
            }
        }
        if status.cleared && c.cleared_at.is_none() {
            c.cleared_at = Some(tick);
            c.menu_open = false;
            // characters go by position, so the one at the goal is the winner
            c.winner = Some((status.character as usize).min(m.players - 1));
            let won = c.winner == Some(m.me);
            let ticks = i64::from(tick - c.since - 3 * COUNT_TICKS);
            c.clear = clear_banner(&self.ui, self.versus, won, &self.course_name, ticks, tick);
        }
        if let Some(at) = c.cleared_at
            && tick - at >= CLEAR_TICKS
        {
            let winner = c.winner;
            if let Some(r) = Results::new(&self.ui, self.versus, &m, winner, &self.name, tick) {
                self.stage = Stage::Result(Box::new(r));
            }
        }
        None
    }

    pub fn draw(&mut self, out: &mut Vec<DrawItem>) {
        let tick = self.tick;
        match &mut self.stage {
            Stage::Playing(c) => {
                out.push(DrawItem::Course);
                if let Some(s) = &mut c.screens
                    && c.cleared_at.is_none()
                {
                    s.draw(out);
                }
                let shown = c.countdown_left(tick) > -90;
                if let Some(cd) = &mut c.countdown
                    && shown
                {
                    push_layout(out, cd, tick);
                }
                if let Some(l) = &mut c.menu
                    && (c.menu_open || l.is_playing("Out") && !l.finished("Out", tick))
                {
                    push_layout(out, l, tick);
                }
                if let Some(l) = &mut c.clear {
                    push_layout(out, l, tick);
                }
                return;
            }
            Stage::Result(r) => {
                out.push(DrawItem::Course);
                if let Some(v) = &mut r.veil {
                    push_layout(out, v, 0);
                }
                push_layout(out, &mut r.layout, tick);
                self.cursor.draw(tick, out);
                return;
            }
            _ => {}
        }
        if let Some(b) = &mut self.bg {
            push_layout(out, b, tick);
        }
        match &mut self.stage {
            Stage::Modes(m) => {
                push_layout(out, &mut m.layout, tick);
                self.cursor.draw(tick, out);
            }
            Stage::Wait(w) => push_layout(out, &mut w.layout, tick),
            Stage::Lobby(l) => push_layout(out, &mut l.layout, tick),
            _ => {}
        }
    }

    /// The player is controlling the course (touch screens show their controls).
    pub fn in_play(&self) -> bool {
        matches!(&self.stage, Stage::Playing(c)
            if !c.menu_open && c.cleared_at.is_none() && c.countdown_left(self.tick) <= 0)
    }

    /// Captures and tests: the stage shown ("modes", "wait", "loading", "lobby", "playing",
    /// "menu", "clear", "result").
    pub fn stage_name(&self) -> &'static str {
        match &self.stage {
            Stage::Modes(_) => "modes",
            Stage::Wait(w) if w.what == WaitFor::Players => "wait",
            Stage::Wait(_) => "loading",
            Stage::Lobby(_) => "lobby",
            Stage::Playing(c) if c.cleared_at.is_some() => "clear",
            Stage::Playing(c) if c.menu_open => "menu",
            Stage::Playing(_) => "playing",
            Stage::Result(_) => "result",
            Stage::Blank => "blank",
        }
    }

    /// Captures: show a screen with a made-up two-player match ("wait", "lobby", "loading",
    /// "result"; "play" / "menu" / "clear" play the match's course offline). False: unknown.
    pub fn debug(&mut self, cx: &mut Cx, what: &str, versus: bool) -> bool {
        self.versus = versus;
        let m = MatchInfo {
            me: 0,
            players: 2,
            gid: 1,
            course: None,
        };
        let tick = self.tick;
        match what {
            "wait" => match Wait::new(&self.ui, WaitFor::Players, versus, tick) {
                Some(mut w) => {
                    w.set_count(1, 2);
                    self.stage = Stage::Wait(Box::new(w));
                }
                None => return false,
            },
            "loading" => match Wait::new(&self.ui, WaitFor::Course, versus, tick) {
                Some(w) => self.stage = Stage::Wait(Box::new(w)),
                None => return false,
            },
            "lobby" => match Lobby::new(&self.ui, versus, &m, &self.name, tick) {
                Some(l) => self.stage = Stage::Lobby(Box::new(l)),
                None => return false,
            },
            "result" => match Results::new(&self.ui, versus, &m, Some(1), &self.name, tick) {
                Some(r) => {
                    self.current = Some(m.clone());
                    self.stage = Stage::Result(Box::new(r));
                }
                None => return false,
            },
            "play" | "menu" | "clear" => {
                let Some(course) = match_course(&self.pool, m.gid).cloned() else {
                    return false;
                };
                let Ok(mut l) = cx.host.load_course(&Source::Library(course.id.clone())) else {
                    return false;
                };
                if l.name.is_empty() {
                    l.name = course.name.clone();
                }
                self.current = Some(m.clone());
                let status = cx.host.course_status();
                let mut c = NetCourse::new(&self.ui, &l.name, &m, &self.name, &status);
                c.since = tick;
                if what == "menu"
                    && let Some(l) = &mut c.menu
                {
                    c.since = tick - 3 * COUNT_TICKS - 100;
                    c.menu_open = true;
                    l.play("In", tick);
                    l.hold("L_Btn_00/Select", 1.0);
                }
                if what == "clear" {
                    c.since = tick - 3 * COUNT_TICKS - 100;
                    c.cleared_at = Some(tick);
                    c.winner = Some(0);
                    c.clear = clear_banner(&self.ui, versus, true, &l.name, 3723, tick);
                }
                self.stage = Stage::Playing(Box::new(c));
            }
            _ => return false,
        }
        true
    }
}

fn account_name(a: &NetAccount) -> String {
    match a {
        NetAccount::SignedIn(n) => n.clone(),
        _ => "Player".into(),
    }
}

/// The clear: the network one for co-op [inf: Type 2 as Course World's] with the course's name
/// and time, no buttons; the win / lose banner for versus (TypeBtl 0 win, 1 lose; InBtl) [inf
/// from the layout's panes]. The game draws the winner's line from a capture of `N_Cap_01`
/// (`PF_Cap_01`); here the line itself stands in its place.
fn clear_banner(
    ui: &Ui,
    versus: bool,
    won: bool,
    name: &str,
    ticks: i64,
    tick: i32,
) -> Option<Layout> {
    let mut c = ui.layout(CLEAR)?;
    c.hold("RegionLanguageType", 1.0);
    for n in ["PF_Space_00", "PF_Claer_00", "L_CapSpace_00"] {
        c.show(n, false);
    }
    c.set_translate("N_Cap_00", [0.0, 0.0]);
    c.material_white("P_ClearMax_00", Some([255, 255, 255, 255]));
    if versus {
        c.show("N_All_00", false);
        c.show("P_ClearMax_00", false);
        c.show("PF_Cap_01", false);
        c.set_translate("N_Cap_01", [0.0, 0.0]);
        c.hold("TypeBtl", if won { 0.0 } else { 1.0 });
        c.play("InBtl", tick);
    } else {
        c.hold("Type", 2.0);
        c.show("N_InBtlBG_00", false);
        c.show("N_Cap_01", false);
        c.show("P_InNetBG_00", true);
        for n in [
            "N_Comment_00",
            "N_JudgeBtn_00",
            "N_Btn_00",
            "N_CountDown_00",
        ] {
            c.show(n, false);
        }
        let i = "L_ClearInfo_00";
        c.text(&format!("{i}/T_CourseName_00"), name);
        for n in [
            "N_CourseMaker_00",
            "L_MiiIcon_00",
            "L_MiiIcon_01",
            "L_MiiIcon_02",
            "L_NationalFlag_00",
            "N_Align_00",
            "N_FirstTime_00",
        ] {
            c.show(&format!("{i}/{n}"), false);
        }
        let ms = ticks.max(0) * 1000 / 60;
        let main = c
            .format("0010", &[digits(ms / 60000, 2), digits(ms / 1000 % 60, 2)])
            .unwrap_or_default();
        let frac = c
            .format("0011", &[digits(ms % 1000, 3)])
            .unwrap_or_default();
        c.text(&format!("{i}/T_Time_00"), &main);
        c.text(&format!("{i}/T_Time_01"), &frac);
        c.play("InNet", tick);
    }
    Some(c)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::{Host, WorldCourse};
    use crate::play::tests::MockHost;
    use crate::{Front, FrontInput, Options, Screen};

    /// The host's Network Play stand-in: what the screens asked for, and what it answers.
    struct MockNet {
        requests: Vec<MatchRequest>,
        cancels: u32,
        left: u32,
        played: Vec<Source>,
        state: MatchState,
        link: LinkState,
    }

    impl Network for MockNet {
        fn account(&mut self) -> NetAccount {
            NetAccount::SignedIn("Ada".into())
        }
        fn sign_in(&mut self) {}
        fn sign_out(&mut self) {}
        fn find_match(&mut self, r: &MatchRequest) {
            self.requests.push(r.clone());
            self.state = MatchState::Searching {
                found: 1,
                wanted: r.players,
            };
        }
        fn cancel_match(&mut self) {
            self.cancels += 1;
            self.state = MatchState::Idle;
        }
        fn poll_match(&mut self) -> MatchState {
            self.state.clone()
        }
        fn play_match(&mut self, course: &Source) -> Result<LoadedCourse, String> {
            self.played.push(course.clone());
            self.state = MatchState::Idle;
            Ok(LoadedCourse {
                name: "Shared".into(),
                ..Default::default()
            })
        }
        fn link(&mut self) -> LinkState {
            self.link
        }
        fn leave_match(&mut self) {
            self.left += 1;
        }
    }

    struct NetHost {
        inner: MockHost,
        net: MockNet,
        /// Course World courses kept on disk (TGRCode's Endless pool).
        world: Vec<CourseEntry>,
    }

    impl Host for NetHost {
        fn library(&mut self) -> Vec<CourseEntry> {
            self.inner.library()
        }
        fn challenge_courses(&mut self) -> Vec<CourseEntry> {
            self.world.clone()
        }
        fn request_server_courses(&mut self) -> bool {
            false
        }
        fn poll_server_courses(&mut self) -> Option<Vec<WorldCourse>> {
            None
        }
        fn load_course(&mut self, s: &Source) -> Result<LoadedCourse, String> {
            self.inner.load_course(s)
        }
        fn title_courses(&mut self) -> usize {
            0
        }
        fn load_title_course(&mut self, _: usize) -> bool {
            false
        }
        fn unload_course(&mut self) {}
        fn step_course(&mut self, s: Option<u32>) {
            self.inner.step_course(s)
        }
        fn course_status(&self) -> CourseStatus {
            self.inner.course_status()
        }
        fn restart_course(&mut self) {}
        fn set_character(&mut self, _: u32) {}
        fn set_run_counters(&mut self, _: u32, _: u32, _: u32) {}
        fn load_value(&mut self, _: &str) -> Option<String> {
            None
        }
        fn store_value(&mut self, _: &str, _: &str) {}
        fn random(&mut self) -> u64 {
            7
        }
        fn network(&mut self) -> Option<&mut dyn Network> {
            Some(&mut self.net)
        }
    }

    fn stage(f: &Front) -> &'static str {
        match &f.screen {
            Screen::Netplay(n) => n.stage_name(),
            _ => "not netplay",
        }
    }

    /// Press `b` for a tick, then run `n` ticks with nothing held.
    fn press(f: &mut Front, h: &mut NetHost, b: Buttons, n: usize) {
        let i = FrontInput {
            menu: b,
            ..Default::default()
        };
        f.tick(&i, h);
        for _ in 0..n {
            f.tick(&FrontInput::default(), h);
        }
    }

    fn run_until(f: &mut Front, h: &mut NetHost, want: &str, most: usize) {
        for _ in 0..most {
            if stage(f) == want {
                return;
            }
            f.tick(&FrontInput::default(), h);
        }
        assert_eq!(stage(f), want);
    }

    /// The whole flow on a made-up network (with the owner's SMM2 UI import; skipped without):
    /// search, lobby, the course, the clear, Continue playing, a lost peer, Quit.
    #[test]
    fn network_play_from_the_mode_select_to_the_result_and_back() {
        let Some(ui) = crate::testing::smm2() else {
            return;
        };
        let library = vec![entry("k2", true), entry("k1", true), entry("l", false)];
        let mut h = NetHost {
            inner: MockHost {
                library: library.clone(),
                ..Default::default()
            },
            net: MockNet {
                requests: Vec::new(),
                cancels: 0,
                left: 0,
                played: Vec::new(),
                state: MatchState::Idle,
                link: LinkState::Up,
            },
            world: Vec::new(),
        };
        let opts = Options {
            boot: false,
            instant_fades: true,
            ..Default::default()
        };
        let mut f = Front::new(ui, None, opts, &mut h);
        assert!(f.capture(&mut h, "netplay", 0, 0));
        assert_eq!(stage(&f), "modes");
        // Multiplayer Co-op (the first button): the search for the shared pool
        press(&mut f, &mut h, Buttons::CONFIRM, 20);
        assert_eq!(stage(&f), "wait");
        let pool = course_pool(&library);
        assert_eq!(
            h.net.requests,
            [MatchRequest {
                versus: false,
                players: PLAYERS,
                pool: pool_id(&pool),
                pick: None,
            }]
        );
        // found: the lobby, then the course everyone draws, after its countdown
        h.net.state = MatchState::Ready(MatchInfo {
            me: 1,
            players: 2,
            gid: 7,
            course: None,
        });
        run_until(&mut f, &mut h, "lobby", 3);
        run_until(&mut f, &mut h, "loading", 400);
        run_until(&mut f, &mut h, "playing", 60);
        let course = match_course(&pool, 7).unwrap();
        assert_eq!(h.net.played, [Source::Library(course.id.clone())]);
        for _ in 0..3 * COUNT_TICKS - 5 {
            f.tick(&FrontInput::default(), &mut h);
        }
        assert_eq!(h.inner.steps, 0, "the course waits for GO!");
        for _ in 0..10 {
            f.tick(&FrontInput::default(), &mut h);
        }
        assert!(h.inner.steps > 0);
        // somebody reaches the goal: the clear, then the result
        h.inner.status.cleared = true;
        h.inner.status.character = 1;
        run_until(&mut f, &mut h, "clear", 3);
        run_until(&mut f, &mut h, "result", CLEAR_TICKS as usize + 5);
        // Continue playing (focused): leave the match, search again
        for _ in 0..40 {
            f.tick(&FrontInput::default(), &mut h);
        }
        h.inner.status.cleared = false;
        press(&mut f, &mut h, Buttons::CONFIRM, 20);
        assert_eq!(h.net.left, 1);
        assert_eq!(h.net.requests.len(), 2);
        assert_eq!(stage(&f), "wait");
        // a peer drops out mid-course: back to the mode select
        h.net.state = MatchState::Ready(MatchInfo {
            me: 0,
            players: 2,
            gid: 8,
            course: None,
        });
        run_until(&mut f, &mut h, "playing", 500);
        h.net.link = LinkState::Lost;
        run_until(&mut f, &mut h, "modes", 5);
        assert_eq!(h.net.left, 2);
        // Quit while searching cancels the search
        h.net.link = LinkState::Up;
        press(&mut f, &mut h, Buttons::CONFIRM, 20);
        assert_eq!(stage(&f), "wait");
        press(&mut f, &mut h, Buttons::BACK, 1);
        assert_eq!(h.net.cancels, 1);
        assert_eq!(stage(&f), "modes");
        // back from the mode select: Course World's hub
        press(&mut f, &mut h, Buttons::BACK, 3);
        assert!(matches!(f.screen, Screen::World(_)));
    }

    #[test]
    fn with_course_world_courses_the_session_openers_pick_is_played() {
        let Some(ui) = crate::testing::smm2() else {
            return;
        };
        let world: Vec<CourseEntry> = ["AB1-CD2-EF3", "GH4-JK5-LM6"]
            .iter()
            .map(|c| CourseEntry {
                id: format!("/pool/{c}"),
                meta: crate::host::CourseMeta {
                    code: Some(c.to_string()),
                    ..Default::default()
                },
                ..Default::default()
            })
            .collect();
        let mut h = NetHost {
            inner: MockHost::default(),
            net: MockNet {
                requests: Vec::new(),
                cancels: 0,
                left: 0,
                played: Vec::new(),
                state: MatchState::Idle,
                link: LinkState::Up,
            },
            world,
        };
        let opts = Options {
            boot: false,
            instant_fades: true,
            ..Default::default()
        };
        let mut f = Front::new(ui, None, opts, &mut h);
        assert!(f.capture(&mut h, "netplay", 0, 0));
        press(&mut f, &mut h, Buttons::CONFIRM, 20);
        assert_eq!(stage(&f), "wait", "no Story Mode courses needed");
        let r = &h.net.requests[0];
        assert_eq!(r.pool, WORLD_POOL);
        assert!(r.pick.as_deref().is_some_and(|c| c.contains('-')));
        // another player opened the session: their pick is played (downloaded by code)
        h.net.state = MatchState::Ready(MatchInfo {
            me: 1,
            players: 2,
            gid: 7,
            course: Some("XY7-ZW8-QR9".into()),
        });
        run_until(&mut f, &mut h, "playing", 500);
        assert_eq!(h.net.played, [Source::Server("XY7-ZW8-QR9".into())]);
    }

    fn entry(key: &str, story: bool) -> CourseEntry {
        CourseEntry {
            id: format!("/c/{key}"),
            key: key.into(),
            built_in: true,
            playable: true,
            collection: Some(if story { "Story Mode" } else { "Lesson" }.into()),
            ..Default::default()
        }
    }

    #[test]
    fn every_player_draws_the_same_course_from_the_shared_pool() {
        let a = vec![
            entry("c3", true),
            entry("a1", true),
            entry("l1", false),
            entry("b2", true),
        ];
        // another player's library lists the same courses in another order
        let b = vec![entry("b2", true), entry("c3", true), entry("a1", true)];
        let (pa, pb) = (course_pool(&a), course_pool(&b));
        assert_eq!(pa.len(), 3, "only the Story Mode courses");
        assert_eq!(pool_id(&pa), pool_id(&pb));
        for gid in [1, 2, 77, 123_456, u32::MAX] {
            assert_eq!(
                match_course(&pa, gid).map(|e| &e.key),
                match_course(&pb, gid).map(|e| &e.key)
            );
        }
        // sessions next to each other get different courses now and then
        let picks: std::collections::HashSet<_> = (100..120)
            .filter_map(|g| match_course(&pa, g).map(|e| e.key.clone()))
            .collect();
        assert!(picks.len() > 1);
        // a different pool: a different fingerprint, so those players never meet
        assert_ne!(pool_id(&pa), pool_id(&pa[..2]));
        assert!(match_course(&[], 5).is_none());
    }
}
