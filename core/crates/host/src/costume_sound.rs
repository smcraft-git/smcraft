//! SMM1 costume sounds (Phase 11): while the player wears a Mystery Mushroom costume, its own
//! sounds (`costume_<id>_<event>.bwav` of the SMM1 sprite cache's `audio/`) replace the jump,
//! power-up, death and clear sounds, and play its landing, skid and taunt sounds. Lanky Mario
//! (the Weird Mushroom) has his jump, his float cue and his growth (`lanky_<event>.bwav`). Both
//! directors load the sounds into the course's sound set when the form is put on.

use std::sync::Arc;

use mmk_audio::Sound;
use mmk_sim::World;
use mmk_sim::player::Form;

/// Reads `<name>.bwav` of the SMM1 sprite cache's `audio/`.
pub type CostumeReader = Arc<dyn Fn(&str) -> Option<Vec<u8>> + Send + Sync>;

/// SMM1 costume sound events (SMM1 event enum "down, goal, jump, powerup, appeal, ground,
/// slip"; `jump2` is an alternate take).
pub const COSTUME_EVENTS: [&str; 8] = [
    "jump", "jump2", "powerup", "appeal", "ground", "slip", "down", "goal",
];

/// The worn costume's sounds (ids in the mixer once loaded).
#[derive(Default)]
pub struct CostumeSounds {
    read: Option<CostumeReader>,
    loaded: Option<u16>,
    ids: [Option<u16>; 8],
    jump_alt: bool,
    /// Lanky Mario's sounds once loaded (jump, float, powerup).
    lanky: Option<[Option<u16>; 3]>,
}

/// Lanky Mario's events and their sounds in the SMM1 sprite cache's `audio/`.
const LANKY_EVENTS: [(&str, &str); 3] = [
    ("jump", "lanky_jump"),
    ("float", "lanky_float"),
    ("powerup", "lanky_powerup"),
];

impl CostumeSounds {
    pub fn new(read: Option<CostumeReader>) -> Self {
        Self {
            read,
            ..Self::default()
        }
    }

    /// The mixer's sounds were replaced (a new sound set): load again when next worn.
    pub fn forget(&mut self) {
        self.loaded = None;
        self.ids = [None; 8];
        self.lanky = None;
    }

    /// Load the worn costume's sounds when it changes; `add` puts a sound into the mixer and
    /// returns its id.
    pub fn sync(&mut self, world: &World, add: &mut dyn FnMut(Sound) -> Option<u16>) {
        if world.player.form == Form::Lanky
            && self.lanky.is_none()
            && let Some(read) = &self.read
        {
            self.lanky = Some(LANKY_EVENTS.map(|(_, name)| {
                read(name)
                    .and_then(|bytes| Sound::from_bwav(name, &bytes).ok())
                    .and_then(&mut *add)
            }));
        }
        let want = (world.player.form == Form::Costume).then_some(world.player.costume);
        if want.is_none() || want == self.loaded {
            return;
        }
        let Some(read) = &self.read else { return };
        let id = want.unwrap_or(0);
        for (k, e) in COSTUME_EVENTS.iter().enumerate() {
            let name = format!("costume_{id:03}_{e}");
            self.ids[k] = read(&name)
                .and_then(|bytes| Sound::from_bwav(&name, &bytes).ok())
                .and_then(&mut *add);
        }
        self.loaded = want;
    }

    /// The worn costume's sound for `event` (None: not costumed, or the costume lacks it).
    pub fn sound(&self, world: &World, event: &str) -> Option<u16> {
        if world.player.form == Form::Lanky {
            let k = LANKY_EVENTS.iter().position(|(e, _)| *e == event)?;
            return self.lanky.as_ref()?[k];
        }
        if world.player.form != Form::Costume || self.loaded != Some(world.player.costume) {
            return None;
        }
        let k = COSTUME_EVENTS.iter().position(|e| *e == event)?;
        self.ids[k]
    }

    /// The costume's jump: its second take every other jump.
    pub fn jump(&mut self, world: &World) -> Option<u16> {
        let first = self.sound(world, "jump")?;
        if world.player.form == Form::Lanky {
            return Some(first);
        }
        let alt = self.sound(world, "jump2").filter(|_| self.jump_alt);
        self.jump_alt = !self.jump_alt;
        Some(alt.unwrap_or(first))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lanky_mario_has_his_own_sounds() {
        // the owner's SMM1 import, when this machine has it (skipped elsewhere)
        let Some(dir) = crate::paths::smm1_ui().map(|d| d.join("sprites/audio")) else {
            return;
        };
        if !dir.join("lanky_jump.bwav").is_file() {
            return;
        }
        let read: CostumeReader =
            Arc::new(move |n: &str| std::fs::read(dir.join(format!("{n}.bwav"))).ok());
        let mut c = CostumeSounds::new(Some(read));
        let mut world = World::new();
        world.player.form = Form::Lanky;
        let mut next = 0u16;
        c.sync(&world, &mut |_| {
            next += 1;
            Some(next)
        });
        assert_eq!(c.sound(&world, "jump"), Some(1));
        assert_eq!(c.sound(&world, "float"), Some(2));
        assert_eq!(c.sound(&world, "powerup"), Some(3));
        assert_eq!(c.jump(&world), Some(1), "no alternate take");
        world.player.form = Form::Super;
        assert_eq!(c.sound(&world, "jump"), None);
    }
}
