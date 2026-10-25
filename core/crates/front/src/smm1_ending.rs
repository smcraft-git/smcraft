//! The SMM1 100 Mario ending's rescue scene (`NetEndingDemoScene`, research/specs/smm1-ending.md
//! §2): not a layout but models, drawn here from the SMM1 import's `ui/ending/ending.json`
//! (`mmk_import::smm1_ending`) as flat quads — the style's stage (`field_model`) at (0, 48), Peach
//! (Toad on Easy) at (416, 48), and the demo Mario in the form he reached the goal with, walking
//! in from x −32 at 1.5 a frame to x 384 under the 384 × 216 camera that follows him inside
//! x 0–1152, y 0–432; then the balloon `Play_EndingText_00` with the line for the difficulty and
//! his costume, and the hold before the credits. Every timer and position is the binary's
//! (v208 addresses below); what is not is marked [inf].

use std::collections::{BTreeSet, HashMap};

use mmk_modes::hundred::RewardNote;
use mmk_ui::Quad;
use serde_json::Value;

use crate::layout::{Layout, Ui};
use crate::menu::UiInput;
use crate::smm1_costume::CostumeWindow;
use crate::smm1_window::{MessageWindow, WindowEvent, WindowKind};
use crate::{DrawItem, push_layout};

/// The view (world units) [inf: the projection size is a runtime global, 0x102569f8; the
/// scissor 384·3 × 216·3 of 0x02703e14 fits it].
pub const VIEW: [f32; 2] = [384.0, 216.0];
/// The camera's bounds: x 0–1152 (384 × 3), y 0–432 (216 × 2) [obs 0x027038c8].
pub const BOUNDS: [f32; 2] = [1152.0, 432.0];
/// Where the stage stands (0x100ba19c): its models are in front of nothing.
pub const FIELD_AT: [f32; 2] = [0.0, 48.0];
/// Peach / Toad (0x02703390).
pub const PEACH_AT: [f32; 2] = [416.0, 48.0];
/// Mario's start and stop (0x02703390, 0x025ef034) and his walk speed.
pub const MARIO_START: [f32; 2] = [-32.0, 47.0];
pub const MARIO_STOP: f32 = 384.0;
pub const WALK_SPEED: f32 = 1.5;
/// The walk animation's rate in the 2D styles (0x025ee7e8).
pub const RUN_RATE: f32 = 1.125;
/// DemoStartWait, TextInWait, PlayerStop (balloon Out at 60 left), NextSceneWait (WU with
/// Peach 300) [obs 0x025eef68 … 0x025ef720].
pub const START_WAIT: i32 = 30;
pub const TEXT_IN_WAIT: i32 = 30;
pub const STOP: i32 = 360;
pub const STOP_OUT_AT: i32 = 60;
pub const NEXT_SCENE_WAIT: i32 = 120;
pub const NEXT_SCENE_WAIT_WU_PEACH: i32 = 300;

/// A model shape: its texture and triangles ([x, y, z, u, v] per vertex).
#[derive(Clone, Debug, Default)]
struct Shape {
    material: String,
    texture: Option<String>,
    wrap: [u8; 2],
    tris: Vec<[f32; 5]>,
}

#[derive(Clone, Debug, Default)]
struct Pattern {
    frames: f32,
    looping: bool,
    textures: Vec<String>,
    tracks: HashMap<String, (u32, Vec<(f32, u32)>)>,
}

impl Pattern {
    /// The texture of `material` at `frame` (None: the pattern doesn't animate it).
    fn texture(&self, material: &str, frame: f32) -> Option<&str> {
        let (base, keys) = self.tracks.get(material)?;
        let f = if self.looping && self.frames > 0.0 {
            frame.rem_euclid(self.frames)
        } else {
            frame.min(self.frames)
        };
        let i = keys
            .iter()
            .take_while(|k| k.0 <= f)
            .last()
            .map_or(*base, |k| k.1);
        self.textures.get(i as usize).map(String::as_str)
    }
}

#[derive(Clone, Debug, Default)]
struct Archive {
    models: HashMap<String, Vec<Shape>>,
    patterns: HashMap<String, Pattern>,
}

/// `ui/ending/ending.json` of the SMM1 import, parsed.
#[derive(Clone, Debug, Default)]
pub struct EndingModels {
    archives: HashMap<String, Archive>,
}

impl EndingModels {
    pub const FILE: &str = "ui/ending/ending.json";

    pub fn load(ui: &Ui) -> Option<Self> {
        Self::from_json(&ui.cache_mut().json_file(Self::FILE)?)
    }

    pub fn from_json(doc: &Value) -> Option<Self> {
        let mut out = EndingModels::default();
        for (name, a) in doc.get("archives")?.as_object()? {
            let mut arch = Archive::default();
            for (m, shapes) in a["models"].as_object().into_iter().flatten() {
                let shapes = shapes
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|s| Shape {
                        material: s["material"].as_str().unwrap_or_default().to_string(),
                        texture: s["texture"].as_str().map(str::to_string),
                        wrap: [
                            s["wrap"][0].as_u64().unwrap_or(2) as u8,
                            s["wrap"][1].as_u64().unwrap_or(2) as u8,
                        ],
                        tris: s["v"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .map(|v| std::array::from_fn(|k| v[k].as_f64().unwrap_or(0.0) as f32))
                            .collect(),
                    })
                    .collect();
                arch.models.insert(m.clone(), shapes);
            }
            for (p, v) in a["patterns"].as_object().into_iter().flatten() {
                let tracks = v["tracks"]
                    .as_object()
                    .into_iter()
                    .flatten()
                    .map(|(mat, t)| {
                        let keys = t["keys"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .map(|k| {
                                (
                                    k[0].as_f64().unwrap_or(0.0) as f32,
                                    k[1].as_u64().unwrap_or(0) as u32,
                                )
                            })
                            .collect();
                        (mat.clone(), (t["base"].as_u64().unwrap_or(0) as u32, keys))
                    })
                    .collect();
                arch.patterns.insert(
                    p.clone(),
                    Pattern {
                        frames: v["frames"].as_f64().unwrap_or(0.0) as f32,
                        looping: v["loop"].as_bool().unwrap_or(false),
                        textures: v["textures"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(|t| t.as_str().map(str::to_string))
                            .collect(),
                        tracks,
                    },
                );
            }
            out.archives.insert(name.clone(), arch);
        }
        Some(out)
    }

    fn has(&self, archive: &str) -> bool {
        self.archives.contains_key(archive)
    }
}

/// The style prefix of the scene's archives: the course's own for the 2D styles. NSMBU's demo
/// Mario is a 3D model the flat-quad import can't draw, and 3D World isn't in SMM1: both take
/// SMW's scene [departure: DECISIONS.md "100 Mario ending: the rescue scene"].
pub fn scene_style(style: u32) -> &'static str {
    match style {
        0 => "M1",
        1 => "M3",
        _ => "MW",
    }
}

/// The demo Mario's model for the form he had at the goal (`mmk_sim::player::Form` as a number)
/// in the scene's style: SMM1's forms there, Super Mario's model for forms SMM1 doesn't have
/// [inf].
pub fn player_model(style: &str, form: u8) -> String {
    let name = match (style, form) {
        (_, 0) => "MarioMdl",
        (_, 2) | ("M1", 8) => "FireMarioMdl",
        ("M3", 3) => "TnkMarioMdl",
        ("M1", 9) => "MegaMarioMdl",
        ("M1", 10) => "SlimMarioMdl",
        // a costume, Link, the Frog Suit, the Power Balloon, the SMB2 Mushroom: small bodies
        (_, 11..=13) | (_, 15) => "MarioMdl",
        _ => "SpMarioMdl",
    };
    format!("{style}_Player_{name}")
}

/// The rescue scene's music (0x02703680: SLink `EndingPeachBGM` for Normal and up, else
/// `EndingKinopioBGM`, by the course's style) [obs data]; 3D World takes SMW's [departure].
pub fn rescue_music(style: u32, peach: bool) -> &'static str {
    match (style, peach) {
        (0, _) => "M1_BGM_Ending",
        (1, true) => "M3_BGM_EndingDemo2",
        (1, false) => "M3_BGM_EndingDemo1",
        (3, true) => "WU_STRM_BGM_LAST_BOSS2_HARP",
        (3, false) => "WU_STRM_BGM_LAST_BOSS_HARP_Dummy",
        (_, true) => "MW_BGM_Ending2",
        (_, false) => "SuperMarioWorld_21_RescueEgg",
    }
}

/// The staff roll's music (0x027053e0: Normal and Expert `_normal`, Easy and Super Expert
/// `_easy`) [obs].
pub fn credits_music(d: usize) -> &'static str {
    if matches!(d, 1 | 2) {
        "BGM_100mario_staffroll_normal"
    } else {
        "BGM_100mario_staffroll_easy"
    }
}

/// What the rescue scene shows, from the run's end.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RescueInfo {
    /// The cleared course's style (`CourseStatus::style`).
    pub style: u32,
    /// 0 Easy … 3 Super Expert: Easy shows Toad.
    pub difficulty: usize,
    /// Mario's form at the goal (`mmk_sim::player::Form` as a number).
    pub form: u8,
    /// His Mystery Mushroom costume (SMM1 costume id), if he wore one.
    pub costume: Option<u16>,
}

/// The balloon's line (0x024e8c84, tables 0x10099aec / 0x10099af0): row by difficulty (Easy 1,
/// Normal 2, Expert and Super Expert 3), variant by costume (none or Mario 0, Peach 1, Toad 6 →
/// 2, Bowser 3, any other 4) [obs].
pub fn thank_you_label(difficulty: usize, costume: Option<u16>) -> String {
    let row = match difficulty {
        0 => 1,
        1 => 2,
        _ => 3,
    };
    let variant = match costume {
        None | Some(0) => 0,
        Some(1) => 1,
        Some(6) => 2,
        Some(3) => 3,
        Some(_) => 4,
    };
    format!("T_ThankYou_{row}{variant}")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RescueState {
    StartWait,
    Walk,
    TextInWait,
    Stop,
    NextSceneWait,
    Done,
}

/// `NetEndingDemoScene`: the demo Mario's state machine (class 0x025ed9c4, timer +0x1460) with
/// the stage, Peach / Toad, the camera and the balloon.
pub struct RescueScene {
    ui: Ui,
    models: Option<EndingModels>,
    style: &'static str,
    peach: bool,
    player: String,
    text: Option<Layout>,
    pub info: RescueInfo,
    pub tick: i32,
    pub state: RescueState,
    timer: i32,
    x: f32,
    /// The walk animation's frame (it runs at 1.125 a tick while he walks).
    run_frame: f32,
    balloon_out: bool,
    /// SMW's cape form: the cape model too.
    cape: bool,
    /// The tick he stopped (the cape's `mt_down` runs from it).
    stopped_at: i32,
}

impl RescueScene {
    pub fn new(ui: &Ui, info: RescueInfo) -> Self {
        let models = EndingModels::load(ui);
        let style = scene_style(info.style);
        let mut player = player_model(style, info.form);
        if models.as_ref().is_some_and(|m| !m.has(&player)) {
            player = format!("{style}_Player_MarioMdl");
        }
        crate::smm1::play_music(ui, rescue_music(info.style, info.difficulty != 0));
        let mut text = ui.layout("Play_EndingText_00");
        if let Some(t) = &mut text {
            t.text_from(
                "T_ThankYou_00",
                &thank_you_label(info.difficulty, info.costume),
            );
            t.show("N_All_00", false);
        }
        Self {
            ui: ui.clone(),
            models,
            style,
            peach: info.difficulty != 0,
            player,
            text,
            info,
            tick: 0,
            state: RescueState::StartWait,
            timer: START_WAIT,
            x: MARIO_START[0],
            run_frame: 0.0,
            balloon_out: false,
            cape: style == "MW" && info.form == 4,
            stopped_at: 0,
        }
    }

    /// The scene has its models (an SMM1 import from importer 5 on).
    pub fn has_models(&self) -> bool {
        self.models.is_some()
    }

    /// Mario's x (world units).
    pub fn mario_x(&self) -> f32 {
        self.x
    }

    /// One tick; true when the scene asks for the credits (`NetEndingScene`, 0x02768ba4).
    pub fn step(&mut self) -> bool {
        self.tick += 1;
        let t = self.tick;
        match self.state {
            RescueState::StartWait => {
                self.timer -= 1;
                if self.timer <= 0 {
                    self.state = RescueState::Walk;
                }
            }
            RescueState::Walk => {
                self.run_frame += RUN_RATE;
                if self.x + WALK_SPEED >= MARIO_STOP {
                    self.x = MARIO_STOP;
                    self.stopped_at = t;
                    self.state = RescueState::TextInWait;
                    self.timer = TEXT_IN_WAIT;
                } else {
                    self.x += WALK_SPEED;
                }
            }
            RescueState::TextInWait => {
                self.timer -= 1;
                if self.timer <= 0 {
                    // the balloon's In (24), then its Wait loop [obs 0x024e8c84 / 0x024e8e68]
                    self.state = RescueState::Stop;
                    self.timer = STOP;
                    if let Some(l) = &mut self.text {
                        l.visible("N_All_00", None);
                        l.play("In", t);
                        let w = l.ticks("In", 1);
                        l.play_loop("Wait", t + w);
                    }
                }
            }
            RescueState::Stop => {
                self.timer -= 1;
                if self.timer == STOP_OUT_AT && !self.balloon_out {
                    self.balloon_out = true;
                    if let Some(l) = &mut self.text {
                        l.stop("Wait");
                        l.play("Out", t);
                    }
                }
                if self.timer <= 0 {
                    self.state = RescueState::NextSceneWait;
                    // NSMBU with Peach waits for its longer harp tune (0x025ef720) [obs]
                    self.timer = if self.info.style == 3 && self.peach {
                        NEXT_SCENE_WAIT_WU_PEACH
                    } else {
                        NEXT_SCENE_WAIT
                    };
                }
            }
            RescueState::NextSceneWait => {
                self.timer -= 1;
                if self.timer <= 0 {
                    self.state = RescueState::Done;
                    return true;
                }
            }
            RescueState::Done => {}
        }
        false
    }

    /// The camera's centre: Mario, kept inside the bounds (0x027038c8) [obs].
    pub fn camera(&self) -> [f32; 2] {
        let (hw, hh) = (VIEW[0] / 2.0, VIEW[1] / 2.0);
        [
            self.x.clamp(hw, BOUNDS[0] - hw),
            MARIO_START[1].clamp(hh, BOUNDS[1] - hh),
        ]
    }

    /// Pattern `name` of `archive`, else of the style's shared player animations.
    fn pattern<'a>(&'a self, archive: &str, name: &str) -> Option<&'a Pattern> {
        let m = self.models.as_ref()?;
        m.archives
            .get(archive)
            .and_then(|a| a.patterns.get(name))
            .or_else(|| {
                m.archives
                    .get(&format!("{}_Player_CommonAnm", self.style))?
                    .patterns
                    .get(name)
            })
    }

    fn quads_of(
        &self,
        archive: &str,
        model: Option<&str>,
        pattern: Option<(&str, f32)>,
        at: [f32; 2],
        out: &mut Vec<Quad>,
    ) {
        let Some(m) = &self.models else { return };
        let Some(a) = m.archives.get(archive) else {
            return;
        };
        // a player archive holds one model
        let shapes = match model {
            Some(name) => a.models.get(name),
            None => a.models.values().next(),
        };
        let Some(shapes) = shapes else {
            return;
        };
        let cam = self.camera();
        let s = 720.0 / VIEW[1];
        let place = |v: &[f32; 5]| [(v[0] + at[0] - cam[0]) * s, (v[1] + at[1] - cam[1]) * s];
        let pat = pattern.and_then(|(p, f)| self.pattern(archive, p).map(|p| (p, f)));
        let mut drawn: Vec<(f32, Quad)> = Vec::new();
        for sh in shapes {
            let tex = pat
                .and_then(|(p, f)| p.texture(&sh.material, f))
                .or(sh.texture.as_deref());
            let Some(tex) = tex else { continue };
            let Some(t) = self.ui.cache_mut().texture_file(archive, tex) else {
                continue;
            };
            let wrap = |w: u8| match w {
                0 => 1,
                1 => 2,
                _ => 0,
            };
            let flags = mmk_ui::BLEND_NORMAL
                | (mmk_ui::COMBINE_NONE << mmk_ui::SHIFT_COMBINE_COLOR)
                | (mmk_ui::COMBINE_NONE << mmk_ui::SHIFT_COMBINE_ALPHA)
                | (wrap(sh.wrap[0]) << mmk_ui::SHIFT_WRAP_S0)
                | (wrap(sh.wrap[1]) << mmk_ui::SHIFT_WRAP_T0);
            for tri in sh.tris.as_chunks::<3>().0 {
                let (a, b, c) = (&tri[0], &tri[1], &tri[2]);
                let z = a[2].max(b[2]).max(c[2]);
                drawn.push((
                    z,
                    Quad {
                        pos: [place(a), place(b), place(c), place(c)],
                        uv0: [[a[3], a[4]], [b[3], b[4]], [c[3], c[4]], [c[3], c[4]]],
                        color: [[255; 4]; 4],
                        black: [0, 0, 0, 0],
                        white: [255; 4],
                        tex0: t.id,
                        tex1: mmk_ui::TEXTURE_NONE,
                        flags,
                        ..Default::default()
                    },
                ));
            }
        }
        // farther first (the camera looks down −z: SMB1's sky at z −1 is behind the stage)
        drawn.sort_by(|a, b| a.0.total_cmp(&b.0));
        out.extend(drawn.into_iter().map(|d| d.1));
    }

    pub fn draw(&mut self, out: &mut Vec<DrawItem>) {
        let mut quads = Vec::new();
        // the clear colour behind the stage [inf: black; SMB1's stage has its own sky]
        let h = [640.0, 360.0];
        quads.push(Quad {
            pos: [[-h[0], h[1]], [h[0], h[1]], [-h[0], -h[1]], [h[0], -h[1]]],
            color: [[0, 0, 0, 255]; 4],
            black: [0, 0, 0, 0],
            white: [255; 4],
            tex0: mmk_ui::TEXTURE_NONE,
            tex1: mmk_ui::TEXTURE_NONE,
            flags: mmk_ui::BLEND_NORMAL,
            ..Default::default()
        });
        let stage = format!("{}_Object_peach", self.style);
        self.quads_of(&stage, Some("field_model"), None, FIELD_AT, &mut quads);
        // Peach / Toad: their wait patterns only in SMW and NSMBU (0x025ec70c) [obs]
        let (model, anim) = if self.peach {
            ("peach_model", "wait_peach")
        } else {
            ("kinopio_model", "wait_kinopio")
        };
        let animated = self.style == "MW";
        let pat = animated.then_some((anim, self.tick as f32));
        self.quads_of(&stage, Some(model), pat, PEACH_AT, &mut quads);
        let walking = matches!(self.state, RescueState::StartWait | RescueState::Walk);
        let at = [self.x, MARIO_START[1]];
        if self.cape {
            // the cape over Super Mario: `mt_walk` while he walks, `mt_down` when he stops,
            // then `mt_wait` (0x025ef1c8, 0x025eeebc) [obs]; drawn behind him at his feet [inf]
            let cape = if walking {
                ("mt_walk", self.run_frame)
            } else {
                let since = (self.tick - self.stopped_at) as f32;
                let down = self.pattern(CAPE, "mt_down").map_or(0.0, |p| p.frames);
                if since < down {
                    ("mt_down", since)
                } else {
                    ("mt_wait", 0.0)
                }
            };
            self.quads_of(CAPE, None, Some(cape), at, &mut quads);
        }
        let anim = if walking {
            ("run", self.run_frame)
        } else {
            ("wait", 0.0)
        };
        let player = self.player.clone();
        self.quads_of(&player, None, Some(anim), at, &mut quads);
        out.push(DrawItem::Layout {
            ui: self.ui.kind(),
            size: [1280.0, 720.0],
            quads,
        });
        if let Some(t) = &mut self.text {
            push_layout(out, t, self.tick);
        }
    }
}

/// SMW's cape (drawn over Super Mario for the cape form).
const CAPE: &str = "MW_Player_Mant";

/// The pause after the credits before the costume window (`cCharaMarioDemoIn`, 0x0259f4c4).
pub const REWARD_WAIT: i32 = 180;

/// The run's Mystery Mushroom reward, for the credits' end (the costume picker 0x021f2428).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CreditsReward {
    /// The costume won (SMM1 id), shown in the costume window.
    pub costume: Option<u16>,
    /// The costumes owned with it (the window's list).
    pub owned: BTreeSet<u16>,
    /// The picker's "pool exhausted" note (credits manager +0x111): the system window after.
    pub note: Option<RewardNote>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CreditsState {
    Rolling,
    /// cCharaMarioDemoIn: the 180 frames before the costume window.
    Reward,
    /// cCharaMarioDemoWait: the costume window is up.
    Costume,
    /// cCharaMarioComplete: the "every costume" system window.
    Complete,
    Done,
}

/// `NetEndingScene` (research/specs/smm1-ending.md §4): the staff credits listing the run's
/// courses (Easy `Nwk_StaffCredit_01`, 8 rows; Normal and Expert `_00`, 16; Super Expert `_03`, 6
/// [obs 0x02598328]), their `InOut` played once at 1.0 with no input [obs 0x02517774]; then the
/// layout is reset (hidden) and, with a costume won, 180 frames later the costume window opens
/// (0x0259f4c4, [`crate::smm1_costume`]) until the player closes it (0x0259f548); when the pool
/// is exhausted the system window says so — `Msg_0052` when nothing is left at any level, else
/// `Msg_0051` (0x0259f5ac) — and is closed with OK; then Course World (0x0259f40c → state 6).
pub struct CreditsScene {
    ui: Ui,
    layout: Option<Layout>,
    pub tick: i32,
    pub state: CreditsState,
    timer: i32,
    reward: CreditsReward,
    costume: Option<CostumeWindow>,
    message: Option<MessageWindow>,
}

/// The credits layout of difficulty `d`.
pub fn credits_layout(d: usize) -> &'static str {
    match d {
        0 => "Nwk_StaffCredit_01",
        3 => "Nwk_StaffCredit_03",
        _ => "Nwk_StaffCredit_00",
    }
}

/// The system message after the costume window (message ids 0x34 / 0x33 → `Nwk_Message`
/// `Msg_0052` / `Msg_0051`, table 0x10224e54) [obs].
pub fn complete_message(note: RewardNote) -> &'static str {
    match note {
        RewardNote::AllOwned => "0052",
        RewardNote::TryAnotherLevel => "0051",
    }
}

impl CreditsScene {
    /// `list`: the cleared courses (name, maker); `reward`: what the run won.
    pub fn new(ui: &Ui, d: usize, list: &[(String, String)], reward: CreditsReward) -> Self {
        let mut layout = ui.layout(credits_layout(d));
        if let Some(c) = &mut layout {
            for i in 0..16 {
                let part = format!("L_MiiInfo_{i:02}");
                if c.pane(&part).is_none() {
                    continue;
                }
                match list.get(i) {
                    Some((name, maker)) => {
                        c.visible(&part, None);
                        c.text(&format!("{part}/T_CourseName_00"), name);
                        c.text(&format!("{part}/T_MiiName_00"), maker);
                        // no country: the flag and its frame stay hidden (a code ≥ 195,
                        // 0x02518808) [obs]
                        c.show(&format!("{part}/L_NationalFlag_00"), false);
                        c.show(&format!("{part}/P_Frame_00"), false);
                    }
                    None => c.show(&part, false),
                }
            }
            c.play("InOut", 0);
        }
        crate::smm1::play_music(ui, credits_music(d));
        Self {
            ui: ui.clone(),
            layout,
            tick: 0,
            state: CreditsState::Rolling,
            timer: 0,
            reward,
            costume: None,
            message: None,
        }
    }

    /// Captures: the credits' end at once (the costume window opens on the next tick).
    pub fn skip_to_reward(&mut self) {
        self.state = CreditsState::Reward;
        self.timer = 1;
    }

    /// cCharaMarioComplete when the pool ran out, else the end.
    fn after_costume(&mut self) {
        let window = self.reward.note.and_then(|n| {
            let id = complete_message(n);
            let text = self.ui.message("Nwk_Message", &format!("Msg_{id}"))?;
            let ok = self
                .ui
                .message("Nwk_Message", &format!("BtnC_{id}"))
                .unwrap_or_else(|| "OK".into());
            MessageWindow::open(&self.ui, WindowKind::Ok, &text, &[&ok], false)
        });
        match window {
            Some(w) => {
                self.message = Some(w);
                self.state = CreditsState::Complete;
            }
            None => self.state = CreditsState::Done,
        }
    }

    /// One tick; true when the scene is over (Course World next).
    pub fn step(&mut self, input: &UiInput) -> bool {
        self.tick += 1;
        match self.state {
            CreditsState::Rolling => {
                let end = self.layout.as_mut().map_or(0, |l| l.ticks("InOut", 1));
                if self.tick >= end {
                    // the credit layout is reset; a costume waits 180 frames, none goes to the
                    // message (0x0259f40c) [obs]
                    if self.reward.costume.is_some() {
                        self.state = CreditsState::Reward;
                        self.timer = REWARD_WAIT;
                    } else {
                        self.after_costume();
                    }
                }
            }
            CreditsState::Reward => {
                self.timer -= 1;
                if self.timer <= 0 {
                    let id = self.reward.costume.unwrap_or_default();
                    self.costume = CostumeWindow::open(&self.ui, id, &self.reward.owned);
                    if self.costume.is_some() {
                        self.state = CreditsState::Costume;
                    } else {
                        self.after_costume();
                    }
                }
            }
            CreditsState::Costume => {
                if self.costume.as_mut().is_none_or(|w| w.step(input)) {
                    self.costume = None;
                    self.after_costume();
                }
            }
            CreditsState::Complete => {
                if let Some(w) = &mut self.message
                    && w.step(input) == Some(WindowEvent::Closed)
                {
                    self.message = None;
                    self.state = CreditsState::Done;
                }
            }
            CreditsState::Done => {}
        }
        self.state == CreditsState::Done
    }

    pub fn draw(&mut self, out: &mut Vec<DrawItem>) {
        let h = [640.0, 360.0];
        out.push(DrawItem::Layout {
            ui: self.ui.kind(),
            size: [1280.0, 720.0],
            quads: vec![Quad {
                pos: [[-h[0], h[1]], [h[0], h[1]], [-h[0], -h[1]], [h[0], -h[1]]],
                color: [[0, 0, 0, 255]; 4],
                black: [0, 0, 0, 0],
                white: [255; 4],
                tex0: mmk_ui::TEXTURE_NONE,
                tex1: mmk_ui::TEXTURE_NONE,
                flags: mmk_ui::BLEND_NORMAL,
                ..Default::default()
            }],
        });
        if self.state == CreditsState::Rolling
            && let Some(l) = &mut self.layout
        {
            push_layout(out, l, self.tick);
        }
        if let Some(w) = &mut self.costume {
            w.draw(out);
        }
        if let Some(w) = &mut self.message {
            w.draw(out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::smm1;

    #[test]
    fn the_balloon_line_follows_difficulty_and_costume() {
        assert_eq!(thank_you_label(0, None), "T_ThankYou_10");
        assert_eq!(thank_you_label(1, Some(0)), "T_ThankYou_20");
        assert_eq!(thank_you_label(1, Some(1)), "T_ThankYou_21");
        assert_eq!(thank_you_label(2, Some(6)), "T_ThankYou_32");
        assert_eq!(thank_you_label(3, Some(3)), "T_ThankYou_33");
        assert_eq!(thank_you_label(3, Some(42)), "T_ThankYou_34");
    }

    #[test]
    fn the_ending_music_by_style_and_difficulty() {
        assert_eq!(rescue_music(0, false), "M1_BGM_Ending");
        assert_eq!(rescue_music(1, false), "M3_BGM_EndingDemo1");
        assert_eq!(rescue_music(2, false), "SuperMarioWorld_21_RescueEgg");
        assert_eq!(rescue_music(3, true), "WU_STRM_BGM_LAST_BOSS2_HARP");
        assert_eq!(rescue_music(4, true), "MW_BGM_Ending2");
        assert_eq!(credits_music(0), "BGM_100mario_staffroll_easy");
        assert_eq!(credits_music(2), "BGM_100mario_staffroll_normal");
        assert_eq!(credits_music(3), "BGM_100mario_staffroll_easy");
    }

    #[test]
    fn forms_pick_the_styles_models() {
        assert_eq!(player_model("M1", 0), "M1_Player_MarioMdl");
        assert_eq!(player_model("M1", 1), "M1_Player_SpMarioMdl");
        assert_eq!(player_model("M1", 9), "M1_Player_MegaMarioMdl");
        assert_eq!(player_model("M3", 3), "M3_Player_TnkMarioMdl");
        assert_eq!(
            player_model("MW", 4),
            "MW_Player_SpMarioMdl",
            "the cape is drawn over"
        );
        assert_eq!(
            player_model("MW", 6),
            "MW_Player_SpMarioMdl",
            "a Cat goes Super"
        );
        assert_eq!(scene_style(3), "MW");
        assert_eq!(scene_style(4), "MW");
    }

    #[test]
    fn the_rescue_runs_on_the_demo_marios_timers() {
        let ui = Ui::new(crate::UiKind::Smm1, mmk_ui::UiCache::in_memory(), None);
        let mut s = RescueScene::new(
            &ui,
            RescueInfo {
                difficulty: 1,
                ..Default::default()
            },
        );
        assert!(!s.has_models());
        let mut walked = None;
        let mut done = None;
        for t in 1..=1200 {
            let end = s.step();
            if walked.is_none() && s.state == RescueState::TextInWait {
                walked = Some(t);
            }
            if end {
                done = Some(t);
                break;
            }
        }
        // 30 standing, 278 walking (−32 → 384 at 1.5), 30, the balloon's 360, 120
        assert_eq!(walked, Some(30 + 278));
        assert_eq!(s.mario_x(), MARIO_STOP);
        assert_eq!(s.camera(), [384.0, 108.0]);
        assert_eq!(done, Some(30 + 278 + 30 + 360 + 120));
        // NSMBU with Peach holds 300 before the credits
        let mut s = RescueScene::new(
            &ui,
            RescueInfo {
                style: 3,
                difficulty: 2,
                ..Default::default()
            },
        );
        let n = (1..2000).find(|_| s.step()).unwrap();
        assert_eq!(n, 30 + 278 + 30 + 360 + 300);
        assert_eq!(s.camera()[0], 384.0);
    }

    /// The owner's import from importer 5 on (skipped without): the stage, Peach and Mario are
    /// drawn, the camera starts at the left bound with Mario off screen.
    #[test]
    fn owner_rescue_scene_draws_its_models() {
        let Some(ui) = smm1() else { return };
        let mut s = RescueScene::new(
            &ui,
            RescueInfo {
                difficulty: 2,
                form: 1,
                ..Default::default()
            },
        );
        if !s.has_models() {
            eprintln!("SMM1 import without the ending (importer < 5); skipping");
            return;
        }
        assert_eq!(s.camera(), [192.0, 108.0]);
        let mut out = Vec::new();
        s.draw(&mut out);
        let DrawItem::Layout { quads, .. } = &out[0] else {
            panic!("a layout draw");
        };
        // the clear colour, the ground (4 triangles), the sky (4), the wall (2), Peach (2), Mario (2)
        assert_eq!(quads.len(), 1 + 4 + 4 + 2 + 2 + 2, "{}", quads.len());
        let peach = &quads[quads.len() - 4];
        // Peach at x 416 − 8 in a camera at 192: (416 − 8 − 192) × 720 / 216
        let x0 = peach.pos.iter().map(|p| p[0]).fold(f32::MAX, f32::min);
        assert!((x0 - 216.0 * 720.0 / 216.0).abs() < 0.01, "{x0}");
    }
}
