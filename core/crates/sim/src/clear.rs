//! Clear conditions (clear-conditions.md, themes.md §6): the course header names a condition by
//! the CRC32 of an entry in the game's condition list; the goal only accepts the player once the
//! condition is met.
//!
//! Three families: count conditions (defeat / collect / use N, latched when the count reaches N;
//! each placed object counts once), status conditions (reach the goal in a form, invincible, …:
//! checked when the goal is touched) and action conditions (reach the goal without taking damage /
//! landing: violated for the rest of the attempt by the first offence).

/// Index in the game's condition list (1-based), from the header's CRC32 (the formats crate's
/// table: v1.0.0's 77 names, then the 21 v3.0.3 adds); None for 0 or an unknown hash.
pub fn index_of(hash: u32) -> Option<u8> {
    if hash == 0 {
        return None;
    }
    mmk_formats::smm2::clear::index(hash)
}

/// Enemy classes of the defeat conditions (list indices 1–41). Only the ones the engine has.
pub mod enemy {
    pub const GOOMBA: u8 = 1;
    pub const KOOPA: u8 = 3;
    pub const BUZZY: u8 = 4;
    pub const SPINY: u8 = 5;
    pub const PIRANHA: u8 = 6;
    pub const FIRE_PIRANHA: u8 = 7;
    pub const MUNCHER: u8 = 8;
    pub const DRY_BONES: u8 = 10;
    pub const FISH_BONE: u8 = 11;
    pub const ROCKY_WRENCH: u8 = 12;
    pub const MONTY_MOLE: u8 = 13;
    pub const HAMMER_BRO: u8 = 14;
    pub const LAKITU: u8 = 16;
    pub const BLOOPER: u8 = 17;
    pub const CHEEP_CHEEP: u8 = 18;
    pub const SPIKE_TOP: u8 = 19;
    pub const LAVA_BUBBLE: u8 = 21;
    pub const CHAIN_CHOMP: u8 = 22;
    pub const WIGGLER: u8 = 23;
    pub const BOB_OMB: u8 = 24;
    pub const THWOMP: u8 = 25;
    pub const BOO: u8 = 26;
    pub const BANZAI_BILL: u8 = 28;
    pub const MAGIKOOPA: u8 = 29;
    pub const ANGRY_SUN: u8 = 30;
    pub const BOWSER: u8 = 37;
    pub const BOWSER_JR: u8 = 38;
    pub const BOOM_BOOM: u8 = 39;
    pub const POM_POM: u8 = 40;
    pub const BULLET_BILL: u8 = 41;
    /// The update's (v3.0.3 list, our 78–87).
    pub const SPIKE: u8 = 78;
    pub const POKEY: u8 = 79;
    pub const MECHAKOOPA: u8 = 80;
    /// Larry, Iggy, Wendy, Lemmy, Roy, Morton, Ludwig.
    pub const KOOPALINGS: [u8; 7] = [81, 82, 83, 84, 85, 86, 87];
}

/// The player's state when touching the goal (status conditions).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AtGoal {
    pub star: bool,
    /// 0 small, 1 super, 2 fire.
    pub form: u8,
    pub shoe: bool,
    pub clown_car: bool,
    pub koopa_car: bool,
    /// Standing on a Lakitu's Cloud.
    pub cloud: bool,
    /// In a Dry Bones Shell; wearing a helmet (0 none, 1 Buzzy, 2 Spiny).
    pub karon_shell: bool,
    pub helmet: u8,
    /// Carrying the Story Mode stone.
    pub holding_stone: bool,
    /// Carrying a Koopa shell.
    pub holding_shell: bool,
    pub yoshi: bool,
    /// Wearing a wearable: 0 none, else its kind + 1 (0 Propeller Box, 1 Cannon Box, 2 Goomba
    /// Mask, 3 Red POW Box, 4 Bullet Bill Mask).
    pub wearing: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Goal {
    None,
    /// Defeat N enemies of a class (list index).
    Defeat(u8),
    /// Collect N 1-Up Mushrooms.
    OneUps,
    /// Collect N coins weighted by value (0 = any coin), or N coins of one value (10/30/50).
    Coins(u32),
    /// Use N POW blocks, press N P Switches.
    Pow,
    PSwitch,
    /// Reach the goal invincible / as Super / as Fire.
    Star,
    Super,
    Fire,
    Raccoon,
    Cape,
    Propeller,
    Cat,
    Builder,
    Superball,
    Mega,
    /// Reach the goal wearing a Goomba's Shoe / flying a Koopa Clown Car.
    Shoe,
    ClownCar,
    /// Reach the goal driving a Koopa Troopa Car / standing on a Lakitu's Cloud / carrying the
    /// Story Mode stone.
    KoopaCar,
    Cloud,
    HoldingStone,
    /// Reach the goal wearing a Buzzy (1) / Spiny (2) helmet; in a Dry Bones Shell.
    Helmet(u8),
    DryBonesShell,
    /// Shake N trees (bell-tree actions, count kind 8).
    BellTrees,
    /// Reach the goal without being held by a swinging claw / without entering water.
    NoClaw,
    NoWater,
    /// Reach the goal holding a Koopa shell / riding Yoshi.
    HoldingShell,
    Yoshi,
    /// Reach the goal without taking damage / without landing after leaving the ground.
    NoDamage,
    NoLanding,
    /// Reach the goal in an update power-up's form (`AtGoal::form`: 12 Link … 17 SMB2).
    Form(u8),
    /// Reach the goal wearing a wearable (`AtGoal::wearing`).
    Wearing(u8),
    /// A condition the engine cannot track yet (list index).
    Unsupported(u8),
}

/// The action conditions' state of one player (local co-op keeps one per player: a rule broken
/// by one player leaves the others able to clear).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ActionState {
    pub violated: bool,
    left_ground: bool,
    ground_frames: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ClearCondition {
    pub goal: Goal,
    /// Target count for count conditions.
    pub n: u32,
    pub count: u32,
    /// Action conditions: the attempt has broken the rule.
    pub violated: bool,
    /// Count conditions: placed objects already counted (each counts once).
    counted: Vec<u32>,
    /// No-landing: frames on the ground since leaving it first.
    left_ground: bool,
    ground_frames: u32,
}

impl ClearCondition {
    pub fn none() -> Self {
        Self::from_header(0, 0, 0)
    }

    /// From the header's category, condition hash and magnitude.
    pub fn from_header(category: u8, hash: u32, magnitude: u16) -> Self {
        let n = magnitude as u32;
        let goal = match (category, index_of(hash)) {
            (0, _) => Goal::None,
            (_, None) if hash == 0 => Goal::None,
            (_, None) => Goal::Unsupported(0),
            (1, Some(i)) => match i {
                1..=41 if n > 0 => Goal::Defeat(i),
                43 if n > 0 => Goal::OneUps,
                53 | 54 if n > 0 => Goal::Pow,
                55 if n > 0 => Goal::PSwitch,
                66 if n > 0 => Goal::Coins(0),
                67 if n > 0 => Goal::Coins(10),
                68 if n > 0 => Goal::Coins(30),
                69 if n > 0 => Goal::Coins(50),
                75 if n > 0 => Goal::BellTrees,
                78..=87 if n > 0 => Goal::Defeat(i),
                // the escort Toad: kind 0, no tracking (the Toad itself fails the run)
                76 => Goal::None,
                _ => Goal::Unsupported(i),
            },
            (2, Some(i)) => match i {
                42 => Goal::Super,
                44 => Goal::Fire,
                47 => Goal::Raccoon,
                48 => Goal::Cape,
                49 => Goal::Propeller,
                50 => Goal::Cat,
                51 => Goal::Builder,
                52 => Goal::Superball,
                46 => Goal::Mega,
                45 => Goal::Star,
                58 => Goal::Shoe,
                62 => Goal::ClownCar,
                64 => Goal::KoopaCar,
                72 => Goal::HoldingShell,
                59 => Goal::Yoshi,
                63 => Goal::Cloud,
                60 => Goal::Helmet(1),
                61 => Goal::Helmet(2),
                65 => Goal::DryBonesShell,
                77 => Goal::HoldingStone,
                // Link, SMB2, Frog, Balloon, Squirrel, Boomerang
                88 => Goal::Form(12),
                89 => Goal::Form(17),
                90 => Goal::Form(13),
                91 => Goal::Form(14),
                92 => Goal::Form(15),
                93 => Goal::Form(16),
                // Cannon Box, Propeller Box, Goomba Mask, Bullet Bill Mask, Red POW Box
                94 => Goal::Wearing(2),
                95 => Goal::Wearing(1),
                96 => Goal::Wearing(3),
                97 => Goal::Wearing(5),
                98 => Goal::Wearing(4),
                _ => Goal::Unsupported(i),
            },
            (3, Some(70)) => Goal::NoDamage,
            (3, Some(71)) => Goal::NoLanding,
            (3, Some(73)) => Goal::NoClaw,
            (3, Some(74)) => Goal::NoWater,
            (_, Some(i)) => Goal::Unsupported(i),
        };
        Self {
            goal,
            n,
            count: 0,
            violated: false,
            counted: Vec::new(),
            left_ground: false,
            ground_frames: 0,
        }
    }

    pub fn active(&self) -> bool {
        self.goal != Goal::None
    }

    pub fn supported(&self) -> bool {
        !matches!(self.goal, Goal::Unsupported(_))
    }

    /// Count `amount` toward the goal. `key`: the placed object (counted once), None for objects
    /// that aren't placed (each counts). Returns true when this completes the count.
    pub fn count(&mut self, key: Option<u32>, amount: u32) -> bool {
        if let Some(k) = key {
            if self.counted.contains(&k) {
                return false;
            }
            self.counted.push(k);
        }
        let before = self.count;
        self.count = self.count.saturating_add(amount);
        before < self.n && self.count >= self.n
    }

    /// A defeated enemy of class `class`.
    pub fn defeated(&mut self, class: u8, key: Option<u32>) -> Option<bool> {
        (self.goal == Goal::Defeat(class)).then(|| self.count(key, 1))
    }

    /// A coin of value 1, 10, 30 or 50 collected.
    pub fn coin(&mut self, value: u32) -> Option<bool> {
        match self.goal {
            Goal::Coins(0) => Some(self.count(None, value)),
            Goal::Coins(v) if v == value => Some(self.count(None, 1)),
            _ => None,
        }
    }

    /// Per frame: held by a swinging claw / in water (the NoClaw / NoWater rules).
    pub fn held_by_claw(&mut self) {
        if self.goal == Goal::NoClaw {
            self.violated = true;
        }
    }

    pub fn in_water(&mut self) {
        if self.goal == Goal::NoWater {
            self.violated = true;
        }
    }

    pub fn damaged(&mut self) {
        if self.goal == Goal::NoDamage {
            self.violated = true;
        }
    }

    /// Per frame, no-landing: a landing counts once the player has stood for more than 2
    /// frames after first leaving the ground [obs: value > 2].
    pub fn ground(&mut self, on_ground: bool) {
        if self.goal != Goal::NoLanding {
            return;
        }
        if !on_ground {
            self.left_ground = true;
            self.ground_frames = 0;
        } else if self.left_ground {
            self.ground_frames += 1;
            if self.ground_frames > 2 {
                self.violated = true;
            }
        }
    }

    /// Exchange the action conditions' state with another player's (co-op passes).
    pub fn swap_action(&mut self, other: &mut ActionState) {
        std::mem::swap(&mut self.violated, &mut other.violated);
        std::mem::swap(&mut self.left_ground, &mut other.left_ground);
        std::mem::swap(&mut self.ground_frames, &mut other.ground_frames);
    }

    /// Is the goal open? `star`, `form`: the player's state when touching it (form 0 small,
    /// 1 super, 2 fire).
    pub fn met(&self, at: AtGoal) -> bool {
        let AtGoal {
            star,
            form,
            shoe,
            clown_car,
            koopa_car,
            cloud,
            karon_shell,
            helmet,
            holding_stone,
            holding_shell,
            yoshi,
            wearing,
        } = at;
        match self.goal {
            Goal::None => true,
            Goal::Defeat(_)
            | Goal::OneUps
            | Goal::Coins(_)
            | Goal::Pow
            | Goal::PSwitch
            | Goal::BellTrees => self.count >= self.n,
            Goal::Star => star,
            Goal::Super => form == 1,
            Goal::Fire => form == 2,
            Goal::Raccoon => form == 3,
            Goal::Cape => form == 4,
            Goal::Propeller => form == 5,
            Goal::Cat => form == 6,
            Goal::Builder => form == 7,
            Goal::Superball => form == 8,
            Goal::Mega => form == 9,
            Goal::Shoe => shoe,
            Goal::ClownCar => clown_car,
            Goal::KoopaCar => koopa_car,
            Goal::Cloud => cloud,
            Goal::Helmet(h) => helmet == h,
            Goal::DryBonesShell => karon_shell,
            Goal::HoldingStone => holding_stone,
            Goal::HoldingShell => holding_shell,
            Goal::Yoshi => yoshi,
            Goal::Form(f) => form == f,
            Goal::Wearing(w) => wearing == w,
            Goal::NoDamage | Goal::NoLanding | Goal::NoClaw | Goal::NoWater => !self.violated,
            Goal::Unsupported(_) => false,
        }
    }

    /// A restart (death): the attempt starts over.
    pub fn reset(&mut self) {
        *self = Self {
            goal: self.goal,
            n: self.n,
            ..Self::none()
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_resolve_to_goals() {
        assert_eq!(index_of(0x7f07acbf), Some(1));
        assert_eq!(index_of(0x46219146), Some(77));
        let c = ClearCondition::from_header(1, 0x7f07acbf, 5);
        assert_eq!((c.goal, c.n), (Goal::Defeat(enemy::GOOMBA), 5));
        assert_eq!(
            ClearCondition::from_header(3, 0x1664515a, 0).goal,
            Goal::NoDamage
        );
        assert_eq!(
            ClearCondition::from_header(2, 0x2a8e77bb, 0).goal,
            Goal::Fire
        );
        assert!(
            matches!(
                ClearCondition::from_header(2, 0xb4fa3d4b, 0).goal,
                Goal::Helmet(_)
            ),
            "helmet"
        );
        assert_eq!(ClearCondition::none().goal, Goal::None);
    }

    #[test]
    fn the_updates_conditions_resolve() {
        // v3.0.3's list: defeat Spikes / Ludwigs, goal as Frog Mario, wearing a Goomba Mask
        let c = ClearCondition::from_header(1, 0x3b8a1111, 3);
        assert_eq!((c.goal, c.n), (Goal::Defeat(enemy::SPIKE), 3));
        assert_eq!(
            ClearCondition::from_header(1, 0xece74bc7, 1).goal,
            Goal::Defeat(enemy::KOOPALINGS[6])
        );
        let frog = ClearCondition::from_header(2, 0x2c66b03e, 0);
        assert_eq!(frog.goal, Goal::Form(13));
        assert!(frog.met(AtGoal {
            form: 13,
            ..Default::default()
        }));
        let mask = ClearCondition::from_header(2, 0x449eb152, 0);
        assert_eq!(mask.goal, Goal::Wearing(3));
        assert!(!mask.met(AtGoal::default()));
        assert!(mask.met(AtGoal {
            wearing: 3,
            ..Default::default()
        }));
    }

    #[test]
    fn counts_each_placed_object_once() {
        let mut c = ClearCondition::from_header(1, 0x7f07acbf, 2);
        assert_eq!(c.defeated(enemy::GOOMBA, Some(7)), Some(false));
        assert_eq!(c.defeated(enemy::GOOMBA, Some(7)), Some(false), "respawn");
        assert_eq!(c.defeated(enemy::KOOPA, Some(8)), None);
        assert!(!c.met(AtGoal::default()));
        assert_eq!(c.defeated(enemy::GOOMBA, None), Some(true));
        assert!(c.met(AtGoal::default()));
    }

    #[test]
    fn coins_and_actions() {
        let mut c = ClearCondition::from_header(1, 0xf55b3863, 30);
        c.coin(10);
        c.coin(1);
        assert_eq!(c.count, 11);
        let mut c = ClearCondition::from_header(1, 0xc78f5040, 2);
        c.coin(1);
        c.coin(10);
        assert_eq!(c.count, 1);
        let mut c = ClearCondition::from_header(3, 0x08327ae6, 0);
        c.ground(true);
        c.ground(false);
        c.ground(true);
        c.ground(true);
        assert!(c.met(AtGoal::default()), "two frames of grace");
        c.ground(true);
        assert!(!c.met(AtGoal::default()));
    }
}
