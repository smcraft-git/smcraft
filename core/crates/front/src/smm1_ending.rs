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
    /// The bone it hangs on (it moves with it in a skeletal animation).
    bone: Option<String>,
    tris: Vec<[f32; 5]>,
}

/// A skeletal animation as the bones' origins per frame (importer 6).
#[derive(Clone, Debug, Default)]
struct Skeletal {
    frames: f32,
    looping: bool,
    bones: HashMap<String, Vec<[f32; 3]>>,
}

impl Skeletal {
    /// Bone `b`'s origin at `frame` (the nearest whole frame), if the animation has it.
    fn at(&self, b: &str, frame: f32) -> Option<[f32; 3]> {
        let f = if self.looping && self.frames > 0.0 {
            frame.rem_euclid(self.frames)
        } else {
            frame.clamp(0.0, self.frames.max(0.0))
        };
        let track = self.bones.get(b)?;
        track
            .get((f as usize).min(track.len().checked_sub(1)?))
            .copied()
    }
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
    /// Each model's bones' rest origins.
    bones: HashMap<String, HashMap<String, [f32; 3]>>,
    skeletal: HashMap<String, Skeletal>,
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
                        bone: s["bone"].as_str().map(str::to_string),
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
            let point = |v: &Value| -> [f32; 3] {
                std::array::from_fn(|k| v[k].as_f64().unwrap_or(0.0) as f32)
            };
            for (m, bones) in a["bones"].as_object().into_iter().flatten() {
                let bones = bones
                    .as_object()
                    .into_iter()
                    .flatten()
                    .map(|(b, p)| (b.clone(), point(p)))
                    .collect();
                arch.bones.insert(m.clone(), bones);
            }
            for (n, v) in a["skeletal"].as_object().into_iter().flatten() {
                let bones = v["bones"]
                    .as_object()
                    .into_iter()
                    .flatten()
                    .map(|(b, fr)| {
                        let fr = fr.as_array().into_iter().flatten().map(point).collect();
                        (b.clone(), fr)
                    })
                    .collect();
                arch.skeletal.insert(
                    n.clone(),
                    Skeletal {
                        frames: v["frames"].as_f64().unwrap_or(0.0) as f32,
                        looping: v["loop"].as_bool().unwrap_or(false),
                        bones,
                    },
                );
            }
            out.archives.insert(name.clone(), arch);
        }
        Some(out)
    }

    /// Bone `bone`'s rest origin in model `model` of `archive`.
    fn bone(&self, archive: &str, model: &str, bone: &str) -> Option<[f32; 3]> {
        self.archives
            .get(archive)?
            .bones
            .get(model)?
            .get(bone)
            .copied()
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
    /// In a Kuribo's Shoe (Some(stiletto)) at the goal.
    pub shoe: Option<bool>,
    /// On Yoshi at the goal.
    pub yoshi: bool,
    /// Wearing a shell helmet (1 Buzzy, 2 Spiny).
    pub helmet: Option<u8>,
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

/// The Kuribo's Shoe's hop (0x025eefbc, 0x025eed54): its start speed, gravity and fastest fall
/// (0x100ba7b0, 0x100ba7f8, 0x100ba7fc) [obs].
pub const HOP_SPEED: f32 = 2.5;
pub const HOP_GRAVITY: f32 = -0.34;
pub const HOP_MAX_FALL: f32 = -4.0;
/// The animation rate standing on Yoshi (0x025eef40: 1.2, else 1.0) [obs].
pub const YOSHI_WAIT_RATE: f32 = 1.2;
/// Small Mario sits this much higher in a shoe than the big forms: his shoe poses stand 8 lower
/// in their quad, the big ones' head starts on the shoe's top [inf: from the art].
const SHOE_SMALL_RAISE: f32 = 8.0;

/// What carries the demo Mario: SMB1 and SMB3 draw the Kuribo's Shoe; SMW (and the styles that
/// take its scene) Yoshi, also for a shoe (0x025ee86c: in MW / WU the shoe is cleared and Yoshi
/// used) [obs].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mount {
    None,
    Shoe { stiletto: bool },
    Yoshi,
}

pub fn mount(style: &str, info: &RescueInfo) -> Mount {
    match (style, info.shoe, info.yoshi) {
        ("MW", Some(_), _) | ("MW", _, true) => Mount::Yoshi,
        ("M1" | "M3", Some(stiletto), _) => Mount::Shoe { stiletto },
        _ => Mount::None,
    }
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
    /// Height above the ground (the shoe's hop).
    y: f32,
    vy: f32,
    /// The shoe stopped hopping (+0x1473).
    hop_done: bool,
    /// The fall's `shoes_jump` was set (+0x1471).
    falling: bool,
    pub mount: Mount,
    /// Mario's texture pattern, the tick it started and its rate.
    anim: (&'static str, i32, f32),
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
            y: 0.0,
            vy: 0.0,
            hop_done: false,
            falling: false,
            mount: mount(style, &info),
            anim: ("run", 0, RUN_RATE),
            balloon_out: false,
            cape: style == "MW" && info.form == 4,
            stopped_at: 0,
        }
    }

    /// Tests: the scene with these models.
    #[cfg(test)]
    pub(crate) fn with_models(mut self, m: EndingModels) -> Self {
        self.models = Some(m);
        self
    }

    /// The scene has its models (an SMM1 import from importer 5 on).
    pub fn has_models(&self) -> bool {
        self.models.is_some()
    }

    /// Mario's x (world units).
    pub fn mario_x(&self) -> f32 {
        self.x
    }

    /// Mario's height above the ground (a shoe's hop).
    pub fn mario_y(&self) -> f32 {
        self.y
    }

    /// Mario's texture pattern now.
    pub fn mario_anim(&self) -> &'static str {
        self.anim.0
    }

    fn set_anim(&mut self, name: &'static str, rate: f32) {
        self.anim = (name, self.tick, rate);
    }

    /// The shoe's hop, every tick of the walk and the stop (0x025eed54): gravity, the fall's
    /// `shoes_jump`, and on landing another hop while walking (`shoes_jumpup`), else
    /// `shoes_wait` and no more [obs].
    fn hop(&mut self) {
        if self.hop_done || !matches!(self.mount, Mount::Shoe { .. }) {
            return;
        }
        self.vy = (self.vy + HOP_GRAVITY).max(HOP_MAX_FALL);
        self.y += self.vy;
        if !self.falling && self.vy < 0.0 {
            self.falling = true;
            self.set_anim("shoes_jump", 1.0);
        }
        if self.y < 0.0 {
            self.y = 0.0;
            self.falling = false;
            if self.state == RescueState::Walk {
                self.vy = HOP_SPEED;
                self.set_anim("shoes_jumpup", 1.0);
            } else {
                self.set_anim("shoes_wait", 1.0);
                self.hop_done = true;
            }
        }
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
                    // PlayerWalk's enter: a shoe starts hopping (0x025eefbc) [obs]
                    if matches!(self.mount, Mount::Shoe { .. }) {
                        self.vy = HOP_SPEED;
                        self.falling = false;
                        self.set_anim("shoes_jump", 1.0);
                    }
                }
            }
            RescueState::Walk => {
                let stop = self.x + WALK_SPEED >= MARIO_STOP;
                self.x = if stop {
                    MARIO_STOP
                } else {
                    self.x + WALK_SPEED
                };
                self.hop();
                if stop {
                    self.stopped_at = t;
                    self.state = RescueState::TextInWait;
                    self.timer = TEXT_IN_WAIT;
                    // TextInWait's enter: `wait`, at 1.2 on Yoshi (0x025ef1c8, 0x025eef40)
                    let rate = if self.mount == Mount::Yoshi {
                        YOSHI_WAIT_RATE
                    } else {
                        1.0
                    };
                    self.set_anim("wait", rate);
                }
            }
            RescueState::TextInWait => {
                self.hop();
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
                self.hop();
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

    /// Model `model` of `archive` (its first without a name) at `at`, with texture pattern
    /// `pattern` at its frame and skeletal animation `skeletal` at its frame (its shapes move
    /// with their bones); back faces left out (the rigs' second quads face away).
    #[allow(clippy::too_many_arguments)]
    fn quads_of(
        &self,
        archive: &str,
        model: Option<&str>,
        pattern: Option<(&str, f32)>,
        skeletal: Option<(&str, f32)>,
        at: [f32; 2],
        out: &mut Vec<Quad>,
    ) {
        let Some(m) = &self.models else { return };
        let Some(a) = m.archives.get(archive) else {
            return;
        };
        // a player archive holds one model
        let (name, shapes) = match model {
            Some(name) => match a.models.get_key_value(name) {
                Some(e) => e,
                None => return,
            },
            None => match a.models.iter().next() {
                Some(e) => e,
                None => return,
            },
        };
        let skel = skeletal.and_then(|(n, f)| a.skeletal.get(n).map(|s| (s, f)));
        let rest = a.bones.get(name);
        let cam = self.camera();
        let s = 720.0 / VIEW[1];
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
            // the bone's move from its rest origin
            let d = sh
                .bone
                .as_deref()
                .and_then(|b| {
                    let (sk, f) = skel?;
                    let now = sk.at(b, f)?;
                    let was = rest?.get(b)?;
                    Some([now[0] - was[0], now[1] - was[1]])
                })
                .unwrap_or([0.0, 0.0]);
            let place = |v: &[f32; 5]| {
                [
                    (v[0] + d[0] + at[0] - cam[0]) * s,
                    (v[1] + d[1] + at[1] - cam[1]) * s,
                ]
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
                let (pa, pb, pc) = (place(a), place(b), place(c));
                // counter-clockwise faces the camera (y up)
                let area = (pb[0] - pa[0]) * (pc[1] - pa[1]) - (pc[0] - pa[0]) * (pb[1] - pa[1]);
                if area <= 0.0 {
                    continue;
                }
                let z = a[2].max(b[2]).max(c[2]);
                drawn.push((
                    z,
                    Quad {
                        pos: [pa, pb, pc, pc],
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

    /// The frame of Mario's pattern now.
    fn anim_frame(&self) -> f32 {
        (self.tick - self.anim.1) as f32 * self.anim.2
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
        self.quads_of(
            &stage,
            Some("field_model"),
            None,
            None,
            FIELD_AT,
            &mut quads,
        );
        // Peach / Toad: their wait patterns only in SMW and NSMBU (0x025ec70c) [obs]
        let (model, anim) = if self.peach {
            ("peach_model", "wait_peach")
        } else {
            ("kinopio_model", "wait_kinopio")
        };
        let animated = self.style == "MW";
        let pat = animated.then_some((anim, self.tick as f32));
        self.quads_of(&stage, Some(model), pat, None, PEACH_AT, &mut quads);
        let walking = matches!(self.state, RescueState::StartWait | RescueState::Walk);
        let mut at = [self.x, MARIO_START[1] + self.y];
        let frame = self.anim_frame();
        let mut anim = self.anim.0.to_string();
        if self.mount == Mount::Yoshi {
            // Yoshi plays the move by its own names (`run_body`, `wait_body`, the head's
            // `wait_head`) and carries Mario on his `mario` bone, who plays the rider's
            // (`yoshi_run`, `yoshi_wait`) [inf: the names; 0x026e8368 hands the move to Yoshi]
            let body = if walking { "run_body" } else { "wait_body" };
            let models = self.models.as_ref();
            let bone = |b: &str| {
                models
                    .and_then(|m| m.archives.get(YOSHI))
                    .and_then(|a| a.skeletal.get(body))
                    .and_then(|s| s.at(b, frame))
                    .or_else(|| models.and_then(|m| m.bone(YOSHI, YOSHI_BODY, b)))
                    .unwrap_or([0.0; 3])
            };
            let (head, rider) = (bone("head"), bone("mario"));
            self.quads_of(
                YOSHI,
                Some(YOSHI_BODY),
                Some((body, frame)),
                Some((body, frame)),
                at,
                &mut quads,
            );
            self.quads_of(
                YOSHI,
                Some(YOSHI_HEAD),
                Some(("wait_head", frame)),
                None,
                [at[0] + head[0], at[1] + head[1]],
                &mut quads,
            );
            at = [at[0] + rider[0], at[1] + rider[1]];
            anim = if walking { "yoshi_run" } else { "yoshi_wait" }.to_string();
        }
        if let Mount::Shoe { .. } = self.mount
            && self.info.form == 0
        {
            at[1] += SHOE_SMALL_RAISE;
        }
        if self.cape {
            // the cape over Super Mario: `mt_walk` while he walks, `mt_down` when he stops,
            // then `mt_wait` / on Yoshi `mt_yoshi_wait` (0x025ef1c8, 0x025eeebc) [obs]; drawn
            // behind him at his feet [inf]
            let cape = if walking {
                ("mt_walk", frame)
            } else {
                let since = (self.tick - self.stopped_at) as f32;
                let down = self.pattern(CAPE, "mt_down").map_or(0.0, |p| p.frames);
                if since < down {
                    ("mt_down", since)
                } else if self.mount == Mount::Yoshi {
                    ("mt_yoshi_wait", 0.0)
                } else {
                    ("mt_wait", 0.0)
                }
            };
            self.quads_of(CAPE, None, Some(cape), None, at, &mut quads);
        }
        let player = self.player.clone();
        self.quads_of(&player, None, Some((&anim, frame)), None, at, &mut quads);
        let feet = [self.x, MARIO_START[1] + self.y];
        if let Mount::Shoe { stiletto } = self.mount {
            // the shoe over his legs, its wait blinking [inf: drawn in front]
            let (arch, model) = if stiletto {
                (
                    format!("{}_Enemy_kutsu_kuriboB", self.style),
                    "kutsu_kuriboB_model",
                )
            } else {
                (
                    format!("{}_Enemy_kutsu_kuriboA", self.style),
                    "kutsu_kuriboA_model",
                )
            };
            let blink = (!self.hop_done).then_some(("wait", self.tick as f32));
            let blink = blink.or(Some(("wait", 0.0)));
            self.quads_of(&arch, Some(model), blink, None, feet, &mut quads);
        }
        if let Some(h) = self.info.helmet {
            // the shell on his head bone (`met` / `togezo`), the large one on the big forms
            // [inf: L / M by size]; its wobble (0x025eec94) is under a unit, left out
            let (kind, bone) = if h == 2 {
                ("Togezo", "togezo")
            } else {
                ("Met", "met")
            };
            let arch = format!("{}_Player_{kind}_model", self.style);
            let pm = self.models.as_ref().and_then(|m| {
                let a = m.archives.get(&self.player)?;
                let (name, _) = a.models.iter().next()?;
                m.bone(&self.player, name, bone)
            });
            if let Some(p) = pm {
                let size = if self.info.form == 0 { "M" } else { "L" };
                let model = format!("Player_{kind}{size}_model");
                self.quads_of(
                    &arch,
                    Some(&model),
                    None,
                    None,
                    [at[0] + p[0], at[1] + p[1]],
                    &mut quads,
                );
            }
        }
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

/// SMW's Yoshi and his two models.
const YOSHI: &str = "MW_Player_Yoshi";
const YOSHI_BODY: &str = "Player_Yoshi_Body_model";
const YOSHI_HEAD: &str = "Player_Yoshi_Head_model";

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

    #[test]
    fn what_carries_him_goes_by_style() {
        let shoe = RescueInfo {
            shoe: Some(true),
            ..Default::default()
        };
        assert_eq!(mount("M1", &shoe), Mount::Shoe { stiletto: true });
        assert_eq!(mount("MW", &shoe), Mount::Yoshi, "SMW's shoe is Yoshi");
        let yoshi = RescueInfo {
            yoshi: true,
            ..Default::default()
        };
        assert_eq!(mount("MW", &yoshi), Mount::Yoshi);
        assert_eq!(mount("M3", &yoshi), Mount::None);
        assert_eq!(mount("M1", &RescueInfo::default()), Mount::None);
    }

    /// In a shoe he hops all the way (2.5 up, −0.34 a tick, −4 at most), each landing on the
    /// walk hops again (`shoes_jumpup`), the one after it stays (`shoes_wait`); the walk and its
    /// timers don't change [obs 0x025eed54].
    #[test]
    fn the_shoe_hops_to_peach() {
        let ui = Ui::new(crate::UiKind::Smm1, mmk_ui::UiCache::in_memory(), None);
        let mut s = RescueScene::new(
            &ui,
            RescueInfo {
                difficulty: 1,
                shoe: Some(false),
                ..Default::default()
            },
        );
        let (mut hops, mut top, mut done) = (0, 0.0f32, None);
        let mut last = s.mario_anim();
        for t in 1..2000 {
            let end = s.step();
            if s.mario_anim() == "shoes_jumpup" && last != "shoes_jumpup" {
                hops += 1;
            }
            last = s.mario_anim();
            top = top.max(s.mario_y());
            if end {
                done = Some(t);
                break;
            }
        }
        // one hop: 2.16, 1.82, … up (gravity first: 8 high, 15 ticks); 278 ticks of walk
        assert!((7.9..8.0).contains(&top), "{top}");
        assert!((15..=20).contains(&hops), "{hops}");
        assert_eq!(s.mario_anim(), "shoes_wait");
        assert_eq!(s.mario_y(), 0.0);
        assert_eq!(done, Some(30 + 278 + 30 + 360 + 120));
    }

    /// A synthetic Yoshi (importer 6's form): his body and head, and Mario on his `mario` bone
    /// as his run moves it.
    #[test]
    fn mario_rides_on_yoshis_bone() {
        let quad = |w: f32, h: f32, z: f32| {
            serde_json::json!([
                [-w, 0.0, z, 0.0, 1.0],
                [w, 0.0, z, 1.0, 1.0],
                [-w, h, z, 0.0, 0.0],
                [w, 0.0, z, 1.0, 1.0],
                [w, h, z, 1.0, 0.0],
                [-w, h, z, 0.0, 0.0]
            ])
        };
        let doc = serde_json::json!({ "archives": {
            "MW_Player_Yoshi": {
                "models": {
                    "Player_Yoshi_Body_model": [{ "material": "m", "texture": "body", "bone": "body", "v": quad(16.0, 32.0, 0.0) }],
                    "Player_Yoshi_Head_model": [{ "material": "m", "texture": "head", "bone": "texL", "v": quad(16.0, 32.0, 0.6) }],
                },
                "bones": { "Player_Yoshi_Body_model": { "body": [5.0, 0.0, 0.0], "head": [5.0, 0.0, 0.0], "mario": [5.0, 0.0, 0.0] } },
                "skeletal": { "run_body": { "frames": 12, "loop": true, "bones": {
                    "body": vec![[5.0, 0.0, 0.0]; 13], "head": vec![[5.0, -1.0, 0.0]; 13],
                    "mario": vec![[-2.0, 9.0, 1.0]; 13] } } },
            },
            "MW_Player_SpMarioMdl": {
                "models": { "Su_mario_model": [{ "material": "m", "texture": "yoshi_run.0", "bone": "body", "v": quad(16.0, 32.0, 0.0) }] },
            },
        }});
        let models = EndingModels::from_json(&doc).unwrap();
        let mut cache = mmk_ui::UiCache::in_memory();
        let tex = || mmk_assets::Mtex {
            format: mmk_assets::MtexFormat::Rgba8,
            variant: mmk_assets::MtexVariant::Srgb,
            width: 1,
            height: 1,
            channel_select: [2, 3, 4, 5],
            data: vec![255; 4],
        };
        for (g, t) in [
            ("MW_Player_Yoshi", "body"),
            ("MW_Player_Yoshi", "head"),
            ("MW_Player_SpMarioMdl", "yoshi_run.0"),
        ] {
            cache.add_texture_file(g, t, tex());
        }
        let ui = Ui::new(crate::UiKind::Smm1, cache, None);
        let mut s = RescueScene::new(
            &ui,
            RescueInfo {
                style: 2,
                form: 1,
                yoshi: true,
                difficulty: 1,
                ..Default::default()
            },
        )
        .with_models(models);
        assert_eq!(s.mount, Mount::Yoshi);
        for _ in 0..200 {
            s.step();
        }
        let mut out = Vec::new();
        s.draw(&mut out);
        let DrawItem::Layout { quads, .. } = &out[0] else {
            panic!("a layout draw");
        };
        // the clear colour, Yoshi's body and head, Mario: 2 triangles each
        assert_eq!(quads.len(), 1 + 2 + 2 + 2, "{}", quads.len());
        let bottom = |q: &Quad| q.pos.iter().map(|p| p[1]).fold(f32::MAX, f32::min);
        let px = 720.0 / VIEW[1];
        let feet = (MARIO_START[1] - s.camera()[1]) * px;
        // the body on the ground, the head 1 down, Mario 9 up on the saddle
        assert!((bottom(&quads[1]) - feet).abs() < 0.01);
        assert!((bottom(&quads[3]) - (feet - px)).abs() < 0.01);
        assert!((bottom(&quads[5]) - (feet + 9.0 * px)).abs() < 0.01);
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
