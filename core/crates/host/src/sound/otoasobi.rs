//! The placed sound effects' sounds and music (`mmk_sim::otoasobi`; research/specs/
//! sound-effect-parts.md, private): the director steps its own copy of the course's
//! [`SoundParts`] after every tick and plays what it lists.
//! - a sound effect: its user's `cue` at the effect (`Otoasobi<id>`, the `_Link` / `_USA`
//!   version as Link or SMB2 Mario); long and own voices restart on their handle, and an own
//!   voice that holds tells the copy how long its sound is (the game asks the voice);
//! - a music effect: its `cue_bgm` (`cue_bgm_hurry` after hurry-up) in place of the theme track
//!   until it ends; then the theme track resumes where it was, as after the Super Star (the
//!   Otoasobi BGM layer 0x71017a6f20 above the Normal BGM layer 0x71017a5990, which seeks its
//!   saved place when the area, theme and hurry are unchanged), and the same music effect again
//!   resumes where it stopped (the layer's +0x54); Star and P Switch music still go over it;
//! - a music-like effect (Heartbeat, Silence, Peaceful, Horror): its `cue` as a held sound; the
//!   theme track plays on (the Otoasobi BGM layer neither plays nor claims the music for their
//!   ids, mask 0x6000000100400) — stopped when it ends;
//! - the audio filters (Twisty Turny, Woozy): the `OtoAsobi` bus's effects every frame as the
//!   audio manager 0x71017b15d0 runs them, and the user's `end` when one runs out.

use mmk_audio::oto::{OtoParams, OtoSettings};
use mmk_sim::otoasobi::{self as oto, SoundParts, Variant, Voice};

use super::*;

/// The long voice and the ambient effect.
const H_OTO_LONG: u32 = 6;
const H_OTO_AMBIENT: u32 = 7;
/// Own voices: this plus the effect id.
const H_OTO_OWN: u32 = 0xF000_0000;
/// Frames an ambient effect fades out over when it ends [inf].
const AMBIENT_FADE: f32 = 30.0;

/// Twisty Turny's band-pass: its centre starts at 60 Hz and sweeps between 50 and 5000 Hz,
/// across in a quarter of the run, Q 1.8, the bus at 2.5 [obs v3.0.3: 0x71017b15d0 (+0xa6c =
/// 60.0), the pair at 0x7102c565e4 set by 0x7100b68c60, 0x71023188b4, 0x71004d41f0(2.5)].
const SWEEP_START: f32 = 60.0;
const SWEEP: (f32, f32) = (50.0, 5000.0);
const FILTER_Q: f32 = 1.8;
const FILTER_GAIN: f32 = 2.5;

/// Woozy's warble: a level from 0 to 1 sets the vibrato's rate (0.5–15 Hz, depth 0.02 s) and
/// the tremolo's depth (0.35–0.8) and rate (800–2000 Hz); it rises over 18 frames through the
/// curve at 0x7102964de4 when a player (re)enters, and falls back by 0.01 a frame [obs v3.0.3:
/// 0x71017b15d0, the pairs at 0x7102c56608…0x7102c56620, 0x7102319268, 0x7102318c0c; the
/// Hermite curve 0x710029b460].
const WOOZY_VIB_RATE: (f32, f32) = (0.5, 15.0);
const WOOZY_VIB_DEPTH: f32 = 0.02;
const WOOZY_TREM_DEPTH: (f32, f32) = (0.35, 0.8);
const WOOZY_TREM_RATE: (f32, f32) = (800.0, 2000.0);
const WOOZY_RISE: f32 = 0.055_555_556;
const WOOZY_FALL: f32 = 0.01;
/// The rise's curve: four (value, slope) keys.
const WOOZY_CURVE: [(f32, f32); 4] = [
    (0.0, 0.0),
    (0.55, 0.916_381_3),
    (0.9, -0.425_306_7),
    (0.0, 0.0),
];

/// Woozy's rise (the game's Hermite segments over its keys, 0x710029b460).
fn woozy_curve(t: f32) -> f32 {
    if t < 0.0 {
        return WOOZY_CURVE[0].0;
    }
    let segs = (WOOZY_CURVE.len() - 1) as f32;
    let s = segs * t;
    let i = s as i32;
    if i >= segs as i32 {
        return WOOZY_CURVE[i.min(segs as i32) as usize].0;
    }
    let u = s - i as f32;
    let (p0, m0) = WOOZY_CURVE[i as usize];
    let (p1, m1) = WOOZY_CURVE[i as usize + 1];
    let h00 = u * (u + u) * u - u * (u * 3.0) + 1.0;
    let h01 = u * (u * 3.0) + u * (u * (u * -2.0));
    let h10 = u + (u * (u * u) - u * (u + u));
    let h11 = u * (u * u) - u * u;
    h00 * p0 + h01 * p1 + h10 * m0 + h11 * m1
}

fn lerp((a, b): (f32, f32), t: f32) -> f32 {
    a + t * (b - a)
}

/// Twisty Turny's sweep.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Sweep {
    centre: f32,
    step: f32,
}

/// Woozy's warble: its level, rising, and whether the last frame had a player entering.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Warble {
    level: f32,
    rising: bool,
    entering: bool,
}

/// The course's effects as this director sees them.
#[derive(Debug, Default)]
pub(super) struct OtoState {
    parts: Option<SoundParts>,
    music: Option<oto::Music>,
    variant: Variant,
    /// The music effect's place when it last stopped: its slot and source frame.
    music_saved: Option<(u8, u32)>,
    sweep: Option<Sweep>,
    warble: Option<Warble>,
    woozy_first: bool,
    bus_on: bool,
}

impl Sounds {
    /// The area's theme changed (a course start, an area change): its tempo for the filters.
    pub(super) fn oto_area(&mut self, world: &World) {
        let tempo = self.tempo(world.level.theme);
        if let Some(p) = &mut self.oto.parts {
            p.set_tempo(world.area, tempo);
        }
    }

    /// A course was loaded or restarted: its effects from the start.
    pub(super) fn oto_start(&mut self, world: &World) {
        self.stop_oto_ambient(0);
        if self.oto.bus_on {
            self.send_oto_bus(None, None);
        }
        let mut parts = world.sound_parts.any().then(|| world.sound_parts.clone());
        if let Some(p) = &mut parts {
            p.set_tempo(world.area, self.tempo(world.level.theme));
        }
        self.oto = OtoState {
            parts,
            ..OtoState::default()
        };
    }

    /// The course music's tempo in this style and `theme`, normal and hurried.
    fn tempo(&self, theme: u8) -> [f32; 2] {
        self.lib
            .db
            .tempo
            .get(&self.style)
            .and_then(|row| row.get(theme as usize))
            .copied()
            .unwrap_or([oto::DEFAULT_BPM; 2])
    }

    /// The course's tempos and own voices' ends, for the picture director's copy.
    pub(crate) fn oto_sync(&self) -> Option<mmk_sim::otoasobi::Sync> {
        let p = self.oto.parts.as_ref()?;
        Some(mmk_sim::otoasobi::Sync {
            tempo: p.tempos(),
            voices: p.voice_ends_all(),
        })
    }

    fn oto_variant(world: &World) -> Variant {
        match world.player.form {
            Form::Link => Variant::Link,
            Form::Smb2 => Variant::Usa,
            _ => Variant::Plain,
        }
    }

    /// The music effect's user, key and where it starts when one is in the theme track's
    /// place.
    pub(super) fn oto_music_key(&self) -> Option<(String, &'static str, u32)> {
        let m = self.oto.music.filter(|m| !m.ambient)?;
        let key = if self.hurry {
            "cue_bgm_hurry"
        } else {
            "cue_bgm"
        };
        let start = match self.oto.music_saved {
            Some((slot, frame)) if slot == m.slot => frame / RESUME_ALIGN * RESUME_ALIGN,
            _ => 0,
        };
        Some((oto::user(m.id, self.oto.variant), key, start))
    }

    fn stop_oto_ambient(&mut self, fade: u32) {
        self.send(Command::StopHandle {
            handle: H_OTO_AMBIENT,
            fade,
        });
    }

    /// Where the music voice is in the music effect `m`'s track (its first wave), if it plays.
    fn oto_music_frame(&self, m: &oto::Music) -> Option<u32> {
        let user = oto::user(m.id, self.oto.variant);
        let key = if self.hurry {
            "cue_bgm_hurry"
        } else {
            "cue_bgm"
        };
        let mut waves = Vec::new();
        self.lib.db.waves_of(&user, key, &mut waves);
        let id = *self.ids.get(waves.first()?)?;
        let v = self.clock.load(Ordering::Relaxed);
        (v != u64::MAX && (v >> 32) as u16 == id).then_some(v as u32)
    }

    /// The `OtoAsobi` bus's settings from the sound data (None: the data has none).
    fn oto_settings(&self) -> Option<OtoSettings> {
        let bus = &self.lib.db.otoasobi_bus;
        let delay = bus.get("Otoasobi_Delay")?;
        let d = |k: &str, v: f32| delay.get(k).copied().unwrap_or(v);
        Some(OtoSettings {
            delay_ms: d("DelayTime", 0.0),
            in_gain: d("InGain", 0.0),
            feedback: d("FeedbackGain", 0.0),
            dry: d("DryGain", 1.0),
            spread: d("ChannelSpread", 0.0),
            lpf: d("LowPassAmount", 0.0),
            vib_depth_max: bus
                .get("OtoAsobi_Vib")
                .and_then(|v| v.get("DepthMax"))
                .copied()
                .unwrap_or(WOOZY_VIB_DEPTH),
        })
    }

    fn send_oto_bus(&mut self, filter: Option<f32>, woozy: Option<[f32; 4]>) {
        let Some(settings) = self.oto_settings() else {
            return;
        };
        self.oto.bus_on = filter.is_some() || woozy.is_some();
        let woozy_first = self.oto.woozy_first;
        self.send(Command::SetOtoasobi {
            settings,
            params: OtoParams {
                filter: filter.map(|c| (c, FILTER_Q)),
                gain: if filter.is_some() { FILTER_GAIN } else { 1.0 },
                woozy,
                woozy_first,
            },
        });
    }

    /// One frame of the audio filters: Twisty Turny's sweep, Woozy's warble, the bus's effects;
    /// `end` for one that ran out (0x71017b15d0, 0x7100e443c0).
    fn oto_filters(&mut self, world: &World, parts: &SoundParts) {
        let running = |id: u8| parts.filters.iter().find(|f| f.id == id).copied();
        // Twisty Turny: from 60 Hz, a quarter of the run across, back and forth
        let twisty = running(46).map(|f| {
            let mut s = self.oto.sweep.unwrap_or(Sweep {
                centre: SWEEP_START,
                step: (SWEEP.1 - SWEEP.0) / f.left.max(1) as f32 * 4.0,
            });
            s.centre += s.step;
            let (lo, hi) = SWEEP;
            if s.step > 0.0 && s.centre >= hi {
                (s.step, s.centre) = (-s.step, hi);
            } else if s.step <= 0.0 && s.centre <= lo {
                (s.step, s.centre) = (-s.step, lo);
            }
            s
        });
        // Woozy: its level rises when a player (re)enters, falls back otherwise
        let woozy = running(47).map(|_| {
            let mut w = self.oto.warble.unwrap_or_default();
            if parts.entered && !w.entering {
                w.rising = true;
            }
            w.entering = parts.entered;
            let t = if w.rising {
                w.level = (w.level + WOOZY_RISE).clamp(0.0, 1.0);
                if w.level >= 1.0 {
                    w.rising = false;
                }
                woozy_curve(w.level)
            } else {
                let next = w.level - WOOZY_FALL;
                w.level = if next <= 0.0 || w.level < next {
                    0.0
                } else {
                    next
                };
                w.level
            };
            (w, t)
        });
        // the bus runs its effects in the order they were put on; on one frame the filter's
        // go first (0x71017b15d0 handles its request before Woozy's)
        let twisty_new = twisty.is_some() && self.oto.sweep.is_none();
        let woozy_new = woozy.is_some() && self.oto.warble.is_none();
        if twisty_new && !woozy_new && woozy.is_some() {
            self.oto.woozy_first = true;
        } else if woozy_new && twisty.is_some() {
            self.oto.woozy_first = false;
        }
        self.oto.sweep = twisty;
        self.oto.warble = woozy.map(|(w, _)| w);
        let params = woozy.map(|(_, t)| {
            [
                WOOZY_VIB_DEPTH,
                lerp(WOOZY_VIB_RATE, t),
                lerp(WOOZY_TREM_DEPTH, t),
                lerp(WOOZY_TREM_RATE, t),
            ]
        });
        if twisty.is_some() || params.is_some() || self.oto.bus_on {
            self.send_oto_bus(twisty.map(|s| s.centre), params);
        }
        for &id in &parts.filters_ended {
            let user = oto::user(id, self.oto.variant);
            let e = Self::player_emitter(world);
            self.sfx_at(world, &user, "end", &PropList::default(), e, 0);
        }
    }

    /// After a tick: the effects set off, the filters and the music effect's changes.
    pub(super) fn oto_step(&mut self, world: &World) {
        let Some(mut parts) = self.oto.parts.take() else {
            return;
        };
        parts.step(world);
        self.oto.variant = Self::oto_variant(world);
        for cue in parts.cues.clone() {
            let (handle, hold) = match oto::VOICES[cue.id as usize] {
                Voice::Music(_) | Voice::Ambient(_) => continue,
                Voice::Long => (H_OTO_LONG, false),
                Voice::Own { hold } => (H_OTO_OWN + cue.id as u32, hold),
                Voice::Shared => (0, false),
            };
            if handle != 0 {
                self.send(Command::StopHandle { handle, fade: 0 });
            }
            let user = oto::user(cue.id, self.oto.variant);
            let e = Emitter {
                x: cue.x,
                y: cue.y,
                water: false,
            };
            let len = self.sfx_at_len(world, &user, "cue", &PropList::default(), e, handle);
            if hold && let Some(len) = len {
                let end = if len.is_finite() {
                    world.tick + len.ceil() as u64
                } else {
                    u64::MAX
                };
                parts.voice_ends(cue.id, end);
            }
        }
        self.oto_filters(world, &parts);
        let (old, new) = (self.oto.music, parts.music);
        self.oto.parts = Some(parts);
        if old.map(|m| (m.id, m.slot)) == new.map(|m| (m.id, m.slot)) {
            self.oto.music = new;
            return;
        }
        // the music effect that stops keeps its place; the theme track's is kept as it goes
        if let Some(m) = old.filter(|m| !m.ambient) {
            if let Some(f) = self.oto_music_frame(&m) {
                self.oto.music_saved = Some((m.slot, f));
            }
        } else if new.is_some_and(|m| !m.ambient) {
            self.save_position();
        }
        self.oto.music = new;
        if old.is_some_and(|m| m.ambient) {
            let fade = self.frames(AMBIENT_FADE);
            self.stop_oto_ambient(fade);
        }
        if let Some(m) = new.filter(|m| m.ambient) {
            let user = oto::user(m.id, self.oto.variant);
            let e = Emitter {
                x: m.x,
                y: m.y,
                water: false,
            };
            self.sfx_at(world, &user, "cue", &PropList::default(), e, H_OTO_AMBIENT);
        }
        let music_changed = old.is_some_and(|m| !m.ambient) || new.is_some_and(|m| !m.ambient);
        if music_changed && world.player.alive() && self.over == Over::None && !self.fanfare {
            self.course_music();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn woozy_s_rise_follows_the_game_s_curve() {
        assert_eq!(woozy_curve(0.0), 0.0);
        // the keys at thirds
        assert!((woozy_curve(1.0 / 3.0) - 0.55).abs() < 1e-5);
        assert!((woozy_curve(2.0 / 3.0) - 0.9).abs() < 1e-5);
        assert_eq!(woozy_curve(1.0), 0.0, "the last key");
        assert!(woozy_curve(0.5) > 0.55 && woozy_curve(0.5) < 1.0);
    }
}
