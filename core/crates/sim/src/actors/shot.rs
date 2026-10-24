//! What a Bill Blaster fires, actor side: the EnemySys **Killer** context (shared enemy draft
//! §5.5, 13-bill-blaster.md "Firing other enemies", 8-coin.md "Fired from a Bill Blaster").
//! Every fired part enters its EnemyDB `action_killer` preset's Shot state; the presets differ in
//! the launch and in how the Shot ends, then the part hands over to its normal context (Basic,
//! Wing for winged parts, Parachute while it carries one) through the shared return-to-normal
//! rule (0x7100796c00). The blaster's alternate form (0x4) spawns with the fast flag 0x2000000
//! (`fast`). The world side (when and where the blaster fires, the coin bursts) is in world.rs
//! `step_blasters`. [obs] values are from v1.0.0 unless marked v3.0.3; both builds agree on
//! them except where noted.

use super::{Actor, ENEMY_GRAVITY, Kind, MAX_FALL, Mode};
use crate::fmath::DetMath;
use crate::level::Level;

/// Coins: the drop gravity and fall cap of the Coin actor's Killer Shot (vf 0x1a0
/// 0x7100849240: −0.14, −4.0) [obs].
const COIN_GRAVITY: f32 = -0.14;
/// A coin's launch from its slot in the burst (0–9): vy and |vx| / 0.6 (tables 0x7101d03f0c,
/// 0x7101d03e8c; the same in v3.0.3 at 0x71023cede0 / 0x71023ced60) [obs].
const COIN_VY: [f32; 10] = [1.5, 1.0, 1.8, 0.8, 1.2, 1.1, 1.8, 1.4, 1.2, 0.9];
const COIN_VX: [f32; 10] = [2.5, 0.75, 2.125, 1.75, 2.0, 1.0, 2.25, 0.5, 2.375, 1.5];
const COIN_VX_SCALE: f32 = 0.6;
/// The fast (red) blaster triples a coin's vx (subtype 5 in 0x7100849240) [obs].
const COIN_FAST: f32 = 3.0;
/// The bounces on its 1st / 2nd / 3rd landing (0x710087eea0, table 0x7101c8bb30) [obs]; after
/// the third it creeps along the floor at ±0.34 (+0xda0, 0x7101c8a574 / 0x7101c8a1bc) [obs].
const COIN_BOUNCE: [f32; 3] = [2.0, 1.0, 0.0];
const COIN_CREEP: f32 = 0.34;
/// Shot coins pushed 3 px further out of the muzzle (Shot enter 0x7100848b70) [obs].
pub const COIN_PUSH: f32 = 3.0;

/// The Default launch (vf 0x110 0x71005fe310): 1.5 px/f (4.5 fast), vy 0.6; a parachute's
/// 2.5 (5.0), vy 0 (Shot enter 0x71005fd870) [obs].
const SHOT_SPEED: [f32; 2] = [1.5, 4.5];
const SHOT_VY: f32 = 0.6;
const CHUTE_SPEED: [f32; 2] = [2.5, 5.0];
/// A parachute opens 18 frames after the shot (counted while not rising) or on landing
/// (10 for a facing the code calls 3 [?]) [obs 0x71005fd870, 0x71005fde30].
const CHUTE_FRAMES: i32 = 18;
/// Winged FlyStop / WingFly parts: 1.2 (3.6), no gravity (0x7101c8c440) [obs].
const WING_SPEED: [f32; 2] = [1.2, 3.6];
/// CloudShot: 2.7 (5.94) (0x7101c8c6d8) [obs].
const CLOUD_SPEED: [f32; 2] = [2.7, 5.94];

/// Frames until the shrunk-into-the-barrel scale reaches 1: it starts at 16 / height and grows
/// ×1.05 a frame (vf 0x108 0x71005fd240, Shot exe 0x71005fd9b0) [obs] — 0 for 16 px parts, 15 for
/// the big (32 px) ones.
const BIG_GROW: u16 = 15;
/// Frames until the animation speed eases from 0.1 to 1 (+ max(0.1·(1 − s), 0.05) a frame) [obs].
const ANIM_EASE: u16 = 16;

/// The `action_killer` presets (EnemyDB) that differ in what this engine models.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Preset {
    /// Default (and the presets that keep its launch and per-frame vfuncs: Kuribo out of water,
    /// Pakkun, Bros, Kameck, Wanwan, Pukupuku, MapObjDefault, …): a ballistic hop.
    Default,
    /// FlyStop (red Koopa, Bob-omb, Gabon): winged, a gliding stop, then hovers.
    FlyStop,
    /// WingFly-like (Togezo / Met / Karon / Togemet): winged, straight, then hovers.
    WingFly,
    /// CloudShot (Boo, Blooper, Koopa Clown Car): no gravity, slows to a stop.
    CloudShot,
    /// ItemDefault / ItemLeaf / BigKinoko: winged items fly 16 px straight.
    Item,
    /// The Coin actor's own Shot.
    Coin,
}

/// A part's `action_killer` preset (enemydb.tsv; vtable slots +0x110 / +0x120 in
/// actionmap.txt) [obs]; parts this engine has no own Shot for fall back to Default [inf].
pub fn preset(kind: Kind) -> Preset {
    match kind {
        Kind::FloatCoin => Preset::Coin,
        Kind::Koopa { red: true } | Kind::BobOmb => Preset::FlyStop,
        Kind::Spiny | Kind::Buzzy | Kind::DryBones | Kind::SpikeTop { .. } => Preset::WingFly,
        Kind::Boo | Kind::Blooper | Kind::ClownCar { .. } => Preset::CloudShot,
        k if k.is_item() => Preset::Item,
        _ => Preset::Default,
    }
}

/// How many of a part a blaster keeps alive at once: its EnemyDB `spout_killer_count`
/// (checked by the blaster's limiter 0x7100364d80 before each shot) [obs enemydb.tsv]; 0 = the
/// blaster never fires it. Coins come as a TenCoin burst that is gone in a few frames [inf].
pub fn spout_limit(kind: Kind) -> usize {
    match kind {
        Kind::Piranha { .. }
        | Kind::Boo
        | Kind::LavaBubble
        | Kind::RockyWrench
        | Kind::Stingby
        | Kind::Skipsqueak { .. } => 2,
        Kind::HammerBro | Kind::Magikoopa | Kind::Blooper | Kind::ChainChomp { .. } => 3,
        Kind::Wiggler | Kind::FishBone => 3,
        Kind::CheepCheep { red: true } => 3,
        Kind::ClownCar { .. } | Kind::KoopaCar | Kind::Porcupuffer | Kind::ShoeGoomba { .. } => 1,
        Kind::Mechakoopa { .. } => 2,
        Kind::Thwomp { .. }
        | Kind::Bowser { .. }
        | Kind::BowserJr { .. }
        | Kind::BoomBoom { .. }
        | Kind::Koopaling { .. }
        | Kind::Bully
        | Kind::Pokey { .. } => 0,
        Kind::FloatCoin => usize::MAX,
        _ => 10,
    }
}

impl Actor {
    /// Leave the muzzle in the Killer context: the preset's launch (Shot enter 0x71005fd870 and
    /// the preset's vf 0x110) [obs]. `self.dir`, `wings`, `big` and `chute` (a parachute) must be
    /// set.
    pub fn launch(&mut self, fast: bool) {
        let f = fast as usize;
        let d = self.dir as f32;
        self.vy = 0.0;
        self.timer = 0;
        let preset = preset(self.kind);
        let winged = self.wings && !self.chute;
        match preset {
            _ if self.chute => {
                self.vx = CHUTE_SPEED[f] * d;
                self.timer = CHUTE_FRAMES;
            }
            Preset::FlyStop | Preset::WingFly if winged => self.vx = WING_SPEED[f] * d,
            Preset::CloudShot => self.vx = CLOUD_SPEED[f] * d,
            _ => {
                self.vx = SHOT_SPEED[f] * d;
                // winged items fly straight (Item Shot enter 0x710082af70: vy 0, no gravity)
                if !(preset == Preset::Item && winged) {
                    self.vy = SHOT_VY;
                }
            }
        }
        self.mode = Mode::Shot {
            x0: self.x,
            y0: self.y,
            t: 0,
            fast,
            landed: false,
        };
    }

    /// A coin of a blaster's TenCoin burst (slot 0–9) leaves the muzzle: the Coin actor's Shot
    /// enter (0x7100848b70, velocity vf 0x1a0 0x7100849240) [obs]. Winged coins fly straight
    /// at the Default 1.5 (4.5); a coin with a parachute launches at 1.5 (4.5), vy 0.6 in v3.0.3
    /// (v1.0.0 used the slot's vx ± 1.5) [obs v3.0.3 0x7101366c30].
    pub fn coin_shot(&mut self, slot: usize, fast: bool) {
        let d = self.dir as f32;
        self.x += COIN_PUSH * d;
        let f = fast as usize;
        self.hops = 0;
        self.timer = 0;
        if self.wings && !self.chute {
            self.vx = SHOT_SPEED[f] * d;
            self.vy = 0.0;
        } else if self.chute {
            self.vx = SHOT_SPEED[f] * d;
            self.vy = SHOT_VY;
            self.timer = CHUTE_FRAMES;
        } else {
            let s = slot % COIN_VY.len();
            self.vy = COIN_VY[s];
            self.vx = COIN_VX[s] * COIN_VX_SCALE * d * if fast { COIN_FAST } else { 1.0 };
            self.speed = COIN_CREEP * d;
        }
        self.mode = Mode::Shot {
            x0: self.x,
            y0: self.y,
            t: 0,
            fast,
            landed: false,
        };
    }

    /// Past the barrel: can touch the player and other actors once 8 px (16 big) from the
    /// launch point (vf 0x118 0x71005fe3a0 with vf 0x128 0x71005fd500) [obs]; a coin at once
    /// (its Shot enter clears the no-contact bit) [obs 0x7100848b70].
    pub(super) fn shot_clear(&self) -> bool {
        let Mode::Shot { x0, y0, .. } = self.mode else {
            return false;
        };
        let r = if self.big { 16.0 } else { 8.0 };
        self.kind == Kind::FloatCoin || (self.x - x0).dhypot(self.y - y0) >= r
    }

    /// One frame of the Shot state.
    pub(super) fn step_shot(&mut self, level: &Level) {
        let Mode::Shot {
            x0,
            y0,
            t,
            fast,
            landed,
        } = self.mode
        else {
            return;
        };
        let t = t.saturating_add(1);
        self.mode = Mode::Shot {
            x0,
            y0,
            t,
            fast,
            landed,
        };
        let grown = !self.big || t >= BIG_GROW;
        let winged = self.wings && !self.chute;
        match preset(self.kind) {
            Preset::Coin if winged => self.shot_straight(level, x0, 16.0),
            Preset::Coin if !self.chute => self.coin_bounce(level),
            Preset::Item if winged => self.shot_straight(level, x0, 16.0),
            Preset::FlyStop if winged => {
                // FlyStop (vf 0x120 0x71005fe820): slows by max(3 % of |vx|, 0.01) a frame;
                // at |vx| ≤ 0.2 or a wall (full size) it hovers [obs]
                if grown && (self.vx.abs() <= 0.2 || self.wall) {
                    return self.end_shot();
                }
                let s = self.vx.abs();
                self.vx = self.vx.signum() * (s - (s * 0.03).max(0.01)).max(0.0);
                self.collide(level);
            }
            Preset::WingFly if winged => {
                // straight at its launch speed; hovers once 8 px (16 big) out at full size and
                // full animation speed (vf 0x120 0x71005fed80 / 0x7100780d10) [obs]
                let r = if self.big { 16.0 } else { 8.0 };
                if grown && t >= ANIM_EASE && (self.x - x0).dhypot(self.y - y0) >= r {
                    return self.end_shot();
                }
                self.collide(level);
            }
            Preset::CloudShot if !self.chute => {
                // CloudShot (vf 0x120 0x71005fd450): slows by max(0.06·k, 2 % of |vx|), k 1
                // (2.2 fast), halved when big; at |vx| ≤ 0.5 back to normal [obs]
                let s = self.vx.abs();
                if s <= 0.5 {
                    return self.end_shot();
                }
                let k: f32 = if fast { 2.2 } else { 1.0 } * if self.big { 0.5 } else { 1.0 };
                self.vx = self.vx.signum() * (s - (0.06 * k).max(s * 0.02)).max(0.0);
                self.collide(level);
            }
            _ => self.shot_hop(level, landed, grown),
        }
    }

    /// The Default Shot (vf 0x120 0x71005fde30) [obs]: above 0.5 px/f the speed loses 0.4 % a
    /// frame (not below 0.5); on the ground it loses half of it. The first landing bounces at
    /// max(1.5, half the impact), later ones only off an impact faster than −1.5 (half of it); a
    /// softer landing at full size ends the Shot. A wall ahead turns it back at half speed and
    /// ends it (vf 0x148 0x71005fd770). A parachute opens after its countdown or on landing.
    fn shot_hop(&mut self, level: &Level, landed: bool, grown: bool) {
        let s = self.vx.abs();
        let mut v = s;
        if v > 0.5 {
            v = (v - v * 0.004).max(0.5);
        }
        if self.on_ground {
            v = (v - s * 0.5).max(0.0);
        }
        self.vx = self.vx.signum() * v;
        if self.wall {
            self.dir = -self.dir;
            self.vx = self.vx.abs() * 0.5 * self.dir as f32;
            return self.end_shot();
        }
        if self.chute {
            if self.vy <= 0.0 && self.timer > 0 {
                self.timer -= 1;
            }
            if self.on_ground || self.timer < 1 {
                return self.end_shot();
            }
        }
        let g = if self.kind == Kind::FloatCoin {
            COIN_GRAVITY
        } else {
            ENEMY_GRAVITY
        };
        self.vy = (self.vy + g).max(MAX_FALL);
        let fall = self.vy;
        self.collide(level);
        if !self.on_ground || fall >= 0.0 || self.chute {
            return;
        }
        if !landed || fall < -1.5 {
            self.vy = if landed {
                -0.5 * fall
            } else {
                (-0.5 * fall).max(1.5)
            };
            if let Mode::Shot { landed, .. } = &mut self.mode {
                *landed = true;
            }
        } else if grown {
            self.end_shot();
        }
    }

    /// Winged coins and items: straight, no gravity, until 16 px from the muzzle (Item Shot exe
    /// 0x710082aff0) [obs].
    fn shot_straight(&mut self, level: &Level, x0: f32, until: f32) {
        if (self.x - x0).abs() >= until {
            return self.end_shot();
        }
        self.vy = 0.0;
        self.collide(level);
    }

    /// A shot coin's bounce (0x710087eea0 with "rising" on, halving off) [obs]: not rising and
    /// on the floor, it bounces 2.0, 1.0, then 0, keeping its launch vx; after the third it
    /// creeps at 0.34; a ceiling sends it down at −2, a wall reverses it. It never leaves the
    /// Shot state (only collection or the despawn ends it).
    fn coin_bounce(&mut self, level: &Level) {
        if self.vy <= 0.0 && self.on_ground {
            if self.hops >= COIN_BOUNCE.len() as u8 {
                self.vx = self.speed;
            } else {
                self.vy = COIN_BOUNCE[self.hops as usize];
                self.hops += 1;
            }
        }
        if self.wall {
            self.vx = -self.vx;
            self.speed = -self.speed;
            self.dir = -self.dir;
        }
        self.vy = (self.vy + COIN_GRAVITY).max(MAX_FALL);
        let rising = self.vy > 0.0;
        self.collide(level);
        if rising && self.vy == 0.0 && !self.on_ground {
            self.vy = -2.0;
        }
    }

    /// Back to normal (the return-to-normal rule 0x7100796c00): a parachute first (it pops
    /// open at vy +0.3 after a shot, 0x71005f36e0), then Wing for a winged part (it hovers from
    /// here), else Basic [obs shared enemy draft §5.2].
    fn end_shot(&mut self) {
        self.timer = 0;
        self.hops = 0;
        self.home_y = self.y;
        if self.vx != 0.0 && self.kind != Kind::FloatCoin {
            self.dir = if self.vx < 0.0 { -1 } else { 1 };
        }
        if std::mem::take(&mut self.chute) {
            self.vx = 0.0;
            self.vy = 0.3;
            self.mode = Mode::Parachute {
                x0: self.x,
                phase: 0.0,
            };
        } else {
            if self.wings {
                self.vx = 0.0;
                self.vy = 0.0;
            }
            self.mode = Mode::Normal;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::level::TILE;

    /// Flat ground, its top at y 32.
    fn floor() -> Level {
        Level::flat(100)
    }

    fn run(a: &mut Actor, lvl: &Level, n: usize) {
        for _ in 0..n {
            a.step(lvl, 0.0);
        }
    }

    #[test]
    fn a_shot_goomba_hops_out_at_the_default_speed() {
        let lvl = floor();
        let mut a = Actor::new(Kind::Goomba, 20.0 * TILE, 4.0 * TILE);
        a.dir = 1;
        a.launch(false);
        assert_eq!((a.vx, a.vy), (1.5, 0.6));
        a.step(&lvl, 0.0);
        // a frame of friction: 0.4 % off the speed
        assert!((a.vx - 1.5 * 0.996).abs() < 1e-5, "{}", a.vx);
        // it lands, bounces once (at least 1.5) and then walks as a Goomba
        let mut bounced = false;
        for _ in 0..200 {
            let vy = a.vy;
            a.step(&lvl, 0.0);
            bounced |= a.vy >= 1.5 && vy <= 0.0;
            if a.mode == Mode::Normal {
                break;
            }
        }
        assert!(bounced);
        assert_eq!(a.mode, Mode::Normal);
        assert!(a.x > 20.0 * TILE + 8.0, "it moved out: {}", a.x);
    }

    #[test]
    fn the_alternate_blaster_launches_three_times_as_fast() {
        let mut a = Actor::new(Kind::Goomba, 0.0, 0.0);
        a.dir = -1;
        a.launch(true);
        assert_eq!((a.vx, a.vy), (-4.5, 0.6));
        // a parachute: 2.5 → 5.0; CloudShot 2.7 → 5.94; a winged FlyStop 1.2 → 3.6
        let mut p = Actor::new(Kind::BobOmb, 0.0, 0.0);
        p.chute = true;
        p.launch(false);
        assert_eq!((p.vx, p.vy, p.timer), (2.5, 0.0, 18));
        p.launch(true);
        assert_eq!(p.vx, 5.0);
        let mut b = Actor::new(Kind::Boo, 0.0, 0.0);
        b.launch(true);
        assert_eq!((b.vx, b.vy), (5.94, 0.0));
        let mut k = Actor::new(Kind::Koopa { red: true }, 0.0, 0.0);
        k.wings = true;
        k.launch(true);
        assert_eq!((k.vx, k.vy), (3.6, 0.0));
    }

    #[test]
    fn a_winged_red_koopa_glides_to_a_stop_and_hovers() {
        let lvl = floor();
        let mut k = Actor::new(Kind::Koopa { red: true }, 20.0 * TILE, 8.0 * TILE);
        k.wings = true;
        k.launch(false);
        let y = k.y;
        let mut n = 0;
        while matches!(k.mode, Mode::Shot { .. }) && n < 400 {
            k.step(&lvl, 0.0);
            n += 1;
        }
        assert_eq!(k.mode, Mode::Normal);
        assert_eq!(k.y, y, "no gravity while it glides");
        // 1.2 → 0.2 losing 3 % (at least 0.01) a frame
        assert!((55..75).contains(&n), "{n}");
    }

    #[test]
    fn a_boo_slows_to_half_a_pixel_then_floats() {
        let lvl = floor();
        let mut b = Actor::new(Kind::Boo, 20.0 * TILE, 8.0 * TILE);
        b.dir = 1;
        b.launch(false);
        let mut n = 0;
        while matches!(b.mode, Mode::Shot { .. }) && n < 400 {
            b.step(&lvl, 0.0);
            n += 1;
        }
        // 2.7 → 0.5 at max(0.06, 2 %) a frame
        assert!((25..45).contains(&n), "{n}");
        assert_eq!(b.mode, Mode::Normal);
    }

    #[test]
    fn a_parachute_opens_18_frames_after_the_shot() {
        let lvl = floor();
        let mut a = Actor::new(Kind::Goomba, 20.0 * TILE, 12.0 * TILE);
        a.chute = true;
        a.launch(false);
        run(&mut a, &lvl, 17);
        assert!(matches!(a.mode, Mode::Shot { .. }));
        a.step(&lvl, 0.0);
        assert!(matches!(a.mode, Mode::Parachute { .. }), "{:?}", a.mode);
        assert_eq!(a.vy, 0.3);
    }

    #[test]
    fn a_shot_coin_arcs_bounces_twice_and_creeps() {
        let lvl = floor();
        let x = 20.0 * TILE;
        let mut c = Actor::new(Kind::FloatCoin, x, 4.0 * TILE);
        c.dir = 1;
        c.coin_shot(0, false);
        assert_eq!(c.x, x + 3.0, "3 px out of the muzzle");
        assert_eq!((c.vx, c.vy), (2.5 * 0.6, 1.5));
        // the third slot of the fast blaster: 2.125 × 0.6 × 3, to the left
        let mut f = Actor::new(Kind::FloatCoin, x, 4.0 * TILE);
        f.dir = -1;
        f.coin_shot(2, true);
        assert!((f.vx + 2.125 * 0.6 * 3.0).abs() < 1e-5 && f.vy == 1.8);
        // rises 1.5 under −0.14 a frame, lands, bounces 2.0 then 1.0 at full speed
        let mut peaks = vec![];
        let mut last = c.vy;
        for _ in 0..200 {
            c.step(&lvl, 0.0);
            if last <= 0.0 && c.vy > 0.0 {
                peaks.push(c.vy);
            }
            last = c.vy;
        }
        assert_eq!(peaks, vec![2.0 - 0.14, 1.0 - 0.14], "two bounces");
        assert!(c.on_ground && matches!(c.mode, Mode::Shot { .. }));
        assert_eq!(c.vx, 0.34, "then it creeps");
        assert!(c.shot_clear(), "collectable");
    }

    #[test]
    fn spout_limits_follow_the_enemydb() {
        assert_eq!(spout_limit(Kind::Goomba), 10);
        assert_eq!(spout_limit(Kind::Boo), 2);
        assert_eq!(spout_limit(Kind::HammerBro), 3);
        assert_eq!(spout_limit(Kind::Bully), 0);
    }
}
