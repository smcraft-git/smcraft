//! The placed sound effects' sounds and music (`mmk_sim::otoasobi`; research/specs/
//! sound-effect-parts.md, private): the director steps its own copy of the course's
//! [`SoundParts`] after every tick and plays what it lists.
//! - a sound effect: its user's `cue` at the effect (`Otoasobi<id>`, the `_Link` / `_USA`
//!   version as Link or SMB2 Mario); long and own voices restart on their handle;
//! - a music effect: its `cue_bgm` (`cue_bgm_hurry` after hurry-up) in place of the theme track
//!   until it ends (then the theme track from the top [inf]); Star and P Switch music still go
//!   over it and come back to it;
//! - a music-like effect (Heartbeat, Silence, Peaceful, Horror: on the window's Music page, with
//!   music slots of their own): its `cue` as a held sound in the music's place — the theme track
//!   stops while it lasts [inf: the game plays these slots on a sound holder, 0x71015378c0] —
//!   stopped when it ends.

use mmk_sim::otoasobi::{self as oto, SoundParts, Variant, Voice};

use super::*;

/// The long voice and the ambient effect.
const H_OTO_LONG: u32 = 6;
const H_OTO_AMBIENT: u32 = 7;
/// Own voices: this plus the effect id.
const H_OTO_OWN: u32 = 0xF000_0000;
/// Frames an ambient effect fades out over when it ends [inf].
const AMBIENT_FADE: f32 = 30.0;

/// The course's effects as this director sees them.
#[derive(Debug, Default)]
pub(super) struct OtoState {
    parts: Option<SoundParts>,
    music: Option<oto::Music>,
    variant: Variant,
}

impl Sounds {
    /// A course was loaded or restarted: its effects from the start.
    pub(super) fn oto_start(&mut self, world: &World) {
        self.stop_oto_ambient(0);
        self.oto = OtoState {
            parts: world.sound_parts.any().then(|| world.sound_parts.clone()),
            ..OtoState::default()
        };
    }

    fn oto_variant(world: &World) -> Variant {
        match world.player.form {
            Form::Link => Variant::Link,
            Form::Smb2 => Variant::Usa,
            _ => Variant::Plain,
        }
    }

    /// A music-like effect holds the music's place (the theme track stays off).
    pub(super) fn oto_ambient(&self) -> bool {
        self.oto.music.is_some_and(|m| m.ambient)
    }

    /// The music effect's user and key when one is in the theme track's place.
    pub(super) fn oto_music_key(&self) -> Option<(String, &'static str)> {
        let m = self.oto.music.filter(|m| !m.ambient)?;
        let key = if self.hurry {
            "cue_bgm_hurry"
        } else {
            "cue_bgm"
        };
        Some((oto::user(m.id, self.oto.variant), key))
    }

    fn stop_oto_ambient(&mut self, fade: u32) {
        self.send(Command::StopHandle {
            handle: H_OTO_AMBIENT,
            fade,
        });
    }

    /// After a tick: the effects set off and the music effect's changes.
    pub(super) fn oto_step(&mut self, world: &World) {
        let Some(mut parts) = self.oto.parts.take() else {
            return;
        };
        parts.step(world);
        self.oto.variant = Self::oto_variant(world);
        for cue in parts.cues.clone() {
            let handle = match oto::VOICES[cue.id as usize] {
                Voice::Music(_) | Voice::Ambient(_) => continue,
                Voice::Long => H_OTO_LONG,
                Voice::Own { .. } => H_OTO_OWN + cue.id as u32,
                Voice::Shared => 0,
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
            self.sfx_at(world, &user, "cue", &PropList::default(), e, handle);
        }
        let (old, new) = (self.oto.music, parts.music);
        self.oto.parts = Some(parts);
        if old.map(|m| (m.id, m.slot)) == new.map(|m| (m.id, m.slot)) {
            self.oto.music = new;
            return;
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
        if world.player.alive() && self.over == Over::None && !self.fanfare {
            self.course_music();
        }
    }
}
