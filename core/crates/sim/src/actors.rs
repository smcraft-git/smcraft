//! Actors: enemies, items and the player's fireballs.
//!
//! From the Phase 3 specs: objects.md §3 (generic walker), 0-goomba.md, 1-koopa.md,
//! 20-super-mushroom.md (shared item movement and block emergence), 34-fire-flower.md (fireballs),
//! 35-super-star.md, 33-one-up.md and the shared enemy draft (death, emergence, activation).
//! Positions are the feet centre (px, y up); boxes extend half_w either side and h upward.

use crate::fmath::DetMath;
use crate::level::{Cell, Level, PipeDir, TILE};

pub mod block_out;
mod boo;
mod bosses;
mod bros;
pub use bros::BroWing;
mod budget;
pub use bosses::BowserStyle;
pub use budget::{Hold as BudgetHold, Reserve, ThrowBudget};
mod ceiling;
mod chute;
mod claw;
pub mod seesaw;
pub use claw::{ClawRope, NUDGE as CLAW_NUDGE, REACH as CLAW_REACH};
mod clown;
pub use clown::{ClownFire, charge_burst_tiles};
mod dokan;
pub use dokan::Dokan;
mod fly;
pub use fly::Fly;
mod koopalings;
mod powerups;
mod propeller;
pub use propeller::{Prop, PropPhase};
mod shot;
pub use koopalings::{shot as koopaling_shot, who as koopaling_who};
pub use powerups::{ARROW_LAUNCH, BOOMERANG_LEGS, STYLE2, SWORD, WEAR, item_for_part};
pub use powerups::{Attack as PuAttack, Reaction as PuReaction};
pub use powerups::{BOOMERANG_STOP as BOOMERANG_STOP_FRAMES, CANNON_LIFE as CANNON_SHOT_LIFE};
pub use powerups::{THROW_MAX as THROW_MAX_USA, THROW_SPEED as THROW_SPEED_USA};
pub use shot::{COIN_PUSH, spout_limit};
mod v3;
mod w3;
mod wing;
pub use bosses::{BossAttack, BossHit};
pub use v3::{LASER_SIGN, MECHA_DOWN, MISSILE_LIFE, Seg, V3State};
pub use w3::{CREEP_CELL, W3Look, W3State, creeper_corner, push as bully_push};

pub const WALK_SPEED: f32 = 0.5;
pub const ENEMY_GRAVITY: f32 = -0.1875;
pub const MAX_FALL: f32 = -4.0;
/// How long homing Bills steer (13-bill-blaster.md, 32-banzai-bill.md).
pub const HOMING_FRAMES: i32 = 528;
pub const TURN_LOCK: i32 = 8;
pub const STOMP_FRAMES: i32 = 30;
pub const SHELL_WAKE: i32 = 511;
pub const SHELL_SHAKE: i32 = 96;
pub const ITEM_SPEED: f32 = 1.0;
/// A Yoshi's Egg: its box's half size round its position (profile YoshiEgg {0, 8, 22, 22}) and
/// the hatch helper's count from Land (+0x8 reset to 0x28) [obs v3.0.3 0x71013f7990].
const EGG_BOX: f32 = 22.0;
/// The Sledge Bro's hover count, and its winged bob: amplitude and step a frame [obs v3.0.3
/// Hovering enter 0x71011d2b20 / exe 0x71011d2b50: 0.4 at 0x7102319274, 0x5b05b04 = 8°].
const SLEDGE_HOVER: f32 = 10.0;
const SLEDGE_BOB: f32 = 0.4;
const SLEDGE_BOB_STEP: u32 = 0x05b0_5b04;
const EGG_HATCH: i32 = 40;
pub const FIREBALL_SPEED: f32 = 3.625;
pub const FIREBALL_GRAVITY: f32 = -0.4375;
pub const FIREBALL_BOUNCE: f32 = 4.0;
pub const FIREBALL_LIFE: i32 = 600;
/// Piranha Plant height (EnemyDB tall_max 24; texture 16×24).
pub const PIRANHA_H: f32 = 24.0;
/// How many of a Lakitu's Spinies may be alive at once: EnemyDB Togezo spout_jugem_count,
/// the limit its spawn limiter checks (0x7100364d80) [obs].
const LAKITU_CHILDREN: u32 = 10;
/// Spike Top crawl speed (px/frame; ×1.5 for the alternate form) [obs 0x7100778710 tail].
const SPIKE_TOP_WALK: f32 = 0.5;
/// Spike Top corner turn rate, 3°/frame as a binary angle (0x2222222), 1.5° (0x1111110) under
/// the water-speed rule [obs v3.0.3 RotationWall / RotationCliff exes 0x7101278570 /
/// 0x7101278940].
const SPIKE_TOP_TURN: i32 = 0x0222_2222;
const SPIKE_TOP_TURN_SLOW: i32 = 0x0111_1110;
/// A quarter turn as a binary angle.
const QUARTER_TURN: i32 = 0x4000_0000;
/// Round an outside corner it moves 0.34 px/f (big 0.72) along its body angle + 5° (0x38e38e3)
/// the way it crawls [obs v3.0.3 0x7101278940, 0x71023197f0].
const SPIKE_TOP_CLIFF_SPEED: f32 = 0.34;
const SPIKE_TOP_CLIFF_SPEED_BIG: f32 = 0.72;
const SPIKE_TOP_CLIFF_LEAD: i32 = 0x038e_38e3;
/// Spike Top righting rate in the air and on landing, 15°/frame (0xaaaaaaa), and onto a
/// ceiling, 80°/frame (0x38e38e38); halved under the water-speed rule [obs 0x71007781d0,
/// 0x71007784c0; v3.0.3 InAir / Stick exes 0x7101276730 / 0x7101276a20].
const SPIKE_TOP_STICK: f32 = 15.0;
const SPIKE_TOP_STICK_CEILING: f32 = 80.0;

/// Unit tangent (cos θ, sin θ) and foot direction (sin θ, −cos θ) of a body angle θ that is
/// a multiple of 90° (the Spike Top crawler).
fn crawl_axes(theta: i32) -> ((f32, f32), (f32, f32)) {
    let c = [(1.0, 0.0), (0.0, 1.0), (-1.0, 0.0), (0.0, -1.0)]
        [(theta.rem_euclid(360) / 90) as usize & 3];
    (c, (c.1, -c.0))
}

/// Turn the angle `cur` (degrees) toward `target` the short way by at most `rate`, landing
/// exactly on it.
fn turn_toward(cur: f32, target: f32, rate: f32) -> f32 {
    let d = (target - cur + 540.0).rem_euclid(360.0) - 180.0;
    if d.abs() <= rate {
        target
    } else {
        (cur + rate * d.signum()).rem_euclid(360.0)
    }
}

// DieFall default launch (shared enemy draft §6.2)
const DIE_SPEED: f32 = 1.5;
const DIE_VY: f32 = 3.9;
const DIE_GRAVITY: f32 = -0.24375;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Goomba,
    /// Goombrat (the Goomba's 0x4 form; Goombud in SMW): a Goomba that turns at ledges.
    Goombrat,
    Koopa {
        red: bool,
    },
    /// A Koopa in its shell (still or sliding).
    Shell {
        red: bool,
    },
    Mushroom,
    FireFlower,
    /// The style power-up: Super Leaf (SMB3) / Cape Feather (SMW).
    StyleItem,
    Star,
    OneUp,
    Fireball,
    /// Piranha Plant (fire = alternate form). Free-standing, or cycling in a pipe.
    Piranha {
        fire: bool,
    },
    /// 10/30/50-coin (value 0, 1, 2).
    BigCoin {
        value: u8,
    },
    /// Fire Piranha Plant's fireball: straight line at 1 px/frame.
    EnemyFire,
    /// Thwomp: a solid that slams down. `side` = Side Thwomp (SideDossun: flag bits 0x18 not
    /// both set), which charges sideways, toward the right when `right` [obs 0x710076ab10].
    Thwomp {
        side: bool,
        right: bool,
    },
    /// One-way lift, `width` tiles; falling = drops once stood on; `right`: a pacing lift that
    /// starts to the right ((flags & 0x18) = 0x10).
    Lift {
        width: u8,
        falling: bool,
        right: bool,
    },
    /// Trampoline.
    Spring,
    /// Sideways spring (the Spring's 0x4 form, JumpStepSide): a solid block that flings what
    /// touches its sides away; `pause` = push animation frames left.
    SideSpring,
    /// Bullet Bill from a blaster: flies straight at 1 px/frame, through terrain.
    BulletBill,
    /// P Switch: pressed from above.
    PSwitch,
    /// Muncher: invulnerable solid hazard.
    Muncher,
    /// A Donut block that has dropped (falls g −0.1875, cap −2, carries riders).
    Donut,
    /// Spiny: walker that can't be stomped.
    Spiny,
    /// Buzzy Beetle: walker, shell when stomped, immune to fire.
    Buzzy,
    /// Spike Top (40-spike-top.md): crawls round blocks — floors, walls and ceilings, turning
    /// inside and outside corners; can't be stomped; fire-proof. `surface` = the placed side
    /// (facing field (flags >> 25) & 3: 0 wall on its right, 1 wall on its left, 2 ceiling,
    /// 3 floor) [obs 0x7100777b10; the sides checked against the terrain round 150 placed Spike
    /// Tops in the sample courses].
    SpikeTop {
        surface: u8,
    },
    /// Hammer Bro (paces, jumps, throws hammers).
    HammerBro,
    /// Hammer: arcs (vx 1.5, vy 3.0, g −0.125, cap −6), through terrain.
    Hammer,
    /// Bob-omb: lit by a stomp (fuse 240), can be kicked, explodes.
    BobOmb,
    /// Dry Bones: walker that turns at ledges; collapses for 224 frames when stomped.
    DryBones,
    /// Boo: chases when not looked at.
    Boo,
    /// A Blooper Nanny's baby (`slot` 1–4): placed by the world on her path 8 + 12·k frames
    /// back (8 + 8·k winged); `chain` = her actor id. 8×8 [inf].
    BabyBlooper {
        slot: u8,
    },
    /// One Boo of a Boo circle (the Boo's 0x4 form, TeresaSpread): 8 slots 37.06° apart round
    /// the placement (`home_x`, `home_y`), spreading to 52 px (×1.4 big) at 0.15·(R − r)
    /// clamped 0.1–0.6 px/f, turning clockwise at up to 1°/frame (0.5 big), the turn speed
    /// ramping in by 0.04/frame (41-boo.md). Radius in `speed`, ring angle in `heading`.
    RingBoo {
        slot: u8,
    },
    /// Banzai Bill: 64×64, flies 1 px/frame (dir 0 right, 1 down, 2 left, 3 up); `homing` = the
    /// 0x4 Bull's-Eye Banzai (turns ≤ 0.25°/frame toward the player for 528 frames).
    BanzaiBill {
        dir: u8,
        homing: bool,
    },
    /// Bumper: bounces the player (82-bumper.md).
    Bumper,
    /// Grinder (saw): hazard circle around (x, y + 24).
    Grinder,
    /// Seesaw plank `width` tiles wide pivoting at its centre (91-seesaw.md).
    Seesaw {
        width: u8,
    },
    /// Airship cannonball: straight line at 1 px/frame (velocity in vx, vy).
    Cannonball,
    /// Lakitu: hovers near the top of the view, drifts ±48 px around an anchor that follows the
    /// player, throws Spiny eggs every 60–119 frames (30-lakitu.md).
    Lakitu,
    /// A thrown Spiny egg: falls, becomes a Spiny on landing.
    SpinyEgg,
    /// Rideable Lakitu cloud (31-lakitu-cloud.md): bobs in place; the rider steers it.
    Cloud,
    /// POW block: a head hit from below sets off a quake (19-pow-block.md).
    Pow,
    /// Key: collect to open a locked door.
    Key,
    /// Twister: walks 0.5 px/frame; its wind column (grows 0.7/frame to 128 px) lifts riders.
    Twister,
    /// Swinging claw (105): a claw on a rope swinging from a pivot; grabs the player.
    Claw,
    /// Cheep Cheep (red = alternate): swims in water, flops and hops out of it.
    CheepCheep {
        red: bool,
    },
    /// Blooper: pulses toward the player, then sinks (48-blooper.md).
    Blooper,
    /// Skewer (83): a spiked pillar that thrusts out 144 px (dir 0 down, 1 left, 2 up, 3 right).
    Skewer {
        dir: u8,
    },
    /// Magikoopa: teleports near the view centre, casts magic at the player (39-magikoopa.md).
    Magikoopa,
    /// Magikoopa's magic: straight line at 0.75 px/frame (velocity in vx, vy).
    Magic,
    /// Rocky Wrench: pops out of its manhole and throws a wrench (58-rocky-wrench.md).
    RockyWrench,
    /// Thrown wrench: rises 0–30 px then flies at 1 px/frame; 480-frame life.
    Wrench,
    /// Wiggler: head + 4 trailing segments; a stomp makes it angry (52-wiggler.md).
    Wiggler,
    /// Goomba in a Goomba's Shoe (45; `heels` = Stiletto): hops toward the player; leaves the
    /// empty shoe when defeated.
    ShoeGoomba {
        heels: bool,
    },
    /// An empty Goomba's Shoe: the player climbs in by landing on it.
    Shoe {
        heels: bool,
    },
    /// A block that moves (wings or a track): brick, ?, hard, ice or cloud, with its contents.
    MovingBlock {
        block: crate::blocks::BlockKind,
        contents: Option<crate::blocks::Contents>,
    },
    /// Koopa Clown Car (42; `fire` = the Fire Koopa Clown Car). Phases: 0 empty (floats),
    /// 1 an enemy rides it (chases the player), 2 the player flies it (the world drives it),
    /// 3 stopped while its Koopaling rider is hit (KoopaJrStop).
    ClownCar {
        fire: bool,
    },
    /// Mushroom Trampoline (113-mushroom-trampoline.md): a bouncy top-only platform `width`
    /// tiles wide. `moving`: a lift that sways sideways ±`range` tiles at up to 0.5 px/f, easing
    /// at the ends [obs 0x7100897c20].
    Trampoline {
        width: u8,
        moving: bool,
        range: u8,
    },
    /// Crate (112-crate.md): carried and thrown; breaks on hitting a wall or an enemy after a
    /// throw (phase 1) and releases what it holds (vy 3.5). Phase 0 = resting (solid).
    Crate {
        contents: Option<crate::blocks::Contents>,
    },
    /// Track Block (85-track-block.md): a 2×2 solid that follows its own path (index `path`)
    /// at 0.5 px/f once ridden while the ON/OFF state is ON (the world passes it in
    /// `target_facing`). Phases: 0 wait, 1 forward, 2 back, 3 end stop (60 f), 4 blink (180 f),
    /// 5 gone, 6 appear (5 f). Normal blocks ping-pong; `alt` ones blink away and respawn.
    /// Path points in `trail`, next node in `chain`.
    TrackBlock {
        path: u8,
        alt: bool,
    },
    /// A fixed object riding a track (59-track.md): Fire Bar (`what` 0), Burner (1), airship
    /// Cannon (2) or Bill Blaster (3, `h` tiles tall). The actor is the moving solid block; the
    /// world moves the level's object (index `index` in its list) along with it.
    TrackMount {
        what: u8,
        index: u16,
        h: u8,
    },
    /// Story Mode stone (75-stone.md): carried like a crate but heavy — thrown it slides at most
    /// 0.8 px/f, slowing 0.1/frame; never breaks; phase 1 = thrown.
    Stone,
    /// Story Mode Toad to escort (73-toad.md): follows a point 32 px back along the player's
    /// path (`target`, set by the world), at 1.5 × the needed speed clamped 0.5–4 [obs], jumping
    /// where the path rises [inf]. If he's knocked out the run fails.
    EscortToad,
    /// Story Mode Toadette waiting at the goal (115): scenery.
    GoalToadette,
    /// A placed empty shell (Buzzy 1 / Spiny 2 / Dry Bones 3): becomes a still `Shell` with
    /// `shell_of` set on its first frame.
    EmptyShell {
        what: u8,
    },
    /// Koopa Troopa Car (72, 3D World): phases 0 empty (brakes 0.05/frame), 1 start spin,
    /// 2 run, 3 turn — the world drives it while the player rides; `hops` = damage taken
    /// (Damage state 0x71006cec20 counts it to 3; the third hit breaks it) [obs].
    KoopaCar,
    /// Super Hammer (116, 3D World): makes Builder Mario. An ItemDefault item that, like the Fire
    /// Flower, waits where it lands (landing choice vfunc +0x4d8 = 0x7100837860 → Wait) [obs].
    SuperHammer,
    /// Superball Flower (SMB1; 34-fire-flower.md) and the ball Superball Mario throws: 3 px/f
    /// on both axes, reflecting off surfaces, 270 frames (34-fire-flower.md) [ext: the bounce].
    SuperballFlower,
    Superball,
    /// A coin that moves (wings or a track): collected on touch.
    FloatCoin,
    /// Yoshi's Egg (45 in SMW / NSMBU): hatches into Yoshi; `red` = Red Yoshi (the egg's big
    /// bits, level.rs).
    YoshiEgg {
        red: bool,
    },
    /// Yoshi: ridden by the player (the world moves him), or waiting / running away (phase 1).
    Yoshi {
        red: bool,
    },
    /// Fish Bone (103): patrols in water, aims and charges; shatters on anything it hits.
    FishBone,
    /// Lava Bubble (60): leaps from the lava (or the area floor) to its placed height.
    LavaBubble,
    /// Monty Mole (102): buried until the player comes near, then chases.
    MontyMole,
    /// Angry Sun (104), screen-locked swooper; `moon` = the alternate form (wipes enemies).
    AngrySun {
        moon: bool,
    },
    /// Chain Chomp (61): chained to a stake at (home_x, home_y), or free (alternate form).
    ChainChomp {
        chained: bool,
    },
    /// Lava lift (skull raft, 64 px): drifts right at 0.8 (fast 2.5) once ridden.
    LavaLift {
        fast: bool,
    },
    /// Bowser (62): 20 HP (big 40), paces around home, jumps and breathes fire (actors/bosses.rs).
    /// `key`: holds a Key (child object 95) that drops on defeat; `clown`: rides a Koopa Clown
    /// Car (flag 0x200).
    Bowser {
        key: bool,
        clown: bool,
    },
    /// Bowser's (and Bowser Jr.'s) flame: flies level, settling onto its lane (`home_y`).
    BowserFire,
    /// Boom Boom (77): 3 HP plus a fireball counter; `pom` = Pom Pom (the 3D World alternate).
    BoomBoom {
        key: bool,
        clown: bool,
        pom: bool,
    },
    /// Pom Pom's shuriken: flies straight (velocity in vx, vy).
    Shuriken,
    /// Bowser Jr. (98): 3 HP plus a fireball counter.
    BowserJr {
        key: bool,
        clown: bool,
    },
    /// Ant Trooper (96; `horned` = Horned Ant Trooper): crawls along the terrain contour
    /// (actors/w3.rs, like the other 3D World enemies below).
    AntTrooper {
        horned: bool,
    },
    /// Stingby (65): a bee that patrols in the air and charges a level player.
    Stingby,
    /// Skipsqueak (51; `spiny` = Spiny Skipsqueak): jumps when the player jumps, keeps to home.
    Skipsqueak {
        spiny: bool,
    },
    /// Hop-Chops (50): patrols in hops, chases a player ahead of it.
    HopChops,
    /// Porcupuffer (114): 4-HP spiky fish, swims after the player or patrols the surface.
    Porcupuffer,
    /// Bully (117): charges the player; hits only push it back.
    Bully,
    /// Charvaargh (86): lava dragon that leaps out at a nearby player.
    Charvaargh,
    /// Piranha Creeper (107): a head on a stalk that creeps along path `path` (the area's
    /// piranha-creeper record of that index); `sleeps` = stays out at the end (alternate).
    PiranhaCreeper {
        path: u8,
        sleeps: bool,
    },
    /// Spike (74): turns to the player, lifts a spiked ball (a snowball in snow themes) and
    /// throws it (actors/v3.rs, like the other 2.0/3.0 parts below).
    Spike,
    /// A Spike's ball rolling along the ground (`snow`: a snowball).
    SpikeBall {
        snow: bool,
    },
    /// Pokey (78; `snow` = Snow Pokey): a walking cactus stack of `height` segments.
    Pokey {
        snow: bool,
        height: u8,
    },
    /// Mechakoopa (111): `weapon` 0 plain, 1 Blasta (missiles), 2 Zappa (laser).
    Mechakoopa {
        weapon: u8,
    },
    /// Blasta Mechakoopa's homing missile.
    MechaMissile,
    /// Zappa Mechakoopa's laser beam.
    MechaLaser,
    /// ON/OFF Trampoline (132, 3D World; `blue` = the alternate colour).
    OnOffTrampoline {
        blue: bool,
        width: u8,
    },
    /// A Koopaling (120–126; `who` 0 Lemmy, 1 Morton, 2 Larry, 3 Wendy, 4 Iggy, 5 Roy, 6 Ludwig)
    /// (actors/koopalings.rs). `key`: holds a Key; `clown`: rides a Koopa Clown Car.
    Koopaling {
        who: u8,
        key: bool,
        clown: bool,
    },
    /// A Koopaling's projectile (`what`: see actors/koopalings.rs).
    KoopalingShot {
        what: u8,
    },
    /// An update power-up item (actors/powerups.rs): `what` 0 the style's part-81 item, 1 the
    /// Master Sword, 2 + kind a wearable.
    PowerItem {
        what: u8,
    },
    /// Link's arrow, the boomerang, Link's bomb, the Cannon Box shot, the Red POW Box ring.
    PlayerArrow,
    PlayerBoomerang,
    LinkBomb,
    CannonShot,
    RedPowRing,
}

impl Kind {
    pub fn is_enemy(self) -> bool {
        matches!(
            self,
            Kind::Goomba
                | Kind::Goombrat
                | Kind::BabyBlooper { .. }
                | Kind::Bowser { .. }
                | Kind::BoomBoom { .. }
                | Kind::BowserJr { .. }
                | Kind::Koopa { .. }
                | Kind::Shell { .. }
                | Kind::Piranha { .. }
                | Kind::BulletBill
                | Kind::BanzaiBill { .. }
                | Kind::Cannonball
                | Kind::Lakitu
                | Kind::SpinyEgg
                | Kind::Magikoopa
                | Kind::RockyWrench
                | Kind::Wiggler
                | Kind::Spiny
                | Kind::Buzzy
                | Kind::SpikeTop { .. }
                | Kind::HammerBro
                | Kind::BobOmb
                | Kind::DryBones
                | Kind::Boo
                | Kind::RingBoo { .. }
                | Kind::ChainChomp { .. }
                | Kind::LavaBubble
                | Kind::MontyMole
                | Kind::AngrySun { moon: false }
                | Kind::ShoeGoomba { .. }
                | Kind::FishBone
        ) || self.w3_enemy()
            || self.v3_enemy()
    }
    /// Clear-condition enemy class (clear.rs) of a defeated actor of this kind.
    pub fn defeat_class(self, buzzy: bool) -> Option<u8> {
        use crate::clear::enemy as e;
        Some(match self {
            Kind::Goomba | Kind::Goombrat | Kind::ShoeGoomba { .. } => e::GOOMBA,
            Kind::Bowser { .. } => e::BOWSER,
            Kind::BoomBoom { pom: true, .. } => e::POM_POM,
            Kind::BoomBoom { .. } => e::BOOM_BOOM,
            Kind::BowserJr { .. } => e::BOWSER_JR,
            Kind::Koopa { .. } => e::KOOPA,
            Kind::Shell { .. } if buzzy => e::BUZZY,
            Kind::Shell { .. } => e::KOOPA,
            Kind::Buzzy => e::BUZZY,
            Kind::Spiny | Kind::SpinyEgg => e::SPINY,
            Kind::Piranha { fire: false } => e::PIRANHA,
            Kind::Piranha { fire: true } => e::FIRE_PIRANHA,
            Kind::Muncher => e::MUNCHER,
            Kind::DryBones => e::DRY_BONES,
            Kind::RockyWrench => e::ROCKY_WRENCH,
            Kind::HammerBro => e::HAMMER_BRO,
            Kind::Lakitu => e::LAKITU,
            Kind::Blooper => e::BLOOPER,
            Kind::CheepCheep { .. } => e::CHEEP_CHEEP,
            Kind::SpikeTop { .. } => e::SPIKE_TOP,
            Kind::ChainChomp { .. } => e::CHAIN_CHOMP,
            Kind::LavaBubble => e::LAVA_BUBBLE,
            Kind::FishBone => e::FISH_BONE,
            Kind::MontyMole => e::MONTY_MOLE,
            Kind::AngrySun { moon: false } => e::ANGRY_SUN,
            Kind::Wiggler => e::WIGGLER,
            Kind::BobOmb => e::BOB_OMB,
            Kind::Thwomp { .. } => e::THWOMP,
            Kind::Boo | Kind::RingBoo { .. } => e::BOO,
            Kind::BabyBlooper { .. } => e::BLOOPER,
            Kind::BanzaiBill { .. } => e::BANZAI_BILL,
            Kind::Magikoopa => e::MAGIKOOPA,
            Kind::BulletBill => e::BULLET_BILL,
            Kind::Spike | Kind::SpikeBall { .. } => e::SPIKE,
            Kind::Pokey { .. } => e::POKEY,
            Kind::Mechakoopa { .. } => e::MECHAKOOPA,
            // who: 0 Lemmy, 1 Morton, 2 Larry, 3 Wendy, 4 Iggy, 5 Roy, 6 Ludwig
            Kind::Koopaling { who, .. } => e::KOOPALINGS[[3, 5, 0, 2, 1, 4, 6][who as usize % 7]],
            _ => return None,
        })
    }
    /// Stomps bounce off without harming it or the player (EnemyDB "Invalid").
    pub fn stomp_bounces(self) -> bool {
        matches!(self, Kind::ChainChomp { .. })
    }
    /// Walks along the ground (turns at walls and other walkers).
    pub fn is_walker(self) -> bool {
        matches!(
            self,
            Kind::Goomba
                | Kind::Goombrat
                | Kind::Koopa { .. }
                | Kind::Shell { .. }
                | Kind::Spiny
                | Kind::Buzzy
                | Kind::SpikeTop { .. }
                | Kind::BobOmb
                | Kind::DryBones
        )
    }
    /// Hurts the player on touch but cannot be stomped.
    pub fn unstompable(self) -> bool {
        matches!(
            self,
            Kind::Piranha { .. }
                | Kind::Bowser { .. }
                | Kind::BowserFire
                | Kind::Shuriken
                | Kind::EnemyFire
                | Kind::Spiny
                | Kind::SpikeTop { .. }
                | Kind::Boo
                | Kind::RingBoo { .. }
                | Kind::Hammer
                | Kind::LavaBubble
                | Kind::AngrySun { .. }
        )
    }
    /// Walkers that turn back at ledges.
    pub fn ledge_turner(self) -> bool {
        matches!(
            self,
            Kind::Koopa { red: true } | Kind::DryBones | Kind::Goombrat
        )
    }
    /// Moves before the player and offers a solid surface (riders are carried).
    pub fn is_platform(self) -> bool {
        matches!(
            self,
            Kind::Thwomp { .. }
                | Kind::Lift { .. }
                | Kind::Trampoline { .. }
                | Kind::OnOffTrampoline { .. }
                | Kind::TrackBlock { .. }
                | Kind::TrackMount { .. }
                | Kind::Crate { .. }
                | Kind::Stone
                | Kind::Spring
                | Kind::SideSpring
                | Kind::Muncher
                | Kind::Donut
                | Kind::Seesaw { .. }
                | Kind::Cloud
                | Kind::Pow
                | Kind::LavaLift { .. }
                | Kind::MovingBlock { .. }
                | Kind::PSwitch
        )
    }
    pub fn is_item(self) -> bool {
        matches!(
            self,
            Kind::Mushroom
                | Kind::FireFlower
                | Kind::SuperballFlower
                | Kind::Star
                | Kind::OneUp
                | Kind::StyleItem
                | Kind::SuperHammer
                | Kind::PowerItem { .. }
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mode {
    /// Item rising out of (or sinking below) a block at constant speed, no gravity, until the
    /// feet reach `until`.
    Emerge {
        until: f32,
    },
    /// Coming out of a block (the Block context's Out, actors/block_out.rs): the block centre
    /// `yb`, where the motion started, this frame's gravity, whether it comes out downward
    /// and a parachute's countdown (a Goomba's hold in water, a preset's own count); terrain
    /// collision is off while `timer` runs. `wet`: it came out in water (the presets' water
    /// variants); `preset`: whose Out it runs; `note`: out of a note block (or a tree);
    /// `clear`: a coin is clear of the block (its hit box is in).
    BlockOut {
        yb: f32,
        y0: f32,
        gravity: f32,
        down: bool,
        left: i32,
        wet: bool,
        preset: block_out::Preset,
        note: bool,
        clear: bool,
    },
    /// Coming out of a pipe mouth (the Dokan context, actors/dokan.rs): the slide's target
    /// (tx, ty) and where it is in the context.
    DokanOut {
        tx: f32,
        ty: f32,
        state: Dokan,
    },
    /// Fired from a Bill Blaster (the Killer context's Shot, actors/shot.rs): the launch point,
    /// frames since the shot, the alternate blaster's fast flag, and whether it has landed.
    Shot {
        x0: f32,
        y0: f32,
        t: u16,
        fast: bool,
        landed: bool,
        /// Fired in water (the Fish Bone's and Wiggler's water variants, actors/shot.rs).
        wet: bool,
    },
    /// Drifting down under a parachute (shared enemy draft §5.2): gravity −0.01875, max fall
    /// −0.4, swinging ±24 px (sin of `phase`, `rate` ±2.5°/frame, doubled while `held` at a
    /// wall); landing drops the parachute.
    Parachute {
        phase: f32,
        rate: f32,
        held: bool,
    },
    Normal,
    /// Flattened Goomba (DieStamp), deleted when the timer runs out.
    Stomped,
    /// Knocked off (DieFall): no collision, falls off the area.
    DieFall,
    ShellWait,
    ShellSlide,
    /// Thrown by the SMB2 Mushroom (actors/powerups.rs): flies until it lands.
    Thrown,
    /// Carried through a clear pipe (the DrawDokan context's In, world/clear_pipe.rs): the world
    /// moves it; no collision, no contact.
    ClearPipe,
    /// Remove at the end of the frame.
    Dead,
}

impl Mode {
    /// Opening a parachute: the swing starts at φ = 0, rightward, or leftward (−2.5°/frame)
    /// for a part shot from a blaster or thrown by Lakitu facing left (Fall 0x71005f4210,
    /// v3.0.3 0x7101058540) [obs].
    pub fn parachute(left: bool) -> Mode {
        Mode::Parachute {
            phase: 0.0,
            rate: if left { -CHUTE_RATE } else { CHUTE_RATE },
            held: false,
        }
    }
}

/// The parachute's swing: 2.5°/frame (period 144 frames), ±24 px [obs].
const CHUTE_RATE: f32 = 2.5;
const CHUTE_SWING: f32 = 24.0;

/// What knocked an actor out (its defeat sound: `r_damage_<cause>`, else `r_damage`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DieCause {
    #[default]
    Other,
    Fire,
    Shell,
    Star,
    Tail,
    Quake,
    Blast,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Actor {
    /// What knocked it out (DieFall), for its defeat sound; a shell's knock-outs count up
    /// along its chain (1, 2, …: the defeat sound's combo).
    pub die_cause: DieCause,
    pub die_combo: u32,
    pub kind: Kind,
    pub mode: Mode,
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    /// +1 right, −1 left
    pub dir: i32,
    pub on_ground: bool,
    pub big: bool,
    pub wings: bool,
    pub timer: i32,
    pub turn_lock: i32,
    pub age: u32,
    /// Frames during which a just-kicked shell cannot hurt the kicker.
    pub kick_lock: i32,
    /// DieFall speeds taken on the next update (die_by).
    pub die_pending: Option<(f32, f32)>,
    /// DieFall updates so far.
    pub die_frames: u16,
    /// A walker's frames standing after a landing.
    pub land_wait: u8,
    /// A walker's enemy contact on its front side at the end of last frame (EnemySys's touch
    /// flag: Walk turns on it; in Turn it stops the walker for the rest of the turn).
    pub touch_front: bool,
    /// A walker in Turn stopped by an enemy in front (vx 0 until the turn ends).
    pub turn_stop: bool,
    /// A walker's wall contact at the end of last frame (Walk reads it a frame late).
    pub wall_prev: bool,
    /// An item touched by the player: 1 = touched at the end of last frame (this update moves it
    /// once more, without landing, and takes it), 2 = taken (the world collects it), 3 = gone.
    pub taken: u8,
    /// A kick's slide speed (signed), taken up by the shell's next update: the kick message
    /// is handled a frame later [obs: Ryujinx trace M1_koopa koopa_kick, vx still 0 on the
    /// kick frame, moving at the kick speed from the next].
    pub kick_speed: f32,
    /// A sliding shell hit a wall last frame: it turns back at the start of this update [obs:
    /// Ryujinx trace M1_koopa koopa_kick, flush against the wall with the old speed for a frame].
    pub bounce_pending: bool,
    /// Kill chain (shells) / stomp chain bookkeeping.
    pub chain: u32,
    /// Paragoomba hop cycle and pause.
    pub hops: u8,
    pub pause: i32,
    pub home_y: f32,
    pub home_x: f32,
    /// Index into World::spawns, if this actor came from a course object.
    pub spawn: Option<usize>,
    /// Hit a wall this frame (in the movement direction).
    pub wall: bool,
    /// Pipe this actor lives in (pipe Piranha) and its mouth (entry point).
    pub pipe: Option<(PipeDir, f32, f32)>,
    /// Index of the pipe generator that made it.
    pub generator: Option<usize>,
    /// Behaviour phase for actors with small state machines (Piranha).
    pub phase: u8,
    /// Wiggler: recent head positions (newest last), for the trailing segments.
    pub trail: Vec<(f32, f32)>,
    /// Per-actor random state (xorshift32), seeded at spawn.
    pub rng: u32,
    /// Nearest player's position, set by the world before each step.
    pub target: (f32, f32),
    /// Stable id (moving solids are tracked by it).
    pub id: u32,
    /// Position before this frame's move (the world sets it; a stomp undoes the move: the
    /// game's enemies update after the player-contact check) [obs: Ryujinx trace M1_koopa].
    pub pre: (f32, f32),
    /// Something stands on this platform (set by the world).
    pub ridden: bool,
    /// Position change this frame (for carrying riders).
    pub moved: (f32, f32),
    /// This shell belongs to a Buzzy Beetle (fire-proof; wakes as a Buzzy).
    pub buzzy: bool,
    /// An empty shell placed as such (the 0x4 forms): 1 Buzzy shell, 2 Spiny shell, 3 Dry
    /// Bones shell (0 = a shell with its owner inside). Empty shells never wake; the Buzzy and
    /// Spiny ones are worn as helmets, the Dry Bones one is ridden.
    pub shell_of: u8,
    /// The placed object's alternate form (flag 0x4) where the kind doesn't encode it: a fast
    /// Spike Top, a lit Bob-omb, …
    pub alt: bool,
    /// Floating movers (Boo): speed along `heading` (degrees).
    pub speed: f32,
    pub heading: f32,
    /// The player's facing (+1 right), set by the world.
    pub target_facing: i32,
    /// The player's height as the shared player-in-box query sees it (`Player::query_height`),
    /// set by the world.
    pub target_h: f32,
    /// Riding a track, and the offset from the track point to (x, y).
    pub rail: Option<crate::track::Rail>,
    pub rail_off: (f32, f32),
    /// Left its track at an open end: falls with its last speed (objects without gravity of
    /// their own; enemies fall by their own rules).
    pub derailed: bool,
    /// Carries a parachute it has not opened yet (out of a block, a pipe or a blaster): it
    /// opens when that context ends.
    pub chute: bool,
    /// The part it holds (`Level::held`): id, flags.
    pub holds: Option<(i16, u32)>,
    /// A coin thrown by a Lakitu, still in its arc (`coin_thrown`).
    pub thrown: bool,
    /// Counted toward a "defeat N" clear condition.
    pub tallied: bool,
    /// Carried by the player: the world positions it; it doesn't step on its own.
    pub held: bool,
    /// Set by the world each frame: the camera's bottom-left corner, and the reference level a
    /// Lava Bubble leaps from (the liquid surface, else the area bottom − 12).
    pub camera: (f32, f32),
    pub floor_ref: f32,
    /// 3D World enemy state and what the world senses for them (actors/w3.rs).
    pub w3: W3State,
    /// SMM1 shell physics: a shell kicked by the mid-air wall assist — its stop point (x) and
    /// kick direction (world/smm1.rs).
    pub assist: Option<(f32, i32)>,
    /// SMM1 extras: this Super Mushroom is a Weird Mushroom (1) or a Mystery Mushroom (2)
    /// (`Options::smb1_mushroom`; SMM1 keeps them as Super Mushroom actors with a subtype).
    pub smm1_item: u8,
    /// State of the 2.0 / 3.0 parts (actors/v3.rs, actors/koopalings.rs).
    pub v3: V3State,
    /// The id of the actor that threw this one (a Lakitu's Spiny egg), and, for a thrower, how
    /// many of its children are alive (set by the world before it steps).
    pub parent: Option<u32>,
    pub children: u32,
    /// The Bill Blaster (`Level::blasters` index) that fired it: each blaster keeps at most
    /// its part's `spout_killer_count` alive.
    pub blaster: Option<usize>,
    /// Rides a tower (a stacked member above the base): the world places it, and its own
    /// motion is skipped (world/tower.rs).
    pub stacked: bool,
    /// A rider's drawn sideways sway (px), its amplitude and phase (degrees).
    pub sway: f32,
    pub sway_amp: f32,
    pub sway_phase: u32,
    /// A Fire Koopa Clown Car's fire record: on the car, or on a boss riding one (actors/clown.rs).
    pub clown_fire: Option<ClownFire>,
    /// A Spiny or Buzzy Beetle walking upside down on the ceiling (actors/ceiling.rs).
    pub ceiling: bool,
    /// A shell that is a Spiny dropped off the ceiling (spiky; wakes as a Spiny). Not `alt`,
    /// which is the placement's 0x4 flag (a red Koopa's shell has it).
    pub spiny_shell: bool,
    /// The shared winged flight (items, coins, map objects and blocks with wings; actors/fly.rs).
    pub fly: Fly,
    /// A Swinging Claw's rope; the actor's own position is the rope's end (actors/claw.rs).
    pub claw: Option<ClawRope>,
    /// A Seesaw's plank (actors/seesaw.rs); the torque of its other riders this frame (set by
    /// the world); the Seesaw a walker stands on.
    pub plank: Option<seesaw::Plank>,
    pub riders_tau: f32,
    pub seesaw: Option<u32>,
    /// A plank walker's ground angle: this frame's and last frame's.
    pub plank_angle: (u32, u32),
    /// Bowser's middle jump is a gap jump (its speed set to land past the gap; MiddleJump's
    /// enter 0x71006b0370 flag +0x90).
    pub gap_jump: bool,
    /// A winged enemy's wing helper: its flap rate (+0x20), eased toward the rate its body's
    /// animation asks for (actors/boo.rs).
    pub wing_rate: f32,
    /// A winged Hammer Bro's WingWalk (actors/bros.rs).
    pub bro_wing: BroWing,
    /// NSMBU's Propeller Mushroom's Move (actors/propeller.rs).
    pub prop: Prop,
    /// Bowser: the Trampoline bodies in the area (x0, y0, x1, y1; set by the world), what his
    /// jump-height search found over him (height from his feet), and his jump reaches
    /// (actors/bosses.rs `bowser_height_search`).
    pub overhead_units: Vec<(f32, f32, f32, f32)>,
    pub spring_over: Option<f32>,
    pub spring_reach_little: f32,
    pub spring_reach_mid: f32,
    pub spring_reach_big: f32,
    pub spring_reach_third: f32,
    pub spring_reach_body: f32,
    /// Bowser under the water-speed rule: the reaches above are the water ones.
    pub spring_slow: bool,
    /// A thrower's reservation in the area's projectile budget (actors/budget.rs).
    pub budget: Reserve,
    /// SMW / NSMBU Bowser's own states and falling-fire rain (actors/bosses/styles.rs).
    pub koopa: BowserStyle,
}

impl Actor {
    pub fn new(kind: Kind, x: f32, y: f32) -> Self {
        Self {
            die_cause: DieCause::Other,
            die_combo: 0,
            kind,
            mode: Mode::Normal,
            x,
            y,
            pre: (x, y),
            vx: 0.0,
            vy: 0.0,
            dir: 1,
            on_ground: false,
            big: false,
            wings: false,
            timer: 0,
            turn_lock: 0,
            age: 0,
            kick_lock: 0,
            die_pending: None,
            die_frames: 0,
            land_wait: 0,
            touch_front: false,
            turn_stop: false,
            wall_prev: false,
            taken: 0,
            kick_speed: 0.0,
            bounce_pending: false,
            chain: 0,
            hops: 0,
            pause: 0,
            home_y: y,
            home_x: x,
            spawn: None,
            wall: false,
            pipe: None,
            trail: Vec::new(),
            generator: None,
            phase: 0,
            rng: 0x9e37_79b9 ^ (x.to_bits().rotate_left(7)) ^ y.to_bits(),
            target: (0.0, 0.0),
            id: 0,
            ridden: false,
            moved: (0.0, 0.0),
            buzzy: false,
            shell_of: 0,
            alt: false,
            speed: 0.0,
            heading: 0.0,
            target_facing: 1,
            target_h: 16.0,
            rail: None,
            rail_off: (0.0, 0.0),
            derailed: false,
            chute: false,
            holds: None,
            thrown: false,
            tallied: false,
            held: false,
            camera: (0.0, 0.0),
            floor_ref: 0.0,
            w3: W3State::default(),
            assist: None,
            smm1_item: 0,
            v3: V3State::default(),
            parent: None,
            children: 0,
            blaster: None,
            stacked: false,
            sway: 0.0,
            sway_amp: 0.0,
            sway_phase: 0,
            clown_fire: matches!(kind, Kind::ClownCar { fire: true }).then(ClownFire::default),
            ceiling: false,
            spiny_shell: false,
            fly: Fly::default(),
            claw: None,
            plank: None,
            riders_tau: 0.0,
            seesaw: None,
            plank_angle: (0, 0),
            gap_jump: false,
            wing_rate: 1.0,
            bro_wing: BroWing::default(),
            prop: Prop::default(),
            overhead_units: Vec::new(),
            spring_over: None,
            spring_reach_little: 0.0,
            spring_reach_mid: 0.0,
            spring_reach_big: 0.0,
            spring_reach_third: 0.0,
            spring_reach_body: 0.0,
            spring_slow: false,
            budget: Reserve::default(),
            koopa: BowserStyle::default(),
        }
    }

    /// Fireballs bounce off / are snuffed without killing.
    pub fn fire_proof(&self) -> bool {
        matches!(
            self.kind,
            Kind::Buzzy
                | Kind::SpikeTop { .. }
                | Kind::DryBones
                | Kind::Boo
                | Kind::RingBoo { .. }
                | Kind::Thwomp { .. }
                | Kind::ChainChomp { .. }
                | Kind::LavaBubble
                | Kind::AngrySun { .. }
                | Kind::FishBone
        ) || (matches!(self.kind, Kind::Shell { .. }) && self.buzzy)
    }

    /// The solid surface this actor offers (for platforms): (x0, y0, x1, y1, top_only).
    /// A flying winged Buzzy Beetle or Spike Top, the ones players stand on (actors/fly.rs).
    pub fn is_para_beetle(&self) -> bool {
        matches!(self.kind, Kind::Buzzy | Kind::SpikeTop { .. })
            && self.wings
            && self.mode == Mode::Normal
            && !self.ceiling
            && !self.thrown
            && !self.held
            && self.rail.is_none()
            && self.pipe.is_none()
    }

    pub fn solid_box(&self) -> Option<(f32, f32, f32, f32, bool)> {
        match self.kind {
            // the P Switch is stood on (16 px; 4.8 once pressed) [obs: Ryujinx trace M1_pswitch];
            // unpressed, its sides stop the player too [obs: Ryujinx trace W_3145588, walking
            // into one: x held 7.9 px from its side]
            Kind::PSwitch if self.alive() => {
                let h = if self.phase > 0 { 4.8 } else { 16.0 };
                Some((
                    self.x - 8.0,
                    self.y,
                    self.x + 8.0,
                    self.y + h,
                    self.phase > 0,
                ))
            }
            // a Para-Beetle is stood on (its body box, from above) (actors/fly.rs)
            Kind::Buzzy | Kind::SpikeTop { .. } if self.is_para_beetle() => {
                let (hw, h) = (self.half_w(), self.height());
                Some((self.x - hw, self.y, self.x + hw, self.y + h, true))
            }
            Kind::Thwomp { .. } if self.mode == Mode::Normal => {
                let hw = if self.big { 32.0 } else { 16.0 };
                Some((self.x - hw, self.y, self.x + hw, self.y + 2.0 * hw, false))
            }
            Kind::Lift { width, .. } => {
                // the top 7.95 px over its centre (thickness parameter DAT_7101c8ab04, 11-lift.md)
                // [obs: Ryujinx trace M1_lift, the rider at 79.95 on a lift centred at 72]
                let hw = width as f32 * 8.0;
                Some((self.x - hw, self.y + 8.0, self.x + hw, self.y + 15.95, true))
            }
            Kind::Trampoline { width, .. } => {
                let hw = width as f32 * 8.0;
                Some((self.x - hw, self.y + 8.0, self.x + hw, self.y + 16.0, true))
            }
            Kind::TrackMount { h, .. } => Some((
                self.x - 8.0,
                self.y,
                self.x + 8.0,
                self.y + h as f32 * TILE,
                false,
            )),
            Kind::TrackBlock { .. } if self.phase != 5 => {
                Some((self.x - 16.0, self.y, self.x + 16.0, self.y + 32.0, false))
            }
            Kind::Stone if !self.held && self.phase == 0 && self.mode == Mode::Normal => {
                Some((self.x - 8.0, self.y, self.x + 8.0, self.y + 16.0, false))
            }
            Kind::Crate { .. } if !self.held && self.phase == 0 && self.mode == Mode::Normal => {
                Some((self.x - 8.0, self.y, self.x + 8.0, self.y + 16.0, false))
            }
            Kind::Muncher if self.mode == Mode::Normal => {
                Some((self.x - 8.0, self.y, self.x + 8.0, self.y + 16.0, false))
            }
            Kind::Cloud => Some((
                self.x - 16.0,
                self.y + 4.0,
                self.x + 16.0,
                self.y + 12.0,
                true,
            )),
            Kind::LavaLift { .. } => Some((
                self.x - 32.0,
                self.y + 8.0,
                self.x + 32.0,
                self.y + 16.0,
                true,
            )),
            Kind::SideSpring => Some((self.x - 8.0, self.y, self.x + 8.0, self.y + 16.0, false)),
            Kind::Pow if self.mode == Mode::Normal => {
                Some((self.x - 8.0, self.y, self.x + 8.0, self.y + 16.0, false))
            }
            Kind::Seesaw { width } => {
                // the plank turns about a point 8 px under its top (the pivot): the top line is
                // its ends' line 8 px up, from end to end, its height here taken at the middle
                // (actors/seesaw.rs) [obs: Ryujinx trace W_3010660: stood on with the centre
                // 3.6 px past the turned plank's end, on its line]
                let p = self.plank.unwrap_or_else(|| seesaw::Plank::new(width));
                let (e, py) = (p.ends, self.y + 8.0);
                let mid = py + seesaw::HALF + (e[1] + e[3]) * 0.5;
                Some((self.x + e[0], py, self.x + e[2], mid, true))
            }
            // a hidden block is no solid until it's bumped from below
            Kind::MovingBlock {
                block: crate::blocks::BlockKind::Hidden,
                ..
            } => None,
            Kind::MovingBlock { block, .. } => {
                use crate::blocks::BlockKind as B;
                let top_only = matches!(block, B::Cloud | B::Donut);
                Some((self.x - 8.0, self.y, self.x + 8.0, self.y + 16.0, top_only))
            }
            Kind::Donut => Some((
                self.x - 8.0,
                self.y + 8.0,
                self.x + 8.0,
                self.y + 16.0,
                true,
            )),
            Kind::Spring if self.mode == Mode::Normal => {
                // top 13.5 px up [obs: Ryujinx trace M1_spring, landing at y + 13.5]
                let top = self.y + 13.5 - self.aux_compression();
                Some((self.x - 8.0, top - 4.0, self.x + 8.0, top, true))
            }
            k if k.is_v3() => self.v3_solid_box(),
            _ => None,
        }
    }

    /// Top-surface slope of a platform (seesaw tilt: `heading` in degrees).
    pub fn solid_slope(&self) -> f32 {
        match self.kind {
            Kind::Seesaw { .. } => self.plank.map_or(0.0, |p| p.slope()),
            _ => 0.0,
        }
    }

    /// The 3D World Goomba's sight (0-goomba.md "Kuribo3W extra states", Basic vtable
    /// 0x7102170c58, v1.0.0): walking on the ground (`phase` 0) it spots a player in front within
    /// 80 px, from 16 px under its feet to 32 over (0x71006f91c0), and stops for its notice
    /// animation (Find, 7), 53 frames counting the one it sees [obs: Ryujinx trace X_cpipeg, a
    /// Goomba landing out of a clear pipe 62 px from the player: still frames 268..320, running
    /// from 321]; then runs at 0.7 (Run, 8) for at least 10 frames, until the player is out of
    /// 128 px sideways, 80 up or 32 down, or behind it for 16 frames (0x71006f97a0); then slows
    /// to a stop by 0.05 a frame (Lost, 9) and walks on [inf: the Lost animation's end taken as
    /// the stop]. The speed to walk at this frame (0 while it stands).
    fn kuribo3w(&mut self) -> f32 {
        const WALK: f32 = 0.3;
        if self.wings || self.big {
            return WALK;
        }
        let (dx, dy) = (self.target.0 - self.x, self.target.1 - self.y);
        let ahead = dx * self.dir as f32 > 0.0;
        match self.phase {
            7 => {
                self.timer -= 1;
                if self.timer > 0 {
                    return 0.0;
                }
                (self.phase, self.timer, self.pause) = (8, 0, 0);
                0.7
            }
            8 => {
                self.timer += 1;
                self.pause = if ahead { 0 } else { self.pause + 1 };
                let lost = dx.abs() > 128.0 || dy > 80.0 || dy < -32.0 || self.pause >= 16;
                if self.timer > 10 && lost {
                    self.phase = 9;
                }
                0.7
            }
            9 => {
                // (Lost eases its speed, not set by the walk below)
                self.vx -= self.vx.clamp(-0.05, 0.05);
                if self.vx == 0.0 {
                    self.phase = 0;
                }
                self.vx.abs()
            }
            _ => {
                if self.on_ground && ahead && dx.abs() <= 80.0 && (-16.0..=32.0).contains(&dy) {
                    (self.phase, self.timer) = (7, 53);
                    return 0.0;
                }
                WALK
            }
        }
    }

    /// Seesaw (91-seesaw.md, actors/seesaw.rs): the riders' torque turns the plank. The player
    /// weighs 2, from the third frame its centre is over the plank (±8·W) (its riders' list lags
    /// as the P Switch's does), then as long as it stands on it, a foot past the end too [obs:
    /// Ryujinx trace W_3010660: on from x 289.5 over an end at 288, still at 370.4 over one at
    /// 366.8], where it stands after its move.
    fn step_seesaw(&mut self, width: u8) {
        let on =
            self.ridden && (self.hops > 0 || (self.target.0 - self.x).abs() <= 8.0 * width as f32);
        self.hops = if on { self.hops.saturating_add(1) } else { 0 };
        let mut plank = self.plank.unwrap_or_else(|| seesaw::Plank::new(width));
        // (a ground pound landing on it this frame: `chain` set by the world)
        if std::mem::take(&mut self.chain) == 1 && on {
            let (sin, _) = crate::fmath::table_sin_cos(plank.theta as u32);
            plank.slam(width, self.target.0 >= self.x + sin * -8.0);
        }
        let mut tau = if self.hops >= 3 && !plank.slammed {
            let (rx, ry) = self.target;
            seesaw::torque(plank.theta, self.x, self.y + 8.0, rx, ry, 2.0)
        } else {
            0.0
        };
        // the walkers standing on it weigh 2 too, where they stand before their move [obs:
        // Ryujinx trace X_seesaw2, a Goomba alone: +0x84c matched to 1e-4]
        if !plank.slammed {
            tau += std::mem::take(&mut self.riders_tau);
        }
        plank.step(width, tau);
        self.speed = plank.omega;
        self.heading = (plank.theta as f64 * (360.0 / 4_294_967_296.0)) as f32;
        self.plank = Some(plank);
    }

    /// Lakitu: anchor x (home_x) follows the player by at most max(|player vx|, 0.05) — the
    /// player's x-speed comes in `speed`; drift toward ±1.5 (accel 0.05), turning 48 px either
    /// side of the anchor; throw every 60–119 frames, the countdown running only inside that
    /// band, with fewer than 10 of its eggs / Spinies alive (the spawn limiter 0x7100364d80:
    /// Togezo's spout_jugem_count) and the box x ± 40, y + 18…28 clear of terrain; release 30
    /// frames into the throw (re-checking the limiter) [obs 0x7100684a10]. The limiter has no
    /// lifetime budget for Spinies — a Lakitu throws for ever (the budget flag is set only for
    /// a child matching a "defeat N" clear condition, 0x7100364500; not modelled).
    fn step_lakitu(&mut self, level: &Level) -> Option<Actor> {
        let (px, _) = self.target;
        let follow = self.speed.abs().max(0.05);
        self.home_x += (px - self.home_x).clamp(-follow, follow);
        if self.x > self.home_x + 48.0 {
            self.dir = -1;
        } else if self.x < self.home_x - 48.0 {
            self.dir = 1;
        }
        let t = 1.5 * self.dir as f32;
        self.vx += (t - self.vx).clamp(-0.05, 0.05);
        self.x += self.vx;
        if self.age <= 1 {
            self.timer = 60 + self.rand(60) as i32;
        }
        let room = self.children < LAKITU_CHILDREN;
        if self.pause > 0 {
            // the throw animation, `pause` frames to the release
            self.pause -= 1;
            if self.pause == 0 {
                self.timer = 60 + self.rand(60) as i32;
                if room {
                    // its part, through the Jugem (thrown) context; a Spiny egg by default
                    // [inf: the thrown state's motion for parts other than the egg]
                    let face = if px < self.x { -1.0 } else { 1.0 };
                    // (a Key is held, dropped on defeat, not thrown)
                    let part = self
                        .holds
                        .filter(|&(id, _)| id != 95)
                        .and_then(|(id, flags)| level.part_actor(id, flags));
                    let kind = part.map_or(Kind::SpinyEgg, |p| p.0);
                    let mut egg = Actor::new(kind, self.x, self.y + 16.8);
                    egg.vx = (self.vx.abs() * 0.9).max(0.5) * face;
                    egg.vy = 2.0;
                    egg.dir = face as i32;
                    egg.parent = Some(self.id);
                    if let Some((_, wings, parachute, big)) = part {
                        (egg.wings, egg.big) = (wings, big);
                        if parachute {
                            egg.vx = 0.0;
                            egg.mode = Mode::parachute(face < 0.0);
                        }
                    }
                    return Some(egg);
                }
            }
        } else if room && (self.x - self.home_x).abs() < 48.0 {
            if self.timer > 0 {
                self.timer -= 1;
            }
            let blocked = [-40.0, -24.0, -8.0, 8.0, 24.0, 40.0].iter().any(|&dx| {
                level.cell_at(self.x + dx, self.y + 18.0).is_solid()
                    || level.cell_at(self.x + dx, self.y + 28.0).is_solid()
            });
            if self.timer == 0 && !blocked {
                self.pause = 30;
            }
        }
        None
    }

    /// Skewer extension along its direction (`speed` = velocity, `heading` = extension, px):
    /// wait 100, wind back to −34 (0.15/frame², ≤ 1.5), hold 40, thrust (0.5, ≤ 7) to 144,
    /// hold 60, retract (0.1, ≤ 2) to 0, repeat.
    fn step_skewer(&mut self) {
        match self.phase {
            0 => {
                self.timer += 1;
                if self.timer >= 100 {
                    self.phase = 1;
                    self.timer = 0;
                }
            }
            1 => {
                self.speed = (self.speed - 0.15).max(-1.5);
                self.heading = (self.heading + self.speed).max(-34.0);
                if self.heading <= -34.0 {
                    self.phase = 2;
                    self.speed = 0.0;
                }
            }
            2 => {
                self.timer += 1;
                if self.timer >= 40 {
                    self.phase = 3;
                    self.timer = 0;
                }
            }
            3 => {
                self.speed = (self.speed + 0.5).min(7.0);
                self.heading = (self.heading + self.speed).min(144.0);
                if self.heading >= 144.0 {
                    self.phase = 4;
                    self.speed = 0.0;
                }
            }
            4 => {
                self.timer += 1;
                if self.timer >= 60 {
                    self.phase = 5;
                    self.timer = 0;
                }
            }
            _ => {
                self.speed = (self.speed - 0.1).max(-2.0);
                self.heading = (self.heading + self.speed).max(0.0);
                if self.heading <= 0.0 {
                    self.phase = 0;
                    self.speed = 0.0;
                }
            }
        }
    }

    /// Rocky Wrench: hidden (30–59) → peek +2 at 0.2/frame for 96 → rise to +10 at 0.4 →
    /// throw → sink at 0.8. `heading` holds the height above home; only a risen one can be hit.
    fn step_rocky(&mut self) -> Option<Actor> {
        let (px, _) = self.target;
        self.dir = if px < self.x { -1 } else { 1 };
        let mut thrown = None;
        match self.phase {
            0 => {
                self.heading = 0.0;
                if self.timer <= 0 {
                    self.timer = 30 + self.rand(30) as i32;
                    self.phase = 1;
                }
            }
            1 => {
                self.timer -= 1;
                if self.timer <= 0 {
                    self.phase = 2;
                    self.timer = 96;
                }
            }
            2 => {
                self.heading = (self.heading + 0.2).min(2.0);
                self.timer -= 1;
                if self.timer <= 0 {
                    self.phase = 3;
                }
            }
            3 => {
                self.heading = (self.heading + 0.4).min(10.0);
                // up: the throw needs a unit of the area's projectile budget; refused, the
                // helper tries again in 40 + U[0,30) frames (0x710121e7f0) [obs v3.0.3]
                if self.pause > 0 {
                    self.pause -= 1;
                } else if self.heading >= 10.0 {
                    if self.budget_request(1) {
                        self.phase = 4;
                        self.timer = 47;
                    } else {
                        self.pause = 40 + self.rand(30) as i32;
                    }
                }
            }
            4 => {
                self.timer -= 1;
                if self.timer <= 0 {
                    let d = self.dir as f32;
                    let mut w = Actor::new(Kind::Wrench, self.x - 2.0 * d, self.home_y + 28.0);
                    w.home_y = w.y + 5.0 * self.rand(7) as f32;
                    w.vx = d;
                    w.timer = 480;
                    thrown = Some(w);
                    self.phase = 5;
                }
            }
            _ => {
                self.heading = (self.heading - 0.8).max(0.0);
                if self.heading <= 0.0 {
                    self.phase = 0;
                    self.timer = 0;
                }
            }
        }
        self.y = self.home_y + self.heading;
        thrown
    }

    /// Wiggler segment positions (4, nearest first), 16 px apart along the head's path.
    pub fn wiggler_segments(&self) -> Vec<(f32, f32)> {
        let mut out = Vec::new();
        let mut dist = 0.0;
        let mut want = 16.0;
        let mut prev = (self.x, self.y);
        for &p in self.trail.iter().rev() {
            dist += ((p.0 - prev.0).powi(2) + (p.1 - prev.1).powi(2)).sqrt();
            prev = p;
            if dist >= want {
                out.push(p);
                want += 16.0;
                if out.len() == 4 {
                    break;
                }
            }
        }
        while out.len() < 4 {
            let k = out.len() as f32 + 1.0;
            out.push((self.x - self.dir as f32 * 16.0 * k, self.y));
        }
        out
    }

    /// The Skewer's spiked head box: 64 wide × 32 deep at the extended end.
    pub fn skewer_head(&self) -> (f32, f32, f32, f32) {
        let (cx, cy) = (self.x, self.y + 32.0);
        let e = self.heading;
        match self.kind {
            Kind::Skewer { dir: 0 } => (cx - 32.0, cy - 32.0 - e, cx + 32.0, cy - e),
            Kind::Skewer { dir: 2 } => (cx - 32.0, cy + e, cx + 32.0, cy + 32.0 + e),
            Kind::Skewer { dir: 1 } => (cx - 32.0 - e, cy - 32.0, cx - e, cy + 32.0),
            _ => (cx + e, cy - 32.0, cx + 32.0 + e, cy + 32.0),
        }
    }

    /// Swinging claw tip (where a held player hangs): the rope's end, the claw actor's own
    /// position (actors/claw.rs); the pivot is `home`.
    pub fn claw_tip(&self) -> (f32, f32) {
        (self.x, self.y)
    }

    /// Spring compression c (0…4 px), kept in `pause`.
    pub fn aux_compression(&self) -> f32 {
        if self.kind == Kind::Spring {
            self.speed
        } else {
            0.0
        }
    }

    /// A player in the Side Thwomp's attack box (see `step_thwomp`): the shared player-in-box
    /// query in its feet-span mode, the player's x inside the box and its feet-to-height span
    /// (`target_h`) overlapping it [obs v3.0.3 0x7100d7fed0].
    fn side_thwomp_sees(&self, level: &Level, right: bool) -> bool {
        let (px, py) = self.target;
        let s = if self.big { 2.0 } else { 1.0 };
        let back = if level.style == "3W" { 8.0 } else { 6.0 } * s;
        let (x0, x1) = if right {
            (self.x - back, self.x + 136.0)
        } else {
            (self.x - 136.0, self.x + back)
        };
        let (cy, hh) = (self.y + 16.0 * s, 16.0 * s + 16.0);
        px >= x0 && px <= x1 && py < cy + hh && py + self.target_h > cy - hh
    }

    /// A Side Thwomp's charge starts: it faces its side and returns here afterwards.
    fn charge_from(&mut self, side: bool, right: bool) {
        if side {
            self.dir = if right { 1 } else { -1 };
            self.home_x = self.x;
        }
    }

    /// Thwomp (12-thwomp.md): Wait (notices a player below within |dx| < W + 32) → Fall
    /// (g −0.25, cap −5) → FallWait 64 → Return (≤ 0.875, +0.2/frame) to the home height.
    /// Side Thwomp (SideDossun): looks one way only; a player in the attack box on that side —
    /// 136 px long from 6·S px behind its centre (8·S in 3D World), centred 16·S above its feet
    /// with half-height 16·S + 16 (S = 1, big 2) — sets it charging that way (0.075/frame² to
    /// 3.0, no gravity) until it hits a wall; FallWait 64, then back to its start at up to
    /// 0.875 px/f (±0.2/frame) [obs Wait 0x7100768800, box 0x710076ab10, Fall 0x7100769210,
    /// Return 0x710076a140].
    fn step_thwomp(&mut self, level: &Level, side: bool, right: bool) {
        let (px, py) = self.target;
        let w = if self.big { 32.0 } else { 16.0 };
        let below = (px - self.x).abs() < w + 32.0 && py > self.y + w - 260.0 && py < self.y + w;
        let triggered = |a: &Actor| {
            if side {
                a.side_thwomp_sees(level, right)
            } else {
                below
            }
        };
        match self.phase {
            0 if self.wings => {
                // winged: patrol at 0.3 px/f (0.003/frame²), turning at walls, bobbing with
                // vy = 0.15·sin(2°·t) while waiting (12-thwomp.md; the Side Thwomp the same
                // in 0x7100768800, its attack box still on its own side)
                let target = 0.3 * self.dir as f32;
                self.vx += (target - self.vx).clamp(-0.003, 0.003);
                self.vy = 0.15 * (2.0 * self.age as f32).to_radians().dsin();
                self.collide(level);
                if self.wall {
                    self.turn();
                }
                if triggered(self) {
                    self.phase = 1;
                    self.vx = 0.0;
                    self.vy = 0.0;
                    self.charge_from(side, right);
                }
            }
            0 => {
                self.vx = 0.0;
                self.vy = 0.0;
                if triggered(self) {
                    self.phase = 1;
                    self.charge_from(side, right);
                }
            }
            1 => {
                if side {
                    // a parachute it carries opens in the charge (actors/chute.rs)
                    let (acc, top) = self.side_thwomp_chute();
                    let t = top * self.dir as f32;
                    self.vx = if self.vx < t {
                        (self.vx + acc).min(t)
                    } else {
                        (self.vx - acc).max(t)
                    };
                    self.vy = 0.0;
                } else {
                    // a parachute it carries opens as it falls (actors/chute.rs)
                    let (g, cap) = self.thwomp_chute();
                    self.vy = (self.vy + g).max(cap);
                }
                let before = (self.x, self.y);
                self.collide(level);
                let stuck = (self.x - before.0).abs() < 1e-5 && (self.y - before.1).abs() < 1e-5;
                if (!side && self.on_ground) || (side && self.wall) || stuck {
                    self.hops = 0; // Fall's exit folds the parachute
                    self.phase = 2;
                    self.timer = 64;
                    self.vx = 0.0;
                    self.vy = 0.0;
                }
            }
            2 => {
                self.timer -= 1;
                if self.timer <= 0 {
                    self.phase = 3;
                }
            }
            _ => {
                if side {
                    // back toward the start, easing to 0.875 px/f by 0.2, snapping onto it
                    let d = self.home_x - self.x;
                    let target = 0.875 * d.signum();
                    self.vx += (target - self.vx).clamp(-0.2, 0.2);
                    if d.abs() <= self.vx.abs() || d.abs() < 0.01 {
                        self.x = self.home_x;
                        self.vx = 0.0;
                        self.phase = 0;
                    } else {
                        self.x += self.vx;
                    }
                } else {
                    self.vy = (self.vy + 0.2).min(0.875);
                    let target = self.home_y;
                    self.y = (self.y + self.vy).min(target);
                    if self.y >= target {
                        self.y = target;
                        self.vy = 0.0;
                        self.phase = 0;
                    }
                }
            }
        }
    }

    /// Turn `heading` (degrees, 0 = right, 90 = up) toward the player by at most `rate`.
    fn home_toward(&mut self, rate: f32) {
        let (tx, ty) = self.target;
        let want = (ty + 8.0 - (self.y + 8.0)).datan2(tx - self.x).to_degrees();
        let d = (want - self.heading + 540.0).rem_euclid(360.0) - 180.0;
        self.heading += d.clamp(-rate, rate);
    }

    fn rand(&mut self, n: u32) -> u32 {
        let mut x = self.rng.max(1);
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        x % n
    }

    pub fn half_w(&self) -> f32 {
        match self.kind {
            Kind::Fireball | Kind::EnemyFire | Kind::Superball => 4.0,
            Kind::Bowser { .. } | Kind::BoomBoom { .. } if self.big => 32.0,
            Kind::Bowser { .. } | Kind::BoomBoom { .. } => 16.0,
            Kind::BowserFire => 12.0,
            Kind::Shuriken => 4.0,
            Kind::BigCoin { .. } => 15.0,
            Kind::BanzaiBill { .. } => 32.0,
            Kind::Thwomp { .. } => {
                if self.big {
                    32.0
                } else {
                    16.0
                }
            }
            Kind::Lift { width, .. } => width as f32 * 8.0,
            Kind::ClownCar { .. } => 14.0,
            Kind::HammerBro if self.alt => 16.0, // Sledge Bro
            Kind::KoopaCar => 14.0,
            Kind::BabyBlooper { .. } => 4.0,
            Kind::Trampoline { width, .. } => width as f32 * 8.0,
            Kind::TrackBlock { .. } => 16.0,
            Kind::TrackMount { .. } => 8.0,
            k if k.is_w3() => self.w3_size().0,
            k if k.is_v3() => self.v3_size().0,
            k if k.is_pu() => self.pu_size().0,
            _ if self.big => 16.0,
            // the Weird Mushroom's contact box is 12 px wide, the Super Mushroom's 14 (SMM1
            // 0x023c3ae4: half-width 6 for 7) [obs]
            Kind::Mushroom if self.smm1_item == 1 => 7.0,
            _ => 8.0,
        }
    }

    pub fn height(&self) -> f32 {
        match self.kind {
            Kind::Fireball | Kind::EnemyFire | Kind::Superball => 8.0,
            Kind::Bowser { .. } | Kind::BoomBoom { .. } if self.big => 64.0,
            Kind::Bowser { .. } | Kind::BoomBoom { .. } => 32.0,
            Kind::BowserFire | Kind::Shuriken => 8.0,
            Kind::BigCoin { .. } => 30.0,
            Kind::BanzaiBill { .. } => 64.0,
            Kind::Thwomp { .. } => {
                if self.big {
                    64.0
                } else {
                    32.0
                }
            }
            Kind::Piranha { .. } => PIRANHA_H * if self.big { 2.0 } else { 1.0 },
            Kind::Koopa { .. } if !self.big => 16.0, // body box 16×16 (art is 16×32)
            Kind::HammerBro if self.alt => 32.0,
            Kind::BabyBlooper { .. } => 8.0,
            Kind::TrackMount { h, .. } => h as f32 * TILE,
            k if k.is_w3() => self.w3_size().1,
            k if k.is_v3() => self.v3_size().1,
            k if k.is_pu() => self.pu_size().1,
            _ if self.big => 32.0,
            _ => 16.0,
        }
    }

    /// (x0, y0, x1, y1) hit box. A Piranha's body extends from its origin along its pipe's
    /// outward direction (down for a down-facing pipe, sideways for side pipes).
    pub fn bbox(&self) -> (f32, f32, f32, f32) {
        let hw = self.half_w();
        let h = self.height();
        match (self.kind, self.pipe) {
            (Kind::Piranha { .. }, Some((PipeDir::Down, _, _))) => {
                (self.x - hw, self.y - h, self.x + hw, self.y)
            }
            (Kind::Piranha { .. }, Some((PipeDir::Right, _, _))) => {
                (self.x, self.y - hw, self.x + h, self.y + hw)
            }
            (Kind::Piranha { .. }, Some((PipeDir::Left, _, _))) => {
                (self.x - h, self.y - hw, self.x, self.y + hw)
            }
            (Kind::BigCoin { .. }, _) => {
                (self.x - 15.0, self.y + 1.0, self.x + 15.0, self.y + 31.0)
            }
            _ => (self.x - hw, self.y, self.x + hw, self.y + h),
        }
    }

    pub fn alive(&self) -> bool {
        !matches!(self.mode, Mode::Stomped | Mode::DieFall | Mode::Dead)
    }

    /// Can touch the player / other actors.
    pub fn solid_contact(&self) -> bool {
        (matches!(self.mode, Mode::Normal | Mode::ShellWait | Mode::ShellSlide)
            || self.shot_clear()
            || self.block_clear())
            && !(matches!(self.kind, Kind::LavaBubble | Kind::MontyMole) && self.phase == 0)
            && !self.boss_hidden()
            && !(matches!(self.kind, Kind::Piranha { .. }) && self.piranha_hidden())
    }

    /// A pipe Piranha is harmless while less than 30 % of the way out.
    fn piranha_hidden(&self) -> bool {
        let Some((dir, px, py)) = self.pipe else {
            return false;
        };
        let travel = self.height() + if self.big { 4.0 } else { 12.0 };
        let out = match dir {
            PipeDir::Up => self.y - (py - travel),
            PipeDir::Down => (py + travel) - self.y,
            PipeDir::Right => self.x - (px - travel),
            PipeDir::Left => (px + travel) - self.x,
        };
        out <= 0.3 * travel
    }

    /// Knock off (DieFall) away from `from_x`.
    pub fn die_fall(&mut self, from_x: f32) {
        self.die_by(from_x, DieCause::Other);
    }

    /// Knock off (DieFall) away from `from_x`, by `cause`.
    pub fn die_by(&mut self, from_x: f32, cause: DieCause) {
        let dir = if self.x < from_x { -1.0 } else { 1.0 };
        self.mode = Mode::DieFall;
        // the knock-off speeds are set when the hit is taken, on the next frame's update: on the
        // hit frame the actor still shows its old speeds [obs: Ryujinx trace M1_hit, Goombas hit
        // by a shell keep vx −0.5 / vy 0 that frame, then move (1.5, 3.9 − 0.24375)]
        self.die_pending = Some((DIE_SPEED * dir, DIE_VY));
        self.die_cause = cause;
    }

    /// Stomp reaction. Returns true if the actor took it (player bounces).
    pub fn stomp(&mut self) -> bool {
        if self.wings {
            // a stomp removes the wings (0-goomba.md, 1-koopa.md)
            self.wings = false;
            self.vy = self.vy.min(0.0);
            return true;
        }
        match self.kind {
            Kind::Buzzy => {
                self.kind = Kind::Shell { red: false };
                self.buzzy = true;
                self.enter_shell_wait();
                true
            }
            Kind::DryBones if self.phase == 0 => {
                // collapse into a pile of bones for 224 frames
                self.phase = 1;
                self.timer = 224;
                self.vx = 0.0;
                true
            }
            Kind::BobOmb if self.phase == 3 => {
                // a walking lit one stops, its fuse still running: Down's enter lights the
                // fuse only when it is unlit [obs v3.0.3 0x7101084420]
                self.phase = 1;
                self.vx = 0.0;
                true
            }
            Kind::BobOmb if self.phase == 0 => {
                // fuse lit: it stops (the fuse runs 240 frames)
                self.phase = 1;
                self.timer = 240;
                self.vx = 0.0;
                true
            }
            Kind::HammerBro => {
                self.die_fall(self.x - self.dir as f32);
                true
            }
            Kind::Lakitu | Kind::Magikoopa | Kind::RockyWrench => {
                self.die_fall(self.x - 1.0);
                true
            }
            Kind::Wiggler => {
                // not hurt: angry after a 60-frame pause
                if self.phase == 0 {
                    self.phase = 1;
                    self.timer = 60;
                }
                true
            }
            Kind::BulletBill | Kind::BanzaiBill { .. } | Kind::Cannonball => {
                // DieStamp for a Bullet Bill: it drops away
                self.mode = Mode::DieFall;
                self.vx = 0.0;
                self.vy = 0.0;
                true
            }
            Kind::FishBone => {
                // it crumbles (DieVanish)
                self.mode = Mode::Dead;
                true
            }
            Kind::ShoeGoomba { .. } => {
                // the Goomba falls out of its shoe (the world drops the empty shoe)
                self.die_fall(self.x - self.dir as f32);
                true
            }
            Kind::Goomba | Kind::Goombrat => {
                self.mode = Mode::Stomped;
                self.timer = STOMP_FRAMES;
                self.vx = 0.0;
                self.vy = 0.0;
                true
            }
            Kind::Koopa { red } => {
                self.kind = Kind::Shell { red };
                self.enter_shell_wait();
                true
            }
            // a stomp on a sliding shell stops it dead; on a still shell, nothing (the touch kicks
            // it instead, handled by the caller)
            Kind::Shell { .. } if self.mode == Mode::ShellSlide => {
                self.enter_shell_wait();
                true
            }
            _ => false,
        }
    }

    fn enter_shell_wait(&mut self) {
        self.bounce_pending = false;
        self.mode = Mode::ShellWait;
        self.timer = SHELL_WAKE;
        self.vx = 0.0;
        self.vy = 0.0;
    }

    /// Kick a still shell (1-koopa.md): 2.5, plus 0.5·|kicker vx| when the kicker moves in the
    /// kick direction.
    pub fn kick(&mut self, dir: i32, kicker_vx: f32) {
        let mut speed = 2.5;
        if kicker_vx != 0.0 && kicker_vx.signum() as i32 == dir {
            speed += 0.5 * kicker_vx.abs();
        }
        self.mode = Mode::ShellSlide;
        self.bounce_pending = false;
        self.dir = dir;
        self.vx = speed * dir as f32;
        self.kick_lock = 16;
        self.chain = 0;
    }

    /// A player's touch kicks a still shell (kick()). From above the slide starts with the
    /// shell's next update (`kick_speed`) [obs: Ryujinx trace M1_koopa koopa_kick]; from the
    /// side the shell already slides this frame [obs: Ryujinx trace M1_shell, x + 3.25 on the
    /// kick frame].
    pub fn touch_kick(&mut self, dir: i32, kicker_vx: f32, from_top: bool, level: &Level) {
        self.kick(dir, kicker_vx);
        if from_top {
            self.kick_speed = std::mem::take(&mut self.vx);
        } else {
            self.vy = (self.vy + ENEMY_GRAVITY).max(MAX_FALL);
            self.collide(level);
            if self.wall {
                self.bounce_pending = true;
            }
        }
    }

    pub fn turn(&mut self) {
        self.dir = -self.dir;
        self.vx = -self.vx;
        self.turn_lock = TURN_LOCK;
        self.turn_stop = false;
    }

    /// One frame of behaviour and movement. `player_x` is used for facing rules.
    pub fn step(&mut self, level: &Level, player_x: f32) {
        self.target.0 = player_x;
        let _ = self.step_spawning(level);
    }

    /// One frame; may return a new actor to add (a Fire Piranha's fireball). Uses `target`.
    pub fn step_spawning(&mut self, level: &Level) -> Option<Actor> {
        let before = (self.x, self.y);
        // a claw's track moves its pivot before the rope's step [obs v3.0.3 0x71013e7b10]
        let pivot_first = self.kind == Kind::Claw;
        if pivot_first {
            self.ride_track(level);
        }
        let out = if self.derailed && self.falls_when_derailed() {
            // gravity as a falling lift's [inf: the rider's own FallDown state isn't decoded]
            self.vy = (self.vy - 0.0625).max(-4.0);
            self.x += self.vx;
            self.y += self.vy;
            None
        } else {
            self.step_inner(level)
        };
        if !pivot_first {
            self.ride_track(level);
        }
        self.moved = (self.x - before.0, self.y - before.1);
        out
    }

    /// Objects with no gravity of their own, which a derail must make fall (platforms and
    /// mounted objects); everything else falls under its own rules once off the track.
    fn falls_when_derailed(&self) -> bool {
        self.kind.is_platform() || matches!(self.kind, Kind::TrackMount { .. })
    }

    /// On a track the rail sets the position (the actor's own motion is replaced); knocked off
    /// (stomped, killed, kicked), it falls under its own rules.
    fn ride_track(&mut self, level: &Level) {
        let Some(mut r) = self.rail else { return };
        if !matches!(self.mode, Mode::Normal) {
            self.rail = None;
            return;
        }
        let before = r.point(&level.tracks);
        let (px, py) = r.step(&level.tracks);
        if r.off {
            // past an open end: it keeps its last motion and falls (FallDown)
            self.rail = None;
            self.derailed = true;
            self.vx = px - before.0;
            self.vy = py - before.1;
            self.x = px + self.rail_off.0;
            self.y = py + self.rail_off.1;
            return;
        }
        self.rail = Some(r);
        if self.kind == Kind::Claw {
            // the claw hangs from a pivot that rides the track
            self.home_x += px - before.0;
            self.home_y += py - before.1;
            return;
        }
        self.x = px + self.rail_off.0;
        self.y = py + self.rail_off.1;
        self.vx = 0.0;
        self.vy = 0.0;
    }

    /// The Parachute context's Fall (v3.0.3 0x7101058410 / 0x7101058540, v1.0.0 0x71005f36e0 …
    /// 0x71005f4380) [obs]: gravity −0.01875 to −0.4; this frame's speed is the change of
    /// 24·sin φ; touching a wall in the swing's direction holds it there at twice the swing
    /// rate until the swing turns back; landing drops the parachute.
    fn step_parachute(&mut self, level: &Level, phase: f32, rate: f32, held: bool) {
        self.vy = (self.vy - 0.01875).max(-0.4);
        let next = phase + rate;
        let swing = CHUTE_SWING * (next.to_radians().dsin() - phase.to_radians().dsin());
        // the swing's direction: the sign of d(sin φ)
        let going = |p: f32| (rate * p.to_radians().dcos()).signum();
        let (mut rate, mut held) = (rate, held);
        if held && going(next) != going(phase) {
            held = false;
            rate /= 2.0;
        }
        self.vx = if held { 0.0 } else { swing };
        self.collide(level);
        if !held && self.wall && self.vx.signum() == going(next) {
            held = true;
            rate *= 2.0;
        }
        // a preset's own end (actors/chute.rs), else the Default landing
        self.mode = if self.chute_fall_ends(level.theme) || self.on_ground {
            self.vx = 0.0;
            Mode::Normal
        } else {
            Mode::Parachute {
                phase: next,
                rate,
                held,
            }
        };
    }

    fn step_inner(&mut self, level: &Level) -> Option<Actor> {
        let player_x = self.target.0;
        let mut spawned = None;
        self.age += 1;
        // an item the player touched is taken in its own next update (20-super-mushroom.md: the
        // touching player is latched and handled in the item update): it moves once more, with
        // no landing, and stops [obs: Ryujinx trace M1_blocks blk_mush, the mushroom's last
        // frame at y 16 − 0.1875]
        match self.taken {
            0 => {}
            1 => {
                if let Mode::Emerge { .. } = self.mode {
                    self.y += self.vy;
                } else if matches!(self.kind, Kind::Mushroom | Kind::OneUp) {
                    // (the fall step moves it, its vy field stays) [obs]
                    self.x += self.vx;
                    self.y += (self.vy + ENEMY_GRAVITY).max(MAX_FALL);
                }
                // (a flower stays put [obs: Ryujinx trace W_3051046])
                self.taken = 2;
                return None;
            }
            2 => return None,
            _ => {
                self.mode = Mode::Dead;
                return None;
            }
        }
        self.wall = false;
        if self.kick_lock > 0 {
            self.kick_lock -= 1;
        }
        if self.turn_lock > 0 {
            self.turn_lock -= 1;
        }
        match self.mode {
            Mode::Dead => {}
            Mode::Emerge { .. } if self.kind == Kind::StyleItem && level.style == "WU" => {
                // straight into its Move (actors/propeller.rs)
                self.mode = Mode::Normal;
                self.propeller_enter(Some(self.vy < 0.0));
                self.propeller_move();
            }
            Mode::Emerge { .. }
                if self.kind == Kind::StyleItem && !matches!(level.style.as_str(), "M1" | "3W") =>
            {
                self.leaf_pop(self.vy < 0.0);
            }
            Mode::Emerge { until } => {
                self.y += self.vy;
                if (self.vy > 0.0 && self.y >= until) || (self.vy < 0.0 && self.y <= until) {
                    let up = self.vy > 0.0;
                    self.y = until;
                    self.mode = Mode::Normal;
                    self.vy = 0.0;
                    // out on the block top, a mushroom stands that frame and walks from the next
                    // [obs: Ryujinx traces M1_shellq, M1_blocks blk_mush: vx 0 on the frame it
                    // reaches the top, +1 the frame after]
                    if up && matches!(self.kind, Kind::Mushroom | Kind::OneUp) {
                        self.on_ground = true;
                    }
                    self.vx = match self.kind {
                        Kind::Mushroom | Kind::OneUp => 0.0,
                        Kind::Star => ITEM_SPEED * self.dir as f32,
                        Kind::PowerItem { what } if powerups::walks(what, 0) => {
                            ITEM_SPEED * self.dir as f32
                        }
                        _ => 0.0,
                    };
                    if self.kind == Kind::Star {
                        self.vy = FIREBALL_BOUNCE;
                    }
                }
            }
            Mode::DokanOut { .. } => self.step_dokan(level),
            Mode::Parachute { phase, rate, held } => self.step_parachute(level, phase, rate, held),
            Mode::BlockOut { .. } => self.step_block_out(level),
            Mode::Shot { .. } => self.step_shot(level),
            Mode::Stomped => {
                self.timer -= 1;
                if self.timer <= 0 {
                    self.mode = Mode::Dead;
                }
            }
            Mode::DieFall => {
                // gone once it has fallen 40 px below the view's bottom, at its next update [obs:
                // Ryujinx trace M1_hit, two Goombas knocked off by a shell last move to y −43.9
                // and are removed the frame after; at −39.9 they go on]
                // 3D World: gone after 24 frames of the fall, wherever it is [obs: Ryujinx trace
                // 3W_hit, two Kuribo3W knocked off by a shell]
                if self.y < self.camera.1 - 40.0 || (level.style == "3W" && self.die_frames >= 24) {
                    self.mode = Mode::Dead;
                    return None;
                }
                self.die_frames += 1;
                if let Some((vx, vy)) = self.die_pending.take() {
                    (self.vx, self.vy) = (vx, vy);
                }
                self.vy = (self.vy + DIE_GRAVITY).max(MAX_FALL);
                self.x += self.vx;
                self.y += self.vy;
            }
            Mode::Normal => spawned = self.step_normal(level, player_x),
            Mode::Thrown => self.step_thrown(level),
            Mode::ClearPipe => {}
            Mode::ShellWait => {
                if self.on_ground && self.vy <= 0.0 {
                    self.vx = 0.0;
                }
                self.vy = (self.vy + ENEMY_GRAVITY).max(MAX_FALL);
                self.collide(level);
                self.timer -= 1;
                if self.shell_of != 0 {
                    self.timer = SHELL_WAKE; // nobody inside
                }
                if self.timer <= 0
                    && let Kind::Shell { red } = self.kind
                {
                    // the Koopa (or Buzzy) walks out, facing the player
                    self.kind = if self.buzzy {
                        Kind::Buzzy
                    } else if self.spiny_shell {
                        // a Spiny that dropped off the ceiling in its shell (actors/ceiling.rs)
                        Kind::Spiny
                    } else {
                        Kind::Koopa { red }
                    };
                    self.buzzy = false;
                    self.spiny_shell = false;
                    self.mode = Mode::Normal;
                    self.dir = if player_x <= self.x { -1 } else { 1 };
                }
            }
            Mode::ShellSlide => {
                if self.kick_speed != 0.0 {
                    self.vx = std::mem::take(&mut self.kick_speed);
                }
                if std::mem::take(&mut self.bounce_pending) {
                    self.dir = -self.dir;
                    self.vx = -self.vx;
                }
                self.vy = (self.vy + ENEMY_GRAVITY).max(MAX_FALL);
                self.collide(level);
                if self.wall {
                    // reverse next frame, keeping the speed (the caller hits the block in the way)
                    self.bounce_pending = true;
                }
                if self.vx.abs() < 0.001 {
                    self.enter_shell_wait();
                }
            }
        }
        if self.mode == Mode::Normal
            && let Kind::Piranha { fire } = self.kind
        {
            spawned = self.step_piranha(fire);
        }
        // (a Charvaargh dives far below the lava between leaps: actors/w3.rs)
        if self.y < -4.0 * TILE && self.mode != Mode::DieFall && self.kind != Kind::Charvaargh {
            self.mode = Mode::Dead; // fell into a pit
        }
        spawned
    }

    /// Piranha Plant (2-piranha-plant.md). Free-standing: bites in place (and a Fire Piranha
    /// spits every 48–95 + 40 frames). In a pipe: hidden 72 → rise 1 px/f → out 96 → sink.
    fn step_piranha(&mut self, fire: bool) -> Option<Actor> {
        let (px, py) = self.target;
        if fire {
            self.dir = if px <= self.x { -1 } else { 1 };
        }
        let Some((dir, mx, my)) = self.pipe else {
            // free-standing: phase 0 = waiting to spit (timer), 1 = spitting
            if !fire {
                return None;
            }
            return self.fire_cycle(px, py);
        };
        let travel = self.height() + if self.big { 4.0 } else { 12.0 };
        let u = match dir {
            PipeDir::Up => (0.0, 1.0),
            PipeDir::Down => (0.0, -1.0),
            PipeDir::Right => (1.0, 0.0),
            PipeDir::Left => (-1.0, 0.0),
        };
        let hidden = (mx - u.0 * travel, my - u.1 * travel);
        let out = (mx, my);
        let towards = |a: &mut Actor, t: (f32, f32)| {
            let step = |v: f32, t: f32| {
                if (t - v).abs() <= 1.0 {
                    t
                } else {
                    v + (t - v).signum()
                }
            };
            a.x = step(a.x, t.0);
            a.y = step(a.y, t.1);
            a.x == t.0 && a.y == t.1
        };
        let mut shot = None;
        match self.phase {
            // hidden; a player at the mouth keeps it in (timer ≥ 60)
            0 => {
                let at_mouth = match dir {
                    PipeDir::Up => (px - mx).abs() <= 28.0 && py <= my + 16.0 && py >= my - 32.0,
                    PipeDir::Down => (px - mx).abs() <= 19.0 && py <= my && py >= my - 35.0,
                    PipeDir::Right => (px - (mx + 8.0)).abs() <= 8.0 && (py - my).abs() <= 17.0,
                    PipeDir::Left => (px - (mx - 8.0)).abs() <= 8.0 && (py - my).abs() <= 17.0,
                };
                if at_mouth {
                    self.timer = self.timer.max(60);
                }
                self.timer -= 1;
                if self.timer <= 0 {
                    self.phase = 1;
                    self.timer = 8;
                }
            }
            // OutMove: 8-frame delay, then 1 px/frame
            1 => {
                if self.timer > 0 {
                    self.timer -= 1;
                } else if towards(self, out) {
                    self.phase = 2;
                    self.timer = 96;
                    self.pause = 48 + self.rand(48) as i32; // fire delay
                }
            }
            // DokanWait: 96 frames (a Fire Piranha waits until it has fired)
            2 => {
                self.timer -= 1;
                let mut fired = true;
                if fire {
                    fired = self.phase_fired();
                    if !fired {
                        shot = self.fire_cycle(px, py);
                    }
                }
                if self.timer <= 0 && fired {
                    self.phase = 3;
                }
            }
            // DokanIn
            _ => {
                if towards(self, hidden) {
                    self.phase = 0;
                    self.timer = 72;
                    self.hops = 0;
                }
            }
        }
        shot
    }

    fn phase_fired(&self) -> bool {
        self.hops >= 2
    }

    /// Fire Piranha spit: wait `pause` frames, then 20 frames of animation, spit, 20 frames.
    /// `hops` = 0 waiting, 1 spitting, 2 done (pipe plants fire once per cycle).
    fn fire_cycle(&mut self, px: f32, py: f32) -> Option<Actor> {
        match self.hops {
            0 => {
                if self.pause > 0 {
                    self.pause -= 1;
                } else {
                    self.hops = 1;
                    self.chain = 0;
                }
                None
            }
            1 => {
                self.chain += 1;
                if self.chain == 20 {
                    // angle ±22° / ±45° toward the player
                    let (mx, my) = match self.pipe {
                        Some((PipeDir::Down, ..)) => (self.x, self.y - self.height() + 8.0),
                        _ => (self.x, self.y + self.height() - 8.0),
                    };
                    let want = (py + 8.0 - my).datan2(px - mx);
                    let face = if px <= self.x {
                        std::f32::consts::PI
                    } else {
                        0.0
                    };
                    let best = [22.0f32, -22.0, 45.0, -45.0]
                        .iter()
                        .map(|d| {
                            let a = if face == 0.0 {
                                d.to_radians()
                            } else {
                                std::f32::consts::PI - d.to_radians()
                            };
                            let diff = (a - want).dsin().datan2((a - want).dcos()).abs();
                            (diff, a)
                        })
                        .min_by(|a, b| a.0.total_cmp(&b.0))
                        .map(|(_, a)| a)
                        .unwrap_or(0.0);
                    let mut f = Actor::new(Kind::EnemyFire, mx, my - 4.0);
                    f.vx = best.dcos();
                    f.vy = best.dsin();
                    f.timer = 600;
                    return Some(f);
                }
                if self.chain >= 40 {
                    if self.pipe.is_some() {
                        self.hops = 2;
                    } else {
                        self.hops = 0;
                        self.pause = 48 + self.rand(48) as i32;
                    }
                }
                None
            }
            _ => None,
        }
    }

    fn step_normal(&mut self, level: &Level, player_x: f32) -> Option<Actor> {
        if self.wings {
            match self.kind {
                Kind::Goomba
                | Kind::Goombrat
                | Kind::Koopa { .. }
                | Kind::Thwomp { .. }
                | Kind::ShoeGoomba { .. } => {} // their own wing behaviour
                Kind::Bowser { .. } | Kind::BoomBoom { .. } | Kind::BowserJr { .. } => {}
                k if k.is_w3() => {} // their own wing behaviour (actors/w3.rs)
                k if k.is_v3() => {} // their own wing behaviour (actors/v3.rs)
                Kind::Spiny => {
                    self.step_pata_togezo(level);
                    return None;
                }
                // the shared fly helper (actors/fly.rs); a track carries a winged part itself
                k if k.flies_winged() && !self.thrown && self.rail.is_none() => {
                    let slow = self.wet();
                    self.step_fly(level, slow);
                    return None;
                }
                Kind::Piranha { .. } | Kind::Muncher if self.pipe.is_none() => {
                    self.step_winged_plant(level);
                    return None;
                }
                // their Wing action is their Basic one (the same action class: Teresa,
                // Pukupuku, FishBone, Hanachan, Kameck, Bros1st / BrosU / FireBros / MegaBros,
                // Choropoo, Gesso, Wanwan) [obs v1.0.0 actionmap]: they act as unwinged, but
                // for the wing-only states inside it: the Boo's PataCharge (actors/boo.rs),
                // the Bros' WingWalk (actors/bros.rs)
                Kind::Boo
                | Kind::CheepCheep { .. }
                | Kind::FishBone
                | Kind::Wiggler
                | Kind::Magikoopa
                | Kind::HammerBro
                | Kind::MontyMole
                | Kind::Blooper
                | Kind::ChainChomp { .. } => {}
                // a Dry Bones' Wing is the red Paratroopa's (Karon Wing vtable 0x7102179e68 =
                // NokonokoRed's; its Fly enter 0x7100697e40 is the same and its exe 0x7100697ec0
                // runs NokonokoRed's Fly exe 0x710072b370) [obs v1.0.0]; the same in v3.0.3 [obs
                // v3.0.3: factory 0x7101135770 builds NokonokoRed's Wing (0x71011fe070) and only
                // swaps the vtable to 0x71028df578, whose Fly enter 0x710113adf0 is NokonokoRed's
                // 0x71011fedf0 and whose exe 0x710113ae70 runs NokonokoRed's 0x71011fee70, then
                // its bones' helper]: step_wings
                Kind::DryBones => {}
                // the Para-Beetle: the Spiny's hover without its wall turn (actors/fly.rs)
                Kind::Buzzy | Kind::SpikeTop { .. } if !self.ceiling => {
                    self.step_para_beetle(level);
                    return None;
                }
                // Bubble Wing: a straight flight bouncing off terrain (actors/wing.rs)
                Kind::LavaBubble if self.pipe.is_none() && self.rail.is_none() => {
                    self.step_winged_bubble(level);
                    return None;
                }
                // its own Fly / Turn (actors/wing.rs)
                Kind::BobOmb if self.phase == 0 => {
                    self.step_winged_bobomb(level);
                    return None;
                }
                k if k.is_enemy() => {
                    self.step_generic_wings(level);
                    return None;
                }
                _ => {}
            }
        }
        match self.kind {
            Kind::DryBones if self.phase == 1 => {
                // collapsed: reassembles after 224 frames
                self.timer -= 1;
                self.vy = (self.vy + ENEMY_GRAVITY).max(MAX_FALL);
                self.collide(level);
                if self.timer <= 0 {
                    self.phase = 0;
                    self.dir = if player_x <= self.x { -1 } else { 1 };
                }
            }
            Kind::BobOmb if self.phase > 0 => self.step_lit_bobomb(level),
            Kind::HammerBro => return self.step_hammer_bro(level),
            Kind::Hammer => {
                self.vy = (self.vy - 0.125).max(-6.0);
                self.x += self.vx;
                self.y += self.vy;
            }
            Kind::Boo => self.step_boo(level.style == "3W"),
            Kind::RingBoo { slot } => {
                let k = if self.big { 0.5 } else { 1.0 };
                let r_max = if self.big { 52.0 * 1.4 } else { 52.0 };
                self.speed += (0.15 * (r_max - self.speed)).clamp(0.1, 0.6 * k);
                self.speed = self.speed.min(r_max);
                let v = (self.age as f32 * 0.04).min(1.0);
                self.heading -= v * k;
                let a = (self.heading - slot as f32 * 37.06).to_radians();
                self.x = self.home_x + self.speed * a.dcos();
                self.y = self.home_y + self.speed * a.dsin() - 8.0;
                self.dir = if self.target.0 < self.x { -1 } else { 1 };
            }
            Kind::Bowser { .. } => return self.step_bowser(level),
            Kind::BoomBoom { .. } => return self.step_boom_boom(level),
            Kind::BowserJr { .. } => return self.step_bowser_jr(level),
            Kind::AntTrooper { .. }
            | Kind::Stingby
            | Kind::Skipsqueak { .. }
            | Kind::HopChops
            | Kind::Porcupuffer
            | Kind::Bully
            | Kind::Charvaargh
            | Kind::PiranhaCreeper { .. } => return self.step_w3(level),
            Kind::Spike
            | Kind::SpikeBall { .. }
            | Kind::Pokey { .. }
            | Kind::Mechakoopa { .. }
            | Kind::MechaMissile
            | Kind::MechaLaser
            | Kind::OnOffTrampoline { .. }
            | Kind::Koopaling { .. }
            | Kind::KoopalingShot { .. } => return self.step_v3(level),
            Kind::PowerItem { .. }
            | Kind::PlayerArrow
            | Kind::PlayerBoomerang
            | Kind::LinkBomb
            | Kind::CannonShot
            | Kind::RedPowRing => self.step_pu(level),
            Kind::BowserFire | Kind::Shuriken => self.step_boss_shot(level),
            Kind::ChainChomp { chained } => self.step_chomp(level, chained),
            Kind::LavaBubble => self.step_lava_bubble(),
            Kind::FishBone => self.step_fish_bone(level),
            Kind::YoshiEgg { red } => {
                if self.phase == 0 {
                    // Wait: the item base's movement
                    self.vy = (self.vy - 0.34).max(-4.0);
                    self.vx *= 0.9;
                    self.collide(level);
                    // Land the first frame it stands on the ground with its box (22 px each
                    // way round its position) touching the camera's view, edges included
                    // (Wait exe 0x7100852bb0 → vfunc 0x4e8 0x71008cc760: the camera views
                    // list; sticky through 0x4f8) [obs v1.0.0]; Land's enter plays "break"
                    // and starts the hatch helper's 40-frame count (0x7100859b00 = v3.0.3
                    // 0x710137acd0)
                    let (cx, cy) = self.camera;
                    let seen = self.x + EGG_BOX >= cx
                        && self.x - EGG_BOX <= cx + crate::world::VIEW_W
                        && self.y + EGG_BOX >= cy
                        && self.y - EGG_BOX <= cy + crate::world::VIEW_H;
                    if self.on_ground && seen {
                        self.phase = 1;
                        self.timer = EGG_HATCH;
                        self.vx = 0.0;
                    }
                }
                if self.phase == 1 {
                    // the helper hatches it when its count runs out (0x71013f7990 [obs
                    // v3.0.3]) [inf: it also waits for the break animation's end, taken as
                    // no longer; the helper counting from Land's own frame]
                    self.timer -= 1;
                    if self.timer <= 0 {
                        self.kind = Kind::Yoshi { red };
                        self.phase = 0;
                        self.timer = 0;
                        self.vx = 0.0;
                    }
                }
            }
            Kind::Yoshi { .. } => self.step_yoshi(level),
            Kind::StyleItem => self.step_style_item(level),
            Kind::FloatCoin => {
                if self.thrown {
                    // a Lakitu's coin: its arc (gravity −0.14, falling at most 1.5) for 50
                    // frames (cState_Thrown exe 0x7100847f90), then the Wing context with the
                    // bob's angle at 180° (+0xdb4 = 0x80000000): the shared flight, 0.625 px/f
                    // its way, the bob starting down (actors/fly.rs) [obs v1.0.0]
                    self.vy = (self.vy - 0.14).max(shot::THROWN_FALL);
                    self.x += self.vx;
                    self.y += self.vy;
                    self.timer -= 1;
                    if self.timer <= 0 {
                        (self.thrown, self.vx, self.vy) = (false, 0.0, 0.0);
                        self.fly_from_top();
                    }
                } else if self.fly.on {
                    let slow = self.wet();
                    self.step_fly(level, slow);
                }
            }
            Kind::ClownCar { .. } => return self.step_clown_car(level),
            Kind::KoopaCar => {
                if self.phase == 0 {
                    // empty: brakes 0.05 per frame (0x71006cb050)
                    self.vx -= self.vx.clamp(-0.05, 0.05);
                    self.vy = (self.vy + ENEMY_GRAVITY).max(MAX_FALL);
                    self.collide(level);
                }
            }
            Kind::ShoeGoomba { .. } => self.step_shoe_goomba(level),
            Kind::Shoe { .. } => {
                self.vx = 0.0;
                self.vy = (self.vy + ENEMY_GRAVITY).max(MAX_FALL);
                self.collide(level);
            }
            Kind::MontyMole => self.step_monty(level),
            Kind::AngrySun { moon } => self.step_sun(moon),
            Kind::SpikeTop { surface } => self.step_spike_top(level, surface),
            Kind::Goomba
            | Kind::Goombrat
            | Kind::Koopa { .. }
            | Kind::Spiny
            | Kind::Buzzy
            | Kind::BobOmb
            | Kind::DryBones => {
                if self.ceiling {
                    self.step_ceiling(level);
                    return None;
                }
                if self.wings
                    && matches!(
                        self.kind,
                        Kind::Goomba | Kind::Goombrat | Kind::Koopa { .. } | Kind::DryBones
                    )
                {
                    self.step_wings(level, player_x);
                    return None;
                }
                // 3D World's Goomba (Kuribo3W) walks at 0.3 (0-goomba.md, Basic slot 54 →
                // DAT_7101c8aeb0) [obs: Ryujinx trace 3W_hit, −0.3 a frame], and runs at a player
                // it spots (`kuribo3w`)
                let speed = if self.kind == Kind::Goomba && level.style == "3W" {
                    self.kuribo3w()
                } else {
                    WALK_SPEED
                };
                if self.alt && self.kind == Kind::BobOmb && self.age <= 1 {
                    // the alternate Bob-omb starts lit and keeps walking (15-bob-omb.md)
                    self.phase = 3;
                    self.timer = 240;
                }
                if self.on_ground {
                    self.vx = speed * self.dir as f32;
                } else {
                    self.vx = self.vx.clamp(-speed, speed);
                }
                // an enemy met in front (the touch flag of last frame's contacts): Walk turns,
                // this frame's move still the old way, then 8 frames of Turn, walking the new way
                // unless stopped: an enemy met in front during the turn stops the walker from the
                // next frame to the turn's end (the Basic action's Walk / Turn, 0x71005ecc70,
                // 0x71005ecd60, turn-done 0x71005e5040; the flag 0x51a bit 3 set by the enemy
                // contact 0x710072bca0 for a moving walker overlapping another enemy ≥ 0.1 px on
                // its front side) [obs: Ryujinx trace W_3051605 with +0x53c / +0x540 recorded, a
                // packed row of Goombas]
                // (a wall is read the same way: the frame after the move that met it)
                let touch = std::mem::take(&mut self.touch_front);
                let wall_prev = std::mem::take(&mut self.wall_prev);
                let mut touch_turn = false;
                if self.turn_lock > 0 {
                    if self.turn_stop {
                        self.vx = 0.0;
                    } else if touch {
                        self.turn_stop = true;
                    }
                } else if (touch || wall_prev) && self.on_ground {
                    touch_turn = true;
                }
                // the frame after landing from a fall onto a slope it stands (Fall hands over to
                // Walk) [obs: Ryujinx trace W_3051605, a Goomba landing on a slope, still a
                // frame, then on; W_3049200 frame 395, a Koopa landing on flat ground walks on]
                if self.land_wait > 0 {
                    self.land_wait -= 1;
                    self.vx = 0.0;
                }
                let was_on_ground = self.on_ground;
                self.vy = (self.vy + ENEMY_GRAVITY).max(MAX_FALL);
                self.collide(level);
                if !was_on_ground
                    && self.on_ground
                    && self.age > 1
                    && level
                        .slope_at(self.x)
                        .is_some_and(|(sy, _)| (sy - self.y).abs() < 0.5)
                {
                    self.land_wait = 1;
                }
                if touch_turn {
                    self.dir = -self.dir;
                    self.turn_lock = TURN_LOCK + 1;
                    self.turn_stop = false;
                } else if self.wall && self.on_ground && was_on_ground {
                    // (pressed through a turn too: the Walk after it turns at once [obs:
                    // W_3051605, a Goomba turned toward a wall pushing it to the turn's end])
                    self.wall_prev = true;
                } else if self.wall && self.turn_lock == 0 {
                    self.turn();
                }
                // a red Koopa turns at ledges: after the move, the floor is probed at its foot
                // sensor's end, 4 px ahead (the walkers' BG sensor records, ids 0–3 of the table
                // 0x71023cb8b0: feet ±4; big: the record's flags bit 0 clear, +4 a side) [obs
                // v3.0.3]; an end exactly on a cell edge still stands on the cell it is leaving
                // (with the edge open 10 of the 580 traces break) [obs: traces]; off the floor,
                // it faces back and walks back from the next frame (this frame's move stands)
                // [obs: Ryujinx trace M1_koopa, a red Koopa pacing a 1-wide pillar between x
                // 323.5 and 332.5, its facing flipping on those frames]
                if self.kind.ledge_turner()
                    && was_on_ground
                    && self.on_ground
                    && !self.wall
                    && self.turn_lock == 0
                {
                    let reach = if self.big { 8.0 } else { 4.0 };
                    let px = self.x + self.dir as f32 * reach;
                    // (the sensor's end on a cell edge still touches the cell it leaves)
                    let cx = if self.dir > 0 {
                        (px / TILE).ceil() - 1.0
                    } else {
                        (px / TILE).floor()
                    };
                    let below = level.cell_at(cx * TILE + TILE * 0.5, self.y - 1.0);
                    let slope = level.slope_at(px).is_some_and(|(s, _)| s >= self.y - 8.0);
                    if below == Cell::Empty && !slope {
                        self.dir = -self.dir;
                        self.turn_lock = TURN_LOCK;
                    }
                }
            }
            Kind::Mushroom | Kind::OneUp => {
                // a wall met is turned at a frame late, the next move the new way [obs: Ryujinx
                // trace X_mushstart: against a wall on frame 0 at +1, away at −1 from frame 1]
                if std::mem::take(&mut self.wall_prev) {
                    self.dir = -self.dir;
                }
                self.vy = (self.vy + ENEMY_GRAVITY).max(MAX_FALL);
                if self.on_ground {
                    self.vx = ITEM_SPEED * self.dir as f32;
                }
                self.collide(level);
                if self.wall {
                    self.wall_prev = true;
                }
            }
            Kind::FireFlower | Kind::SuperballFlower | Kind::SuperHammer => {
                self.vx = 0.0;
                self.vy = (self.vy + ENEMY_GRAVITY).max(MAX_FALL);
                self.collide(level);
            }
            Kind::Star => {
                self.vx = ITEM_SPEED * self.dir as f32;
                self.vy = (self.vy + ENEMY_GRAVITY).max(MAX_FALL);
                self.collide(level);
                if self.on_ground {
                    self.vy = FIREBALL_BOUNCE; // bounces 4.0 on every landing
                    self.on_ground = false;
                }
                if self.wall {
                    self.dir = -self.dir;
                }
            }
            Kind::Fireball if self.alt => {
                // a Fire Koopa Clown Car's PlayerShot (exe 0x7100ddf010, v3.0.3): straight at
                // its launch speed, put out by terrain, at most 600 frames [obs]; the charged
                // one (`phase` 1, ChargeShot 0x7100ddf610) has no life count and bursts on
                // terrain (`phase` 2: ChargeExplode, its tiles hit by the world,
                // actors/clown.rs `charge_burst_tiles`)
                if self.phase == 2 {
                    return None;
                }
                if self.phase != 1 {
                    self.timer -= 1;
                }
                self.x += self.vx;
                self.y += self.vy;
                if level.cell_at(self.x, self.y + 4.0).is_solid() {
                    if self.phase == 1 {
                        // the side it hit (+0x19ec: 1 wall, 2 floor, 3 ceiling) [inf: a wall
                        // when the cell it came from sideways is free]
                        let side = if !level.cell_at(self.x - self.vx, self.y + 4.0).is_solid() {
                            1
                        } else if self.vy > 0.0 {
                            3
                        } else {
                            2
                        };
                        // (held outside, as the game's terrain check leaves it)
                        self.x -= self.vx;
                        self.y -= self.vy;
                        self.phase = 2;
                        self.hops = side;
                    } else {
                        self.mode = Mode::Dead;
                    }
                } else if self.timer <= 0 {
                    self.mode = Mode::Dead;
                }
            }
            Kind::Fireball => {
                self.timer -= 1;
                self.vy = (self.vy + FIREBALL_GRAVITY).max(MAX_FALL);
                self.collide(level);
                if self.on_ground {
                    self.vy = FIREBALL_BOUNCE;
                    self.on_ground = false;
                }
                if self.wall || self.timer <= 0 {
                    self.mode = Mode::Dead; // snuffed against a wall
                }
            }
            Kind::Superball => {
                self.timer -= 1;
                let before = (self.vx, self.vy);
                let y0 = self.y;
                self.collide(level);
                if self.wall {
                    self.vx = -before.0;
                } else {
                    self.vx = before.0;
                }
                if self.on_ground {
                    self.vy = 3.0;
                    self.on_ground = false;
                } else if before.1 > 0.0 && self.y < y0 + before.1 - 0.01 {
                    self.vy = -3.0; // a ceiling
                } else {
                    self.vy = before.1;
                }
                if self.timer <= 0 {
                    self.mode = Mode::Dead;
                }
            }
            Kind::EnemyFire => {
                // `alt`: a Fire Koopa Clown Car's KoopaClownFire (EnemyShot exe 0x7100ddeb30,
                // v3.0.3): level flight at half speed in water (`ridden`, set by the world), put
                // out by terrain (collision flags 0xc3402001) [obs; inf: which waters halve it]
                let k = if self.alt && self.ridden { 0.5 } else { 1.0 };
                self.x += self.vx * k;
                self.y += self.vy;
                if self.alt && level.cell_at(self.x, self.y + 4.0).is_solid() {
                    self.mode = Mode::Dead;
                }
                self.timer -= 1;
                if self.timer <= 0 {
                    self.mode = Mode::Dead;
                }
            }
            Kind::Piranha { .. } => {
                // free-standing plants stand on their surface and may fall if unsupported
                if self.pipe.is_none() {
                    self.vx = 0.0;
                    self.vy = (self.vy + ENEMY_GRAVITY).max(MAX_FALL);
                    self.collide(level);
                }
            }
            Kind::Thwomp { side, right } => self.step_thwomp(level, side, right),
            Kind::MovingBlock { .. } => {} // wings: the fly helper (above); a track: its rail
            Kind::TrackMount { .. } => {}  // moved by its rail only
            Kind::BabyBlooper { .. } => {} // placed by the world
            Kind::SideSpring => {
                if self.pause > 0 {
                    self.pause -= 1;
                }
            }
            Kind::EmptyShell { what } => {
                self.kind = Kind::Shell { red: false };
                self.shell_of = what;
                self.buzzy = what != 3; // Buzzy and Spiny shells are fire-proof
                self.enter_shell_wait();
            }
            Kind::GoalToadette => {
                self.vy = (self.vy + ENEMY_GRAVITY).max(MAX_FALL);
                self.collide(level);
            }
            Kind::EscortToad => {
                let (tx, ty) = self.target;
                let dx = tx - self.x;
                let want = if dx.abs() < 2.0 {
                    0.0
                } else {
                    (1.5 * dx.abs() / 16.0).clamp(0.5, 4.0) * dx.signum()
                };
                self.vx += (want - self.vx).clamp(-0.2, 0.2);
                if dx.abs() >= 2.0 {
                    self.dir = dx.signum() as i32;
                }
                if self.on_ground && (ty > self.y + 12.0 || (self.wall && dx.abs() > 4.0)) {
                    self.vy = 4.5;
                    self.on_ground = false;
                }
                self.vy = (self.vy - 0.25).max(MAX_FALL);
                self.collide(level);
            }
            Kind::Stone => {
                if !self.held {
                    self.vy = (self.vy + ENEMY_GRAVITY).max(MAX_FALL);
                    if self.phase == 1 {
                        self.vx = self.vx.clamp(-0.8, 0.8);
                    }
                    self.collide(level);
                    if self.on_ground {
                        self.vx -= self.vx.clamp(-0.1, 0.1);
                        if self.vx == 0.0 {
                            self.phase = 0;
                        }
                    }
                }
            }
            Kind::Trampoline { moving, range, .. } => {
                if moving {
                    // the lift base's Move (0x7100897c20) [obs]: sideways between ±range tiles
                    // of the start (Trampoline init 0x71008c3200: ±3, or ±2 in mode 1), at
                    // min(0.5, 0.125 + 1/16 of the distance to the nearer end), turning at the
                    // ends
                    if self.age <= 1 {
                        self.home_x = self.x;
                    }
                    let r = range as f32 * TILE;
                    let off = self.x - self.home_x;
                    let d = (off + r).min(r - off).max(0.0);
                    let speed = (d * 0.0625 + 0.125).min(0.5);
                    let mut next = off + speed * self.dir as f32;
                    if next.abs() >= r {
                        next = r * self.dir as f32;
                        self.dir = -self.dir;
                    }
                    self.x = self.home_x + next;
                }
            }
            Kind::TrackBlock { path, alt } => self.step_track_block(level, path, alt),
            Kind::Crate { .. } => {
                if !self.held {
                    self.vy = (self.vy + ENEMY_GRAVITY).max(MAX_FALL);
                    self.collide(level);
                    if self.phase == 1 {
                        if self.wall {
                            self.phase = 2; // broken: the world releases the contents
                        } else if self.on_ground {
                            // each landing halves the speed
                            self.vx *= 0.5;
                            if self.vx.abs() < 0.25 {
                                self.vx = 0.0;
                                self.phase = 0;
                            } else {
                                self.vy = 1.0;
                            }
                        }
                    } else if self.on_ground {
                        self.vx = 0.0;
                    }
                }
            }
            Kind::Lift { falling, .. } => {
                if falling && (self.ridden || self.phase == 1) {
                    // flag 0x2: gravity −0.01875, falling at most 1.2 (11-lift.md) [obs v1.0.0
                    // FallLift Move enter 0x7100899500, table 0x7101c8c658]
                    self.phase = 1;
                    let (g, cap) = if self.wings {
                        (-0.01875, -1.2)
                    } else {
                        (-0.0625, -4.0)
                    };
                    self.vy = (self.vy + g).max(cap);
                    self.y += self.vy;
                } else if falling && self.wings && self.rail.is_none() {
                    // a resting winged falling lift bobs 8·sin φ about its height, φ stepping
                    // 0x1000000 (1.40625°) a frame [obs v3.0.3 Stop exe 0x71013c2430, rate
                    // _INIT 0x7100aff810]
                    self.fly.angle.1 = self.fly.angle.1.wrapping_add(0x100_0000);
                    self.y = self.home_y + 8.0 * crate::fmath::table_sin_cos(self.fly.angle.1).0;
                } else if !falling && self.rail.is_none() && !self.derailed {
                    // off a track it paces ±48 px about its spawn (actor 8): speed
                    // min(0.5, d/16 + 0.125) for d the distance to the nearer end, stopping on
                    // the end and turning there (11-lift.md, Move 0x7100897c20) [obs: Ryujinx
                    // trace M1_lift, 232 ↔ 328 about 280]
                    let (lo, hi) = (self.home_x - 48.0, self.home_x + 48.0);
                    let d = (self.x - lo).min(hi - self.x);
                    let sp = (d / 16.0 + 0.125).min(0.5);
                    if self.dir > 0 {
                        self.x = (self.x + sp).min(hi);
                        if self.x >= hi {
                            self.dir = -1;
                        }
                    } else {
                        self.x = (self.x - sp).max(lo);
                        if self.x <= lo {
                            self.dir = 1;
                        }
                    }
                    if self.wings {
                        // flag 0x2 adds 8·sin φ across the motion, φ stepping 0x2000000
                        // (2.8125°) a frame [obs v3.0.3 Move 0x71013c06d0, rate _INIT
                        // 0x7100aff510]
                        self.fly.angle.1 = self.fly.angle.1.wrapping_add(0x200_0000);
                        self.y =
                            self.home_y + 8.0 * crate::fmath::table_sin_cos(self.fly.angle.1).0;
                    }
                }
            }
            Kind::Spring => {
                // compress 1 px/frame for 4 frames, hold 5, then launch and release over 4
                // (10-spring.md); `chain` = 1 on the launch frame (the world launches the rider)
                self.chain = 0;
                match self.phase {
                    // pressed 1 px on the first ridden frame, to 4, then held until the launch
                    // 4 frames later [obs: Ryujinx trace M1_spring: the rider at top, −1, −2,
                    // −3, −4 ×4, launched on the 4th]
                    0 => {
                        if self.ridden {
                            self.phase = 1;
                            self.pause = 1;
                        }
                    }
                    1 => {
                        self.pause += 1;
                        if self.pause >= 4 {
                            self.phase = 2;
                            self.timer = 4;
                        }
                    }
                    2 => {
                        self.timer -= 1;
                        if self.timer <= 0 {
                            self.phase = 3;
                            self.chain = 1;
                        }
                    }
                    _ => {
                        self.pause -= 1;
                        if self.pause <= 0 {
                            self.pause = 0;
                            self.phase = 0;
                        }
                    }
                }
                // the drawn / stood-on press: 1 px a frame to 4; 3D World eases down 3 px
                // [obs: Ryujinx trace 3W_spring: 0, 0, 0.277, 0.985, 1.975, 3 from the first
                // ridden frame, the launch at the same frame as in 2D]
                let n = match self.phase {
                    1 => self.pause,
                    2 => 8 - self.timer,
                    _ => 0,
                };
                self.speed = if level.style == "3W" {
                    const EASE: [f32; 7] = [0.0, 0.0, 0.0, 0.277, 0.985, 1.975, 3.0];
                    if self.phase == 3 {
                        (self.pause as f32 * 0.75).min(3.0)
                    } else {
                        EASE[(n.max(0) as usize).min(6)]
                    }
                } else {
                    self.pause as f32
                };
                self.vy = (self.vy + ENEMY_GRAVITY).max(MAX_FALL);
                self.vx = 0.0;
                self.collide(level);
            }
            Kind::BulletBill => {
                if self.phase == 1 {
                    // Bull's-Eye Bill (from a red blaster): 1°/frame toward the player for 528
                    // frames, then straight on
                    if self.age <= 1 {
                        self.heading = if self.dir < 0 { 180.0 } else { 0.0 };
                    }
                    if self.timer < HOMING_FRAMES {
                        self.home_toward(1.0);
                        self.timer += 1;
                    }
                    let a = self.heading.to_radians();
                    (self.vx, self.vy) = (a.dcos(), a.dsin());
                    self.x += self.vx;
                    self.y += self.vy;
                    if self.vx.abs() > 0.01 {
                        self.dir = self.vx.signum() as i32;
                    }
                } else {
                    self.vx = self.dir as f32;
                    self.x += self.vx;
                }
            }
            Kind::BanzaiBill { dir, homing } => {
                if self.age <= 1 {
                    self.heading = [0.0, -90.0, 180.0, 90.0][dir as usize & 3];
                }
                if homing && self.timer < HOMING_FRAMES {
                    self.home_toward(0.25);
                    self.timer += 1;
                }
                let a = self.heading.to_radians();
                self.x += a.dcos();
                self.y += a.dsin();
            }
            Kind::Donut => {
                // 3D World: gone after 21 frames of its fall (the rider drops on its own from
                // there) [obs: Ryujinx trace 3W_donut]; `pause` counts the fall frames
                self.pause += 1;
                if level.style == "3W" && self.pause > 21 {
                    self.phase = 9;
                    return None;
                }
                self.vy = (self.vy - 0.1875).max(-2.0);
                self.y += self.vy;
            }
            Kind::Seesaw { width } => self.step_seesaw(width),
            Kind::Lakitu => return self.step_lakitu(level),
            Kind::SpinyEgg => {
                self.vy = (self.vy + ENEMY_GRAVITY).max(MAX_FALL);
                self.collide(level);
                if self.on_ground {
                    self.kind = Kind::Spiny;
                    self.dir = if self.target.0 < self.x { -1 } else { 1 };
                }
            }
            Kind::Cloud => {
                if self.ridden {
                    // the rider steers (target_facing carries the input direction here)
                    self.vx = 1.5 * self.target_facing as f32;
                    self.vy = 0.0;
                    self.x += self.vx;
                    if self.timer < 0 {
                        self.timer = 900;
                    }
                } else {
                    // idle bob ±1.6 px around home
                    let below = self.y < self.home_y - 1.6;
                    let t = if self.y > self.home_y + 1.6 {
                        -0.1
                    } else if below || self.vy >= 0.0 {
                        0.1
                    } else {
                        -0.1
                    };
                    self.vy += (t - self.vy).clamp(-0.004, 0.004);
                    self.y += self.vy;
                }
                if self.timer > 0 {
                    self.timer -= 1;
                    if self.timer == 0 {
                        self.mode = Mode::Dead;
                    }
                }
            }
            Kind::Pow | Kind::Key => {}
            Kind::CheepCheep { red } => {
                // `in_water` is passed in `ridden` by the world
                if self.ridden {
                    let t = self.dir as f32;
                    self.vx += (t - self.vx).clamp(-0.02, 0.02);
                    if red && (self.x - self.home_x).abs() > 40.0 {
                        self.dir = if self.x > self.home_x { -1 } else { 1 };
                    }
                    let phase = self.age as f32 * 3.0;
                    let tv = 0.3 * phase.to_radians().dsin();
                    self.vy += (tv - self.vy).clamp(-0.1, 0.1);
                    self.collide(level);
                    if self.wall {
                        self.dir = -self.dir;
                    }
                } else {
                    // out of water: flop, hopping 4.0 on every landing
                    self.vy = (self.vy + ENEMY_GRAVITY).max(MAX_FALL);
                    self.collide(level);
                    if self.on_ground {
                        self.vy = 4.0;
                        self.vx = 0.5 * self.dir as f32;
                        self.on_ground = false;
                    }
                    if self.wall {
                        self.dir = -self.dir;
                    }
                }
            }
            Kind::Blooper => {
                if self.alt {
                    // Blooper Nanny: her path, which the babies follow (48-blooper.md)
                    self.trail.push((self.x, self.y + 8.0));
                    if self.trail.len() > 200 {
                        self.trail.remove(0);
                    }
                }
                let (px, py) = self.target;
                if self.phase == 0 {
                    // sink at 0.5 for at least 30 frames, until level with the player
                    self.speed += (0.5 - self.speed).clamp(-0.5, 0.5);
                    self.x += 0.0;
                    self.y -= self.speed;
                    self.timer += 1;
                    if self.timer >= 30 && self.y <= py + 16.0 {
                        self.phase = 1;
                        self.timer = 0;
                        self.heading = if px >= self.x { 45.0 } else { 135.0 };
                    }
                } else {
                    // pulse: speed 2·|sin φ|, φ += 6°/frame, 38 frames
                    let t = 2.0 * (self.timer as f32 * 6.0).to_radians().dsin().abs();
                    self.speed += (t - self.speed).clamp(-1.0, 1.0);
                    let a = self.heading.to_radians();
                    self.x += self.speed * a.dcos();
                    self.y += self.speed * a.dsin();
                    self.timer += 1;
                    if self.timer >= 38 {
                        self.phase = 0;
                        self.timer = 0;
                        self.speed = 0.0;
                    }
                }
                self.dir = if px >= self.x { 1 } else { -1 };
            }
            Kind::Skewer { .. } => self.step_skewer(),
            Kind::Magic | Kind::Wrench => {
                if self.kind == Kind::Wrench && self.y < self.home_y {
                    self.y += 1.0; // rises first
                } else {
                    self.x += self.vx;
                    self.y += if self.kind == Kind::Magic {
                        self.vy
                    } else {
                        0.0
                    };
                }
                self.timer -= 1;
                if self.timer <= 0 {
                    self.mode = Mode::Dead;
                }
            }
            Kind::RockyWrench => return self.step_rocky(),
            Kind::Wiggler => {
                if self.alt && self.age <= 1 && self.phase == 0 {
                    // the alternate (0x4) Wiggler is built in its Angry state (0x710067bcc0)
                    self.phase = 1;
                    self.timer = 60;
                }
                if self.timer > 0 {
                    self.timer -= 1; // anger pause
                    self.vx = 0.0;
                } else {
                    let speed = if self.phase == 1 { 1.0 } else { 0.5 };
                    if self.phase == 1 && self.on_ground && self.age.is_multiple_of(64) {
                        self.dir = if self.target.0 < self.x { -1 } else { 1 };
                    }
                    if self.on_ground {
                        self.vx = speed * self.dir as f32;
                    }
                }
                self.vy = (self.vy + ENEMY_GRAVITY).max(MAX_FALL);
                self.collide(level);
                if self.wall && self.turn_lock == 0 {
                    self.turn();
                }
                self.trail.push((self.x, self.y));
                if self.trail.len() > 200 {
                    self.trail.remove(0);
                }
            }
            Kind::LavaLift { fast } => {
                if self.ridden || self.phase == 1 {
                    self.phase = 1;
                    self.x += if fast { 2.5 } else { 0.8 };
                }
            }
            Kind::Magikoopa => {}
            Kind::Claw => {
                if self.timer > 0 {
                    self.timer -= 1; // re-grab cooldown
                }
                // the rope (actors/claw.rs) [obs v3.0.3 0x71013e7b10 → 0x7101ca03f0]
                self.step_claw();
            }
            Kind::Twister => {
                if self.on_ground {
                    self.vx = 0.5 * self.dir as f32;
                }
                self.vy = (self.vy + ENEMY_GRAVITY).max(MAX_FALL);
                self.collide(level);
                if self.wall {
                    self.dir = -self.dir;
                }
                // column height in `heading`
                self.heading = (self.heading + 0.7).clamp(12.0, 128.0);
            }
            Kind::Cannonball => {
                self.x += self.vx;
                self.y += self.vy;
            }
            Kind::Bumper => {
                if self.timer > 0 {
                    self.timer -= 1;
                }
            }
            Kind::Grinder => {}
            Kind::PSwitch => {
                // stood on for 3 frames: pressed (`chain` = 1 on that frame; the world flips
                // the switch) [obs: Ryujinx trace M1_pswitch, the rider down to 4.8 on the 3rd
                // frame after landing]
                self.chain = 0;
                if self.phase == 0 && self.ridden {
                    self.pause += 1;
                    if self.pause >= 3 {
                        self.phase = 1;
                        self.timer = 34;
                        self.chain = 1;
                    }
                }
                if self.phase > 0 && self.chain == 0 {
                    // pressed: squash animation, then removed (PushReserve 2 + DelWait 2 + anim)
                    self.timer -= 1;
                    if self.timer <= 0 {
                        self.mode = Mode::Dead;
                    }
                } else {
                    self.vx = 0.0;
                    self.vy = (self.vy + ENEMY_GRAVITY).max(MAX_FALL);
                    self.collide(level);
                }
            }
            Kind::Muncher => {}
            Kind::BigCoin { .. } | Kind::Shell { .. } => {}
        }
        None
    }

    // ------------------------------------------------------------ Spike Top

    /// Spike Top crawler (40-spike-top.md, Basic action 0x7102187530). The body is a square
    /// (half-size h = 8, big 16) centred at (x, y + h); `heading` is the body angle θ (degrees:
    /// 0 upright on a floor, 90 on a wall to its right, 180 on a ceiling, 270 on a wall to its
    /// left). It crawls along σ·(cos θ, sin θ), σ = `dir`, feet toward (sin θ, −cos θ), at 0.5
    /// px/f (alternate 0.75) [obs 0x7100778710 tail, 0x710077a6d0]. Phases: 0 floor or air,
    /// 1 wall or ceiling, 2 inside corner (RotationWall: turns in place), 3 outside corner
    /// (RotationCliff: swings round the block corner), 4 Stick (rights itself in place). A turn
    /// keeps its target angle in `pause`, its start angle in `speed`, σ in `hops` (0 = +1) and
    /// the binary angle it has left to turn in `timer`.
    fn step_spike_top(&mut self, level: &Level, surface: u8) {
        let h = if self.big { 16.0 } else { 8.0 };
        if self.age <= 1 {
            // spawn orientation and crawl sense per placed side (Start 0x7100777b10): on a
            // wall it climbs, on a ceiling it keeps its on-screen direction
            let (theta, sigma) = match surface {
                0 => (90, 1),
                1 => (270, -1),
                2 => (180, -self.dir),
                _ => (0, self.dir),
            };
            self.heading = theta as f32;
            self.pause = theta;
            self.dir = sigma;
            self.phase = if theta == 0 { 0 } else { 1 };
        }
        match self.phase {
            1 => self.spike_top_crawl(level, h),
            2 => self.spike_top_turn(level, h, false),
            3 => self.spike_top_turn(level, h, true),
            4 => self.spike_top_stick(level),
            _ => self.spike_top_floor(level, h),
        }
    }

    fn spike_top_speed(&self) -> f32 {
        if self.alt {
            SPIKE_TOP_WALK * 1.5
        } else {
            SPIKE_TOP_WALK
        }
    }

    /// On the floor (Walk) or in the air (InAir 0x71007781d0: gravity −0.1875, cap −4, the
    /// body righting itself at 15°/frame). Where the floor under its feet from the centre to
    /// the front ends (the Walk's ray along the feet, 0x710231c300: from under the centre 8 px
    /// ahead, big 16 [obs v3.0.3 0x7101276c70]) it wraps the block's corner if solid terrain
    /// is 3.5 px behind the centre and 4 px under the feet, else it reverses (8-frame lock) —
    /// so it turns back at the end of a semisolid; a block in the way is climbed, anything
    /// else (the area edge) turns it [obs 0x7100778710].
    fn spike_top_floor(&mut self, level: &Level, h: f32) {
        let speed = self.spike_top_speed();
        let s = self.dir as f32;
        if self.on_ground {
            let under = match level.cell_at(self.x, self.y - 0.5) {
                Cell::Empty => level.cell_at(self.x + s * (h - 0.01), self.y - 0.5),
                c => c,
            };
            let slope = level
                .slope_at(self.x)
                .is_some_and(|(top, _)| top >= self.y - 8.0);
            if under == Cell::Empty && !slope && self.turn_lock == 0 {
                if level.cell_at(self.x - 3.5 * s, self.y - 4.0).is_solid() {
                    self.spike_top_begin(true);
                    return;
                }
                self.turn();
            }
            self.vx = speed * self.dir as f32;
        } else {
            self.vx = self.vx.clamp(-speed, speed);
            let rate = if self.slow_rule(level) {
                SPIKE_TOP_STICK * 0.5
            } else {
                SPIKE_TOP_STICK
            };
            self.heading = turn_toward(self.heading, 0.0, rate);
        }
        let rising = self.vy > 0.0;
        let was_ground = self.on_ground;
        self.vy = (self.vy + ENEMY_GRAVITY).max(MAX_FALL);
        self.collide(level);
        if rising && self.vy == 0.0 && !self.on_ground {
            // bumped a ceiling: it sticks there, flipping σ to keep its screen direction
            // (InAir → Stick 0x7100778400)
            self.pause = 180;
            self.dir = -self.dir;
            self.phase = 4;
            return;
        }
        if self.on_ground && !was_ground && self.heading != 0.0 {
            self.pause = 0;
            self.phase = 4;
            return;
        }
        if self.wall && self.on_ground && self.turn_lock == 0 {
            let s = self.dir as f32;
            if level.cell_at(self.x + s * (h + 1.0), self.y + h).is_solid() {
                self.spike_top_begin(false);
            } else {
                self.turn();
            }
        }
    }

    /// On a wall or the ceiling: a block across the leading face is an inside corner; where
    /// the surface under its feet from the centre to the front ends it wraps the corner (solid
    /// 3.5 px behind the centre, 4 px beyond the feet); with nothing to cling to it falls
    /// (InAir) [obs 0x7100778710; v3.0.3 Walk 0x7101276c70].
    fn spike_top_crawl(&mut self, level: &Level, h: f32) {
        let solid = |x: f32, y: f32| level.cell_at(x, y).is_solid();
        let (u, f) = crawl_axes(self.pause);
        let s = self.dir as f32;
        let t = (u.0 * s, u.1 * s);
        let speed = self.spike_top_speed();
        let mut cx = self.x + speed * t.0;
        let mut cy = self.y + h + speed * t.1;
        let (lx, ly) = (cx + (h + 0.01) * t.0, cy + (h + 0.01) * t.1);
        let blocked = [-(h - 1.0), 0.0, h - 1.0]
            .iter()
            .any(|&k| solid(lx + k * f.0, ly + k * f.1));
        if blocked {
            // flush against the block, then turn in place
            let face = |v: f32, d: f32| {
                if d > 0.0 {
                    (v / TILE).floor() * TILE
                } else {
                    ((v / TILE).floor() + 1.0) * TILE
                }
            };
            if t.0 != 0.0 {
                cx = face(lx, t.0) - h * t.0;
            } else {
                cy = face(ly, t.1) - h * t.1;
            }
            self.x = cx;
            self.y = cy - h;
            self.spike_top_begin(false);
            return;
        }
        self.x = cx;
        self.y = cy - h;
        let surface = |k: f32| {
            solid(
                cx + k * t.0 + (h + 0.5) * f.0,
                cy + k * t.1 + (h + 0.5) * f.1,
            )
        };
        if surface(0.0) || surface(h - 0.01) {
            return;
        }
        let (bx, by) = (
            cx - 3.5 * t.0 + (h + 4.0) * f.0,
            cy - 3.5 * t.1 + (h + 4.0) * f.1,
        );
        if solid(bx, by) {
            self.spike_top_begin(true);
            return;
        }
        // nothing to cling to: it falls as a floor walker (InAir 0x7100778140 resets the
        // surface to the floor)
        self.pause = 0;
        self.phase = 0;
        self.on_ground = false;
        self.vx = 0.0;
        self.vy = 0.0;
    }

    /// Start a corner turn: inside corners turn the body +90°·σ, outside corners −90°·σ. The
    /// enters (0x71012784d0 / 0x7101278720 [obs v3.0.3]) stop it where it is: round an
    /// outside corner its centre starts just past the edge.
    fn spike_top_begin(&mut self, cliff: bool) {
        let s = self.dir;
        let quarter = if cliff { -90 * s } else { 90 * s };
        self.speed = self.pause as f32;
        self.pause = (self.pause + quarter).rem_euclid(360);
        self.hops = u8::from(s < 0);
        self.timer = QUARTER_TURN;
        self.phase = if cliff { 3 } else { 2 };
        self.vx = 0.0;
        self.vy = 0.0;
        self.on_ground = false;
    }

    /// RotationWall / RotationCliff [obs v3.0.3 exes 0x7101278570 / 0x7101278940]: the body
    /// angle steps 3° a frame (1.5° under the water-speed rule) toward the new quarter and is
    /// done once what is left is within a step — 31 frames (61 slow) — then it walks on. An
    /// inside corner turns in place. Round an outside corner it moves 0.34 px/f (big 0.72)
    /// along θ + 5°·σ, θ taken before the step, with terrain off (enter 0x7101278720: +0x524
    /// bit 4) and the integrator halving the move under the rule (0x7101290e30) — a quarter
    /// circle of radius ≈ 6.5 px (13.75), the motion 5° outside the body's tangent. Each frame
    /// the contact probe (0x7101279310, cliff mode) needs ground at the feet + R(θ)·(σ·(6 −
    /// 10·q), −4) (big σ·(8 − 12·q); q = the quarter turn left; offsets 0x7101279020 /
    /// 0x7101279030 → 0x71023ce47c / 0x71023ce46c), else it drops (InAir, its speed zeroed).
    /// The turn ends with the body 0.5–0.8 px into the new face, the next frame's BG pushing
    /// it out [inf: modelled as a snap of the feet onto the face].
    fn spike_top_turn(&mut self, level: &Level, h: f32, cliff: bool) {
        let s: i32 = if self.hops == 0 { 1 } else { -1 };
        let sf = s as f32;
        // +1 = the body angle rising (counter-clockwise)
        let sign = if cliff { -s } else { s };
        let slow = self.slow_rule(level);
        let rate = if slow {
            SPIKE_TOP_TURN_SLOW
        } else {
            SPIKE_TOP_TURN
        };
        let start = (self.speed as i32 / 90).wrapping_mul(QUARTER_TURN) as u32;
        let angle = |left: i32| start.wrapping_add(sign.wrapping_mul(QUARTER_TURN - left) as u32);
        let left = self.timer;
        let theta = angle(left);
        self.vx = 0.0;
        self.vy = 0.0;
        if cliff {
            let v = if self.big {
                SPIKE_TOP_CLIFF_SPEED_BIG
            } else {
                SPIKE_TOP_CLIFF_SPEED
            };
            let lead = theta.wrapping_add(s.wrapping_mul(SPIKE_TOP_CLIFF_LEAD) as u32);
            let (sn, cs) = crate::fmath::table_sin_cos(lead);
            self.vx = cs * v * sf;
            self.vy = sn * v * sf;
            // the probe: feet + R(θ)·(a, b)
            let (sn, cs) = crate::fmath::table_sin_cos(theta);
            let (ox, ease) = if self.big { (8.0, 12.0) } else { (6.0, 10.0) };
            let q = left as f32 * f32::from_bits(0x3000_0000) * 2.0;
            let (a, b) = (ox * sf - ease * sf * q, -4.0);
            let (fx, fy) = (self.x + h * sn, self.y + h - h * cs);
            if !level
                .cell_at(fx + (cs * a - sn * b), fy + (sn * a + cs * b))
                .is_solid()
            {
                // InAir (its enter 0x71012766a0 aims it back at the floor quarter)
                self.pause = 0;
                self.phase = 0;
                self.dir = s;
                self.on_ground = false;
                self.vx = 0.0;
                self.vy = 0.0;
                return;
            }
        }
        let done = left <= rate;
        self.timer = if done { 0 } else { left - rate };
        self.heading = (angle(self.timer) as f64 * (360.0 / 4_294_967_296.0)) as f32;
        if cliff {
            let m = if slow { 0.5 } else { 1.0 };
            self.x += self.vx * m;
            self.y += self.vy * m;
        }
        self.vx = 0.0;
        self.vy = 0.0;
        if done {
            self.dir = s;
            self.spike_top_settle();
            if cliff {
                // the BG's push-out onto the new face
                let (_, f) = crawl_axes(self.pause);
                let snap = |v: f32| (v / TILE).round() * TILE - v;
                if f.0 != 0.0 {
                    let d = snap(self.x + h * f.0);
                    if d.abs() < 2.0 {
                        self.x += d;
                    }
                } else {
                    let d = snap(self.y + h + h * f.1);
                    if d.abs() < 2.0 {
                        self.y += d;
                    }
                }
            }
        }
    }

    /// Stick (0x71007784c0): still, it rights itself toward `pause` at 15°/frame (80°/frame
    /// onto a ceiling), then walks on.
    fn spike_top_stick(&mut self, level: &Level) {
        self.vx = 0.0;
        self.vy = 0.0;
        let mut rate = if self.pause == 180 {
            SPIKE_TOP_STICK_CEILING
        } else {
            SPIKE_TOP_STICK
        };
        if self.slow_rule(level) {
            rate *= 0.5;
        }
        self.heading = turn_toward(self.heading, self.pause as f32, rate);
        if self.heading == self.pause as f32 {
            self.spike_top_settle();
        }
    }

    fn spike_top_settle(&mut self) {
        self.heading = self.pause as f32;
        if self.pause == 0 {
            self.phase = 0;
            self.on_ground = true;
        } else {
            self.phase = 1;
            self.on_ground = false;
        }
    }

    /// Lit Bob-omb (15-bob-omb.md): fuse 240 frames; stopped (phase 1), kicked (phase 2,
    /// sliding with friction 0.03125, bouncing) or placed lit and walking (phase 3). At 0 it
    /// explodes (`phase` = 9, world handles).
    fn step_lit_bobomb(&mut self, level: &Level) {
        self.timer -= 1;
        if self.phase == 2 {
            let f = 0.03125;
            self.vx = if self.vx > 0.0 {
                (self.vx - f).max(0.0)
            } else {
                (self.vx + f).min(0.0)
            };
        } else if self.phase == 3 {
            // placed lit: walks on until it goes off
            if self.on_ground {
                self.vx = WALK_SPEED * self.dir as f32;
            }
        } else if self.on_ground {
            self.vx = 0.0;
        }
        self.vy = (self.vy + ENEMY_GRAVITY).max(MAX_FALL);
        let impact = self.vy;
        self.collide(level);
        if self.on_ground && impact < -0.1875 && self.hops < 2 {
            self.vy = 0.5 * impact.abs();
            self.on_ground = false;
            self.hops += 1;
        }
        if self.wall {
            self.vx = -self.vx;
            self.dir = -self.dir;
        }
        if self.timer <= 0 {
            self.phase = 9;
        }
    }

    /// Kick a lit Bob-omb: 2.5, or min(2.5 + 0.35·|v|, 4.0) moving that way; vy 2.0.
    pub fn kick_bobomb(&mut self, dir: i32, kicker_vx: f32) {
        let mut speed = 2.5;
        if kicker_vx != 0.0 && kicker_vx.signum() as i32 == dir {
            speed = (2.5 + 0.35 * kicker_vx.abs()).min(4.0);
        }
        self.phase = 2;
        self.dir = dir;
        self.vx = speed * dir as f32;
        self.vy = 2.0;
        self.on_ground = false;
        self.hops = 0;
        self.kick_lock = 16;
    }

    /// Hammer Bro (3-hammer-bro.md, SMB1 "Bros1st"): paces ±16 px around home at 0.3; every
    /// 300–449 frames, standing on flat ground, it jumps (decision 0x71006205d0): up or down at
    /// random, down forced within 8 px of the view top, down only when a floor to land on is
    /// found below (else up; a forced one waits). Up launches 5.5 (winged 6.5), down 2.5, with
    /// no sideways speed, passing through terrain until it can land (`bro_ghost_move`). Throws
    /// hammers every 75–104 frames (70 % chance of a double, 10 frames apart), always facing the
    /// player. Jump state in `hops` (0 none, 2 up, 3 down), start height in `speed`.
    fn step_hammer_bro(&mut self, level: &Level) -> Option<Actor> {
        if self.alt {
            return self.step_sledge_bro(level);
        }
        if self.wings && !self.stacked {
            return self.step_bro_wing(level);
        }
        let (px, _) = self.target;
        self.dir = if px < self.x { -1 } else { 1 };
        if self.on_ground && self.hops == 0 {
            let d = self.home_x - self.x;
            let pace = if self.phase == 0 { 1.0 } else { -1.0 };
            if (pace > 0.0 && d < -16.0) || (pace < 0.0 && d > 16.0) {
                self.phase ^= 1;
            }
            let target = 0.3 * if self.phase == 0 { 1.0 } else { -1.0 };
            self.vx = if self.vx < target {
                (self.vx + 0.1).min(target)
            } else {
                (self.vx - 0.1).max(target)
            };
        }
        // jump timer in `pause`
        if self.pause <= 0 {
            self.pause = 300 + self.rand(150) as i32;
        }
        self.pause -= 1;
        let flat = level
            .slope_at(self.x)
            .is_none_or(|(top, _)| top < self.y - 8.0);
        if self.pause == 0 && self.on_ground && self.hops == 0 && flat {
            match self.bro_jump_kind(level) {
                Some(down) => {
                    self.hops = if down { 3 } else { 2 };
                    self.vy = if down {
                        2.5
                    } else if self.wings {
                        6.5
                    } else {
                        5.5
                    };
                    self.vx = 0.0;
                    self.speed = self.y;
                    self.on_ground = false;
                }
                None => self.pause = 1, // forced down with nowhere to land: try again
            }
        }
        self.vy = (self.vy + ENEMY_GRAVITY).max(MAX_FALL);
        if self.hops != 0 {
            if self.bro_ghost_move(level, ENEMY_GRAVITY) {
                // landed: Walk again, pacing round here (home re-latched on landing)
                self.home_x = self.x;
            }
        } else {
            self.collide(level);
        }
        if self.wall {
            self.phase ^= 1;
        }
        // throw timer in `timer`; second hammer pending in `chain`
        if self.timer <= 0 {
            self.timer = 75 + self.rand(30) as i32;
            // the throw needs a unit of the area's projectile budget: refused, the helper only
            // redraws its timer (0x7101098560 case 2 → case 1) [obs v3.0.3]
            if !self.budget_request(1) {
                return None;
            }
            if self.rand(100) < 70 {
                self.chain = 10;
            }
            return Some(self.make_hammer());
        }
        self.timer -= 1;
        if self.chain > 0 {
            self.chain -= 1;
            if self.chain == 0 {
                return Some(self.make_hammer());
            }
        }
        None
    }

    /// The Bros' jump decision (0x71006205d0): Some(true) to jump down, Some(false) up, None to
    /// wait. Up or down at random; within 8 px of the view top it must go down, and down needs
    /// a landing spot below (0x71007a3a60: rows 16 px apart, down to 16 px above the view
    /// bottom, where a floor lies under x ± 4 and the 24 px above it is clear at x ± 4..20);
    /// a random down without one becomes up.
    fn bro_jump_kind(&mut self, level: &Level) -> Option<bool> {
        use crate::world::VIEW_H;
        let forced = self.y >= self.camera.1 + VIEW_H - 8.0;
        let down = forced || self.rand(2) == 1;
        if !down {
            return Some(false);
        }
        if self.bro_landing_below(level) {
            Some(true)
        } else if forced {
            None
        } else {
            Some(false)
        }
    }

    fn bro_landing_below(&self, level: &Level) -> bool {
        let floor = |x: f32, y: f32| {
            let c = level.cell_at(x, y);
            c.is_solid() || c == Cell::SemiTop
        };
        let solid = |x: f32, y: f32| level.cell_at(x, y).is_solid();
        let lowest = (self.camera.1 + 16.0).max(level.bottom);
        let mut top = (self.y / TILE).round() * TILE - TILE;
        while top >= lowest {
            let clear = [-20.0, -12.0, -4.0, 4.0, 12.0, 20.0]
                .iter()
                .all(|&dx| !solid(self.x + dx, top + 4.0) && !solid(self.x + dx, top + 20.0));
            if clear && (floor(self.x - 4.0, top - 8.0) || floor(self.x + 4.0, top - 8.0)) {
                return true;
            }
            top -= TILE;
        }
        false
    }

    /// Airborne through terrain after a Bros jump (the launch 0x7100621040 sets the
    /// no-terrain flag; 0x71007a4b20 clears it). It may land again — a down jump once 16 px
    /// below its start, an up jump once falling (and, near the view top, no more than 8 px above
    /// its start) — when its body is clear (columns x ± 4 from 1 px over its feet to about 21 px,
    /// plus x ± 12 / ± 20 except near its starting height) and a floor lies within this frame's
    /// fall (|vy| + max(|g| + 0.5, 0.5)) under x ± 4; it snaps onto it. `vy` is this frame's
    /// speed. Returns true on landing.
    fn bro_ghost_move(&mut self, level: &Level, g: f32) -> bool {
        use crate::world::VIEW_H;
        let up = self.hops == 2;
        let start = self.speed;
        let near_top = self.y + self.vy >= self.camera.1 + VIEW_H - 8.0;
        let allowed = if up {
            self.vy <= 0.0 && (!near_top || self.y + self.vy <= start + 8.0)
        } else {
            self.y <= start - 16.0
        };
        if allowed {
            let solid = |x: f32, y: f32| level.cell_at(x, y).is_solid();
            let floor = |x: f32, y: f32| {
                let c = level.cell_at(x, y);
                c.is_solid() || c == Cell::SemiTop
            };
            let near_start = self.y + 1.0 >= start && self.y + 1.0 <= start + 24.0;
            let reach_up = if near_start { 4.8 } else { 20.8 };
            let column_clear = |dx: f32| {
                !solid(self.x + dx, self.y + 1.0) && !solid(self.x + dx, self.y + reach_up)
            };
            let mut clear = column_clear(-4.0) && column_clear(4.0);
            if !near_start {
                clear &= [-20.0, -12.0, 12.0, 20.0]
                    .iter()
                    .all(|&dx| column_clear(dx));
            }
            let reach = self.vy.abs() + (g.abs() + 0.5).max(0.5);
            let top = (self.y / TILE).floor() * TILE;
            let under = |dx: f32| floor(self.x + dx, top - 0.5);
            if clear && top >= self.y - reach && (under(-4.0) || under(4.0)) {
                self.y = top;
                self.vy = 0.0;
                self.hops = 0;
                self.on_ground = true;
                return true;
            }
        }
        self.x += self.vx;
        self.y += self.vy;
        self.on_ground = false;
        false
    }

    /// Sledge Bro (the Hammer Bro's 0x4 form, MegaBros; 3-hammer-bro.md): paces like a Hammer
    /// Bro; every 360 + U[0,180) frames jumps (vy 5.1, 6.5 winged), hovers 9 frames at the top
    /// (Hovering enter 0x71011d2b20 sets 10, its exe 0x71011d2b50 holds while the count is
    /// still above 1 and hip-drops on the 10th), hip-drops (gravity −0.75, cap −8) and quakes on
    /// landing (the world stuns grounded players), then pauses 50 frames; throws single
    /// hammers every 40 + U[0,40) frames.
    ///
    /// Winged, it keeps this action: MegaBros overrides the slots that send a winged Bro into
    /// WingWalk (0x128 → 0x71011d2710 returns 0, 0x130 → 0x71011d2720 always Walk; its Quake
    /// exe 0x71011d3120 ends in Walk too), and its throw helpers' Wing-context gate (helper
    /// slot 0x88: 0x71011d3680 / 0x71011d32e0 return 0, the Bros' 0x7101091f30 returns 1)
    /// leaves the timed throws on [obs v3.0.3, action vtable 0x71028f6b30, helpers
    /// 0x71028f7340 / 0x71028f7498 / 0x71028f7bc8]. Its wings change only the jump (6.5, slot
    /// 0x190) and the hover, which bobs: vy = 0.4 · cos θ, θ from 90° by 8° (0x5b05b04) a frame
    /// (a dip of about 1.8 px) [obs 0x71011d2b50]. [?] the 50-frame hover (+0x520 bit 25).
    /// Jump state in `heading`: 0 normal, > 0 the hover count, −1 dropping; landing pause in
    /// `kick_lock` (50 on the landing frame).
    fn step_sledge_bro(&mut self, level: &Level) -> Option<Actor> {
        let (px, _) = self.target;
        self.dir = if px < self.x { -1 } else { 1 };
        if self.on_ground && self.kick_lock == 0 && self.heading == 0.0 {
            let d = self.home_x - self.x;
            let pace = if self.phase == 0 { 1.0 } else { -1.0 };
            if (pace > 0.0 && d < -16.0) || (pace < 0.0 && d > 16.0) {
                self.phase ^= 1;
            }
            let target = 0.3 * pace;
            self.vx += (target - self.vx).clamp(-0.1, 0.1);
        } else if self.on_ground {
            self.vx = 0.0;
        }
        if self.pause <= 0 {
            self.pause = 360 + self.rand(180) as i32;
        }
        self.pause -= 1;
        if self.pause == 0 && self.on_ground && self.heading == 0.0 {
            // JumpUp (0x7100704bc0): straight up through terrain like a Hammer Bro's up jump,
            // landing again during the hip-drop (0x71007050e0 → 0x71007a4b20)
            self.vy = if self.wings { 6.5 } else { 5.1 };
            self.vx = 0.0;
            self.hops = 2;
            self.speed = self.y;
            self.on_ground = false;
            self.home_x = self.x;
        }
        if self.heading > 0.0 && self.heading <= 1.0 {
            self.heading = -1.0; // the count's last frame: HipDrop (enter 0x71011d2cf0)
        }
        let g = if self.heading > 0.0 {
            let k = (SLEDGE_HOVER - self.heading) as u32;
            self.heading -= 1.0;
            self.vx = 0.0;
            self.vy = if self.wings {
                let angle = 0x4000_0000u32.wrapping_add(k.wrapping_mul(SLEDGE_BOB_STEP));
                SLEDGE_BOB * crate::fmath::table_sin_cos(angle).1
            } else {
                0.0
            };
            0.0
        } else if self.heading < 0.0 {
            self.vy = (self.vy - 0.75).max(-8.0);
            -0.75
        } else {
            let before = self.vy;
            self.vy = (self.vy + ENEMY_GRAVITY).max(MAX_FALL);
            if before > 0.0 && self.vy <= 0.0 && !self.on_ground {
                self.heading = SLEDGE_HOVER; // the hover at the top
            }
            ENEMY_GRAVITY
        };
        // (`kick_lock` counts down in `step`)
        if self.hops == 0 {
            self.collide(level);
        } else if self.heading > 0.0 {
            // (hovering it moves only by the winged bob; it can land only once it drops)
            self.y += self.vy;
        } else {
            self.bro_ghost_move(level, g);
        }
        if self.heading < 0.0 && self.on_ground {
            self.heading = 0.0;
            self.kick_lock = 50; // landing quake
        }
        if self.wall {
            self.phase ^= 1;
        }
        if self.timer <= 0 {
            self.timer = 40 + self.rand(40) as i32;
            // (the Bros' throw helper and its budget request, as the Hammer Bro's)
            if !self.budget_request(1) {
                return None;
            }
            return Some(self.make_hammer());
        }
        self.timer -= 1;
        None
    }

    fn make_hammer(&self) -> Actor {
        let d = self.dir as f32;
        let mut h = Actor::new(Kind::Hammer, self.x - d * 3.0, self.y + 18.0);
        h.vx = 1.5 * d;
        h.vy = 3.0;
        h.dir = self.dir;
        h
    }

    /// Koopa Clown Car (42-koopa-clown-car.md). Empty: velocity × 0.95 with a ±0.2 bob in 0.01
    /// steps. With an enemy rider (Chase, 0x710118bc60): within 320 px across and 800 px up or
    /// down of the player it homes on him at up to 1.0 px/f, accelerating 0.02 (the Fire car
    /// at half the speed, with its charge and shots: actors/clown.rs), facing him when he is
    /// more than 16 px away; out of that range it drifts as Free does. Flown by the player:
    /// moved by the world (`World::fly_clown_car`). Bob speed in `heading`. Stopped
    /// (KoopaJrStop, exe 0x710118f8b0): velocity 0 every frame [obs].
    fn step_clown_car(&mut self, level: &Level) -> Option<Actor> {
        let (px, py) = self.target;
        let (dx, dy) = (px - self.x, py + 8.0 - self.y);
        let chase = self.phase == 1 && dx.abs() <= 320.0 && dy.abs() <= 800.0;
        // (a player flying it runs the record from the world: `clown_fire_player`)
        if !chase && self.phase != 2 {
            self.clown_fire_idle();
        }
        match self.phase {
            1 if chase => {
                let scale = if self.big { 2.0 } else { 1.0 };
                let turn = self.clown_chase((dx, dy), (1.0, 0.02), (self.x, self.y), scale);
                if turn && dx.dhypot(dy) > 16.0 && dx != 0.0 {
                    self.dir = if dx < 0.0 { -1 } else { 1 };
                }
                self.collide(level);
            }
            0 | 1 => {
                self.vx *= 0.95;
                if self.timer <= 0 {
                    self.timer = 40;
                    self.hops ^= 1;
                }
                self.timer -= 1;
                self.heading =
                    (self.heading + if self.hops == 0 { 0.01 } else { -0.01 }).clamp(-0.2, 0.2);
                self.vy = self.heading;
                self.collide(level);
            }
            3 => {
                self.vx = 0.0;
                self.vy = 0.0;
            }
            _ => {}
        }
        None
    }

    /// Goomba's Shoe enemy (45-goombas-shoe.md): on the ground it brakes to 0 by 0.5/frame and
    /// pauses 50 frames, then hops toward the player (vy 4.5, vx 1.5); in the air a wall flips
    /// it (4-frame lock). Pause in `timer`.
    fn step_shoe_goomba(&mut self, level: &Level) {
        if self.wings {
            self.step_shoe_wings(level);
            return;
        }
        let (px, _) = self.target;
        // the 50-frame pause, then the hop's wind-up animation: it hops once that ends (Walk
        // 0x71006fef60 waits for the animation helper's state 2), 24 frames [obs: Ryujinx trace
        // W_3145588, a big Stiletto's first hop 74 frames after it appears]
        if self.on_ground {
            self.vx -= self.vx.clamp(-0.5, 0.5);
            if self.timer <= 0 {
                self.timer = 50 + 24;
            }
            self.timer -= 1;
            if self.timer == 0 {
                self.dir = if px < self.x { -1 } else { 1 };
                self.vy = 4.5;
                self.vx = 1.5 * self.dir as f32;
                self.on_ground = false;
            }
        }
        // in the air, last frame's wall flips it before this frame's move (Walk 0x71006fef60
        // reads the wall flag of the last move) [obs: Ryujinx trace W_3145588, a big Stiletto
        // against a wall a frame, then off it at 1.5]
        if std::mem::take(&mut self.wall_prev) && !self.on_ground && self.turn_lock == 0 {
            self.turn();
            self.vx = 1.5 * self.dir as f32;
            self.turn_lock = 4;
        }
        self.vy = (self.vy + ENEMY_GRAVITY).max(MAX_FALL);
        self.collide(level);
        self.wall_prev = self.wall && !self.on_ground;
        if self.on_ground && self.vy <= 0.0 && self.timer <= 0 {
            self.timer = 50 + 24;
        }
    }

    /// A winged Goomba's Shoe (KutsuKuribo Wing, Jump 0x71006ff480 / exe 0x71006ff530): on the
    /// ground it brakes, waits 50 frames and its 12-frame wind-up, then hops (vy 4.0, 0.5 toward
    /// the player); from the hop's top it hovers, y moved by amplitude · sin θ a frame (θ from 0
    /// stepping −5°, the amplitude rising from 0 by 0.5 to 1, gravity off) for 60 frames, then
    /// holds that last step 40 frames, then falls (−0.1875) to the ground [obs: Ryujinx trace
    /// W_3145588, a winged Stiletto]. Phases 0 ground, 1 hop / hover, 2 hold, 3 fall; θ steps in
    /// `hops`, the amplitude in `speed`, frames in `timer`.
    fn step_shoe_wings(&mut self, level: &Level) {
        let (px, _) = self.target;
        // the engine's sine: 256 steps a turn, linear between them (a 24-bit fraction); θ after k
        // steps of −1/72 turn (0xfc71c720) [obs: the y steps of the trace, 0.49997 at −30°]
        let sin = |k: u8| {
            let a = (k as u32).wrapping_mul(0xfc71_c720);
            let i = (a >> 24) as f64;
            let s0 = libm::sin(i * std::f64::consts::TAU / 256.0) as f32;
            let s1 = libm::sin((i + 1.0) * std::f64::consts::TAU / 256.0) as f32;
            s0 + (s1 - s0) * (a & 0xff_ffff) as f32 * 5.960_464_5e-8
        };
        let mut gravity = true;
        match self.phase {
            0 => {
                if self.on_ground {
                    self.vx -= self.vx.clamp(-0.5, 0.5);
                    if self.timer <= 0 {
                        self.timer = 50 + 12;
                    }
                    self.timer -= 1;
                    if self.timer == 0 {
                        self.speed = self.vy.abs();
                        self.dir = if px < self.x { -1 } else { 1 };
                        self.vx = 0.5 * self.dir as f32;
                        if self.vy <= 0.0 {
                            self.vy = 4.0;
                        }
                        self.on_ground = false;
                        self.timer = 60;
                        self.hops = 0;
                        self.phase = 1;
                    }
                }
            }
            1 => {
                if self.vy <= 0.0 {
                    self.y += self.speed * sin(self.hops);
                    self.hops = self.hops.wrapping_add(1);
                    if self.speed < 1.0 {
                        self.speed = (self.speed + 0.5).min(1.0);
                    }
                    self.vy = 0.0;
                    gravity = false;
                    self.timer -= 1;
                    if self.timer <= 0 {
                        self.timer = 40;
                        self.phase = 2;
                    }
                }
            }
            2 => {
                self.timer -= 1;
                if self.timer <= 0 {
                    self.phase = 3;
                } else {
                    self.y += self.speed * sin(self.hops);
                    self.vy = 0.0;
                    gravity = false;
                }
            }
            _ => {
                if self.on_ground {
                    self.phase = 0;
                    self.timer = 50 + 12;
                }
            }
        }
        if gravity {
            self.vy = (self.vy + ENEMY_GRAVITY).max(MAX_FALL);
        }
        self.collide(level);
    }

    /// The style power-up. SMB1's Big Mushroom and 3D World's Super Bell are ItemDefault items
    /// (20-super-mushroom.md): the Big Mushroom walks like the Super Mushroom, the Bell waits
    /// where it lands like the Fire Flower (landing choice vfunc +0x4d8: 0x71008357f0 → Move
    /// for BigKinoko, 0x7100837860 → Wait for SuperBell) [obs]. The Super Leaf (SMB3) and Cape
    /// Feather (SMW) share the ItemLeaf preset (vtable 0x71021967b8): out of an upward block
    /// they shoot up through the terrain at 4.0 (gravity −0.25) until they stop rising (down:
    /// −0.24, gravity −0.0625, for 9 frames) (Block Out 0x710082ecb0 / 0x710082ec80), then sway
    /// down (Move 0x710082dd70): the sideways speed swings between ±2 at 0.125/frame (the push
    /// flips once |vx| ≥ 2, 0x710082e720), falling 1.225 px/f while gathering speed to the
    /// right, 1.1 to the left, and rising 0.025 while braking (0x710082e110, table
    /// 0x7101d03c18) — ±16 px swings, a 64-frame period, about 0.57 px/f down. The Move passes
    /// through terrain (its enter 0x7101347170 sets +0x524 bit 4) and has no ground case, so the
    /// leaf never lands: it sways on down through the floor [obs v3.0.3 0x7101346980]. A placed
    /// one starts in that Move where it stands (a new actor's base context is the Basic one,
    /// 0x7101298b90, whose enter 0x71013466f0 picks Move with no event) and sways down through
    /// the floor under it too [obs v3.0.3]. In water the push flips at |vx| ≥ 3 (0x71013472a0),
    /// and out of water again vx is held ≥ −2. Phases: 0 new, 1 shooting up, 2 swaying through
    /// terrain; push direction in `hops` bit 0; frames left of a downward pop in `timer`.
    /// NSMBU's Propeller Mushroom has its own Move (actors/propeller.rs).
    fn step_style_item(&mut self, level: &Level) {
        if level.style == "M1" {
            self.vy = (self.vy + ENEMY_GRAVITY).max(MAX_FALL);
            if self.on_ground {
                self.vx = ITEM_SPEED * self.dir as f32;
            }
            self.collide(level);
            if self.wall {
                self.dir = -self.dir;
                self.vx = ITEM_SPEED * self.dir as f32;
            }
            return;
        }
        if level.style == "3W" {
            self.vx = 0.0;
            self.vy = (self.vy + ENEMY_GRAVITY).max(MAX_FALL);
            self.collide(level);
            return;
        }
        // a Leaf / Feather / Propeller Mushroom passing through the floor is gone once it is 40
        // px below the view's bottom, at its next update (as a falling enemy) [obs: Ryujinx
        // traces X_leafplace, X_featherplace, X_propplace: placed ones sway / bob down through
        // the floor and freeze in the records at y −40.03 and −41.05 with the view's bottom at 0]
        if self.y < self.camera.1 - 40.0 {
            self.mode = Mode::Dead;
            return;
        }
        if level.style == "WU" {
            self.step_propeller(level);
            return;
        }
        match self.phase {
            0 => {
                // placed: the Move from here (enter: vx 0, push to the right)
                self.phase = 2;
                self.vx = 0.0;
                self.hops = 0;
                self.leaf_sway();
            }
            1 => {
                // shooting out of the block, through the terrain
                if self.timer > 0 {
                    self.vy = (self.vy - 0.0625).max(MAX_FALL);
                    self.timer -= 1;
                } else {
                    self.vy -= 0.25;
                }
                self.y += self.vy;
                let done = if self.timer > 0 {
                    false
                } else {
                    self.vy <= 0.0
                };
                if done {
                    self.phase = 2;
                    self.vx = 0.0;
                    self.hops = 0;
                }
            }
            _ => self.leaf_sway(),
        }
    }

    /// One frame of the ItemLeaf sway (Move exe 0x7101346980, mode 0, with the ItemLeaf slots
    /// 0x7101346d00 / 0x7101346d20 / 0x71013472a0), through terrain [obs v3.0.3].
    fn leaf_sway(&mut self) {
        let wet = self.wet();
        // out of the water this frame: the sideways speed is held at −2 or more
        if !wet && self.prop.wet_prev {
            self.vx = self.vx.max(-2.0);
        }
        self.prop.wet_prev = wet;
        let right = self.hops & 1 == 0;
        self.vx += if right { 0.125 } else { -0.125 };
        self.vy = match (u8::from(self.vx < 0.0)) + (self.hops & 1) {
            0 => -1.225,
            1 => 0.025,
            _ => -1.1,
        };
        if self.vx.abs() >= if wet { 3.0 } else { 2.0 } {
            self.hops ^= 1;
        }
        self.x += self.vx;
        self.y += self.vy;
        self.on_ground = false;
    }

    /// The Super Leaf / Cape Feather comes out of a block (instead of the Super Mushroom's slow
    /// rise): up at 4.0, or down at −0.24 for 9 frames.
    fn leaf_pop(&mut self, down: bool) {
        self.mode = Mode::Normal;
        self.phase = 1;
        self.vx = 0.0;
        if down {
            self.vy = -0.24;
            self.timer = 9;
        } else {
            self.vy = 4.0;
            self.timer = 0;
        }
    }

    fn step_track_block(&mut self, level: &Level, path: u8, alt: bool) {
        if self.trail.is_empty() {
            // path points: the start, then 2 tiles per node [inf: the 2×2 grid]
            let codes = level
                .track_paths
                .iter()
                .find(|(i, _)| *i == path)
                .map(|(_, c)| c.as_slice())
                .unwrap_or(&[]);
            let mut p = (self.x, self.y);
            self.trail.push(p);
            for &c in codes {
                let Some((dx, dy)) = crate::snake::node_step(c) else {
                    break;
                };
                p = (p.0 + dx * 2.0 * TILE, p.1 + dy * 2.0 * TILE);
                self.trail.push(p);
            }
            self.chain = 1;
            self.home_x = self.x;
            self.home_y = self.y;
        }
        let on = self.target_facing != 0;
        let last = self.trail.len() as u32 - 1;
        match self.phase {
            0 => {
                if self.ridden && on && last > 0 {
                    self.phase = 1;
                    self.chain = 1;
                }
            }
            1 | 2 if on => {
                // axis-aligned steps toward the next node, 0.5 px/f
                let (tx, ty) = self.trail[self.chain as usize];
                let step = |v: f32, t: f32| v + (t - v).clamp(-0.5, 0.5);
                if self.x != tx {
                    self.x = step(self.x, tx);
                } else {
                    self.y = step(self.y, ty);
                }
                if (self.x, self.y) == (tx, ty) {
                    let end = if self.phase == 1 {
                        self.chain == last
                    } else {
                        self.chain == 0
                    };
                    if end {
                        self.timer = 60;
                        self.phase = 3;
                    } else if self.phase == 1 {
                        self.chain += 1;
                    } else {
                        self.chain -= 1;
                    }
                }
            }
            3 => {
                self.timer -= 1;
                if self.timer <= 0 {
                    if alt {
                        self.phase = 4;
                        self.timer = 180;
                    } else {
                        // reverse
                        let back = self.chain == last;
                        self.phase = if back { 2 } else { 1 };
                        self.chain = if back { last - 1 } else { 1 };
                    }
                }
            }
            4 => {
                self.timer -= 1;
                if self.timer <= 0 {
                    self.phase = 5;
                    self.timer = 30;
                }
            }
            5 => {
                // back at the start once the spot is clear [inf: 30 frames]
                self.timer -= 1;
                if self.timer <= 0 {
                    self.x = self.home_x;
                    self.y = self.home_y;
                    self.phase = 6;
                    self.timer = 5;
                }
            }
            6 => {
                self.timer -= 1;
                if self.timer <= 0 {
                    self.phase = 0;
                    self.chain = 1;
                }
            }
            _ => {} // switched OFF: holds still
        }
    }

    /// Yoshi without a rider (yoshi.md §6): stands, slowing by 0.07/frame (gravity −0.34, max
    /// −4); after his rider was hit (phase 1) he runs at 2.0 (accel 0.1), turning at walls with a
    /// 10-frame lock, until remounted.
    fn step_yoshi(&mut self, level: &Level) {
        if self.phase == 1 {
            let target = 2.0 * self.dir as f32;
            self.vx += (target - self.vx).clamp(-0.1, 0.1);
        } else if self.on_ground {
            self.vx -= self.vx.clamp(-0.07, 0.07);
        } else {
            self.vx *= 0.98;
        }
        self.vy = (self.vy - 0.34).max(-4.0);
        self.collide(level);
        if self.wall && self.turn_lock == 0 {
            self.turn();
            self.turn_lock = 10;
            if self.phase != 1 {
                self.vx = -self.vx;
            }
        }
    }

    /// Fish Bone (103-fish-bone.md). `ridden` = in water (set by the world). Phases: 0 swim
    /// (0.5 px/f, 1.0 winged, ±64 px around home, turning at walls; vy decays by
    /// max(0.1·|vy|, 0.01)), 1 aim (40 frames, stopped), 2 charge (+0.25/frame to 2.0, 6.0 winged,
    /// along the aim; shatters on a wall, floor, ceiling or leaving the water), 3 fall (out of
    /// water, −0.1875). Detection: within 80 px (160 winged) on the facing side. Aim in `heading`.
    fn step_fish_bone(&mut self, level: &Level) {
        let (px, py) = self.target;
        let winged = self.wings;
        if self.age <= 1 {
            self.home_x = self.x;
        }
        match self.phase {
            0 => {
                if !self.ridden {
                    self.phase = 3;
                    return;
                }
                self.vx = if winged { 1.0 } else { 0.5 } * self.dir as f32;
                self.vy -= self.vy.signum() * (self.vy.abs() * 0.1).max(0.01).min(self.vy.abs());
                self.collide(level);
                let past = (self.x - self.home_x) * self.dir as f32 > 64.0;
                if (self.wall || past) && self.turn_lock == 0 {
                    self.turn();
                    self.turn_lock = 8;
                }
                let (dx, dy) = (px - self.x, py - self.y);
                let reach = if winged { 160.0 } else { 80.0 };
                let facing = dx * self.dir as f32 > 0.0;
                if facing && (dx * dx + dy * dy).sqrt() <= reach && (!winged || dy.abs() <= 80.0) {
                    self.phase = 1;
                    self.timer = 40;
                    self.heading = dy.datan2(dx).to_degrees();
                }
            }
            1 => {
                self.vx = 0.0;
                self.vy = 0.0;
                self.timer -= 1;
                if self.timer <= 0 {
                    self.phase = 2;
                    self.speed = 0.0;
                }
            }
            2 => {
                self.speed = (self.speed + 0.25).min(if winged { 6.0 } else { 2.0 });
                let (s, c) = self.heading.to_radians().dsin_cos();
                self.vx = self.speed * c;
                self.vy = self.speed * s;
                let up = self.vy > 0.0;
                self.collide(level);
                let ceiling = up && self.vy <= 0.0;
                if self.wall || ceiling || (self.on_ground && !up) || !self.ridden {
                    self.mode = Mode::Dead; // shatters
                }
            }
            _ => {
                self.vy = (self.vy - 0.1875).max(MAX_FALL);
                self.collide(level);
                if self.ridden {
                    self.phase = 0;
                }
            }
        }
    }

    /// Lava Bubble (60-lava-bubble.md). Hidden 120 frames 16 px below the reference level L, then
    /// rises with vy = A·cos θ, θ = 90°·(risen / (H·16)) changing ≤ 10°/frame, where H = (home −
    /// L + 16)/16 tiles and A = clamp(min(H·4/7, 8) + 1, 1, H + 1); within 20 px of the top it
    /// falls under gravity −0.1875 (≥ −6), upside down, until it is below the hide depth again.
    /// Phases: 0 hidden, 1 rising, 2 falling. θ (degrees) in `heading`.
    fn step_lava_bubble(&mut self) {
        let s = if self.big { 2.0 } else { 1.0 };
        let hide = self.floor_ref - 16.0 * s;
        match self.phase {
            0 => {
                if self.age <= 1 {
                    self.timer = 120;
                }
                self.y = hide;
                self.vy = 0.0;
                self.timer -= 1;
                if self.timer <= 0 {
                    self.phase = 1;
                    self.heading = 0.0;
                }
            }
            1 => {
                let h_tiles = ((self.home_y - self.floor_ref + 16.0) / 16.0).max(1.0);
                let a = ((h_tiles * 4.0 / 7.0).min(8.0) + 1.0)
                    .min(h_tiles + 1.0)
                    .max(1.0);
                let want = 90.0 * ((self.y - hide) / (h_tiles * 16.0)).clamp(0.0, 1.0);
                self.heading += (want - self.heading).clamp(-10.0, 10.0);
                self.vy = (a * self.heading.to_radians().dcos()).max(0.25);
                self.y += self.vy;
                if self.home_y - self.y < 20.0 {
                    self.phase = 2;
                }
            }
            2 => {
                self.vy = (self.vy - 0.1875).max(-6.0);
                self.y += self.vy;
                if self.y < hide {
                    self.phase = 0;
                    self.timer = 120;
                }
            }
            _ => {
                // launched from a pipe (actors/dokan.rs): flies on, falling, until it is
                // below its leap's hiding level
                self.vy = (self.vy - 0.1875).max(-6.0);
                self.x += self.vx;
                self.y += self.vy;
                if self.y < hide {
                    self.vx = 0.0;
                    self.phase = 0;
                    self.timer = 120;
                }
            }
        }
    }

    /// Monty Mole (102-monty-mole.md). Buried (phase 0) until the player is within 96 × 64 px,
    /// then a 60-frame mound (1), a leap out at vy 5.0 through the terrain (2), and a chase (3):
    /// toward the player at up to 1.7 px/f, accelerating 0.018 (0.03 when turning), re-aiming at
    /// most every 64 frames; walls bounce it back at 0.675× (≥ 0.5). Not buried → leaps at once.
    fn step_monty(&mut self, level: &Level) {
        let (px, py) = self.target;
        match self.phase {
            0 => {
                let covered = level.cell_at(self.x, self.y + 8.0).is_solid();
                let near = (px - self.x).abs() <= 96.0 && (py - (self.y - 1.0)).abs() <= 64.0;
                if !covered {
                    self.phase = 2;
                    self.vy = 5.0;
                } else if near {
                    self.phase = 1;
                    self.timer = 60;
                }
            }
            1 => {
                self.timer -= 1;
                if self.timer <= 0 {
                    self.phase = 2;
                    self.vy = 5.0;
                }
            }
            2 => {
                // through the terrain until clear of it, then normal physics
                self.vy = (self.vy - 0.1875).max(MAX_FALL);
                let inside = level.cell_at(self.x, self.y + 1.0).is_solid()
                    || level
                        .cell_at(self.x, self.y + self.height() - 1.0)
                        .is_solid();
                if inside && self.vy > 0.0 {
                    self.y += self.vy;
                } else {
                    self.collide(level);
                    if self.on_ground {
                        self.phase = 3;
                        self.dir = if px < self.x { -1 } else { 1 };
                        self.turn_lock = 64;
                    }
                }
            }
            _ => {
                if self.turn_lock == 0 {
                    let d = if px < self.x { -1 } else { 1 };
                    if d != self.dir {
                        self.dir = d;
                        self.turn_lock = 64;
                    }
                }
                if self.on_ground {
                    let target = 1.7 * self.dir as f32;
                    let acc = if self.vx * (self.dir as f32) < 0.0 {
                        0.03
                    } else {
                        0.018
                    };
                    self.vx += (target - self.vx).clamp(-acc, acc);
                    if self.wings && self.age.is_multiple_of(48) {
                        self.vy = 3.5625;
                    }
                }
                let vx = self.vx;
                self.vy = (self.vy + ENEMY_GRAVITY).max(MAX_FALL);
                self.collide(level);
                if self.wall {
                    let b = -0.675 * vx;
                    self.vx = if b.abs() < 0.5 { 0.5 * b.signum() } else { b };
                }
            }
        }
    }

    /// Angry Sun (104-angry-sun.md): hovers 48 px in from a top corner of the view and 64 px below
    /// its top edge, following the camera; waits 120 frames, spins one turn (circling 16 px around
    /// the anchor, 36 frames), then swoops along a half-ellipse 121 px deep to the other corner at
    /// 1.40°/frame. The Moon (alternate) only hangs in its corner. Phases: 0 wait, 1 spin, 2 swoop;
    /// side in `hops` (0 right, 1 left), angle in `heading`. The first wait is 300 frames (init
    /// writes the Wait timer, +0x600, before Appear; each swoop's end writes 120) [obs v3.0.3
    /// init 0x71013e4a30, Wait exe 0x71013e6320, Attack exe 0x71013e6840]; the Appear fly-in
    /// before it is not modelled.
    fn step_sun(&mut self, moon: bool) {
        use crate::world::{VIEW_H, VIEW_W};
        let (cx0, cy0) = self.camera;
        let side = if self.hops == 0 { 1.0 } else { -1.0 };
        let centre = cx0 + VIEW_W / 2.0;
        let r = VIEW_W / 2.0 - 48.0;
        let top = cy0 + VIEW_H - 64.0;
        let anchor = (centre + side * r, top);
        if self.age <= 1 {
            self.timer = 300;
        }
        if moon {
            self.x = anchor.0;
            self.y = anchor.1 - 16.0;
            return;
        }
        match self.phase {
            0 => {
                self.x = anchor.0;
                self.y = anchor.1 - 16.0;
                self.timer -= 1;
                if self.timer <= 0 {
                    self.phase = 1;
                    self.heading = 0.0;
                }
            }
            1 => {
                self.heading += 10.0;
                let a = self.heading.to_radians();
                self.x = anchor.0 + 16.0 * a.dsin();
                self.y = anchor.1 - 16.0 + 8.0 * (1.0 - a.dcos());
                if self.heading >= 360.0 {
                    self.phase = 2;
                    self.heading = 0.0;
                }
            }
            _ => {
                self.heading = (self.heading + 1.40).min(180.0);
                let a = self.heading.to_radians();
                self.x = centre + side * r * a.dcos();
                self.y = top - 16.0 - 121.0 * a.dsin().abs();
                self.dir = -side as i32;
                if self.heading >= 180.0 {
                    self.hops ^= 1;
                    self.phase = 0;
                    self.timer = 120;
                }
            }
        }
    }

    /// Chain Chomp (61-chain-chomp.md). Chained: hops around the stake (0.8 px/f, 1.5 hop on each
    /// landing, turns past 40·s px or at walls, pulled back past the 49.6·s chain), and after
    /// 120–179 frames crouches 61 frames, lunges at 8 px/f (+1/frame) to 51.2·s + 4 px from the
    /// stake, bites 61 frames and curves back onto the stake. 3 of 7 lunges aim at the player (elevation 73.1°,
    /// 45° or 18.3° by height); the rest are random. Free: hops at 1.2 px/f with 3.0 hops.
    /// Phases: 0 walk, 1 crouch, 2 lunge, 3 bite, 4 return. Attack counter in `hops`.
    fn step_chomp(&mut self, level: &Level, chained: bool) {
        let s = if self.big { 2.0 } else { 1.0 };
        let (px, py) = self.target;
        let stake = (self.home_x, self.home_y + 8.0 * s);
        if !chained {
            if self.on_ground {
                self.vx += (1.2 * self.dir as f32 - self.vx).clamp(-0.05, 0.05);
                self.vy = 3.0;
                self.on_ground = false;
            }
            self.vy = (self.vy - 0.1875).max(-4.0);
            self.collide(level);
            if self.wall {
                self.turn();
                self.vx = 1.2 * self.dir as f32;
            }
            return;
        }
        match self.phase {
            0 => {
                if self.timer <= 0 && self.age <= 1 {
                    self.timer = 120 + self.rand(60) as i32;
                }
                let away = (self.x - stake.0) * self.dir as f32 > 0.0;
                if self.turn_lock == 0 && away && (self.x - stake.0).abs() > 40.0 * s {
                    self.turn();
                    self.turn_lock = 10;
                }
                self.vx = 0.8 * self.dir as f32;
                if self.on_ground {
                    self.vy = 1.5;
                    self.on_ground = false;
                }
                self.vy = (self.vy - 0.1875).max(-4.0);
                // chain: pulled back past its slack length
                let (cx, cy) = (self.x, self.y + 8.0 * s);
                let d = ((cx - stake.0).powi(2) + (cy - stake.1).powi(2)).sqrt();
                let l = 49.6 * s;
                if d > l {
                    let k = (d - l) * 0.3 / d;
                    self.vx = (self.vx - (cx - stake.0) * k).clamp(-4.0, 4.0);
                    self.vy = (self.vy - (cy - stake.1) * k).max(-4.0);
                }
                self.collide(level);
                if self.wall && self.turn_lock == 0 {
                    self.turn();
                    self.turn_lock = 10;
                }
                self.timer -= 1;
                if self.timer <= 0 && self.on_ground {
                    self.phase = 1;
                    self.timer = 61;
                    self.vx = 0.0;
                    self.vy = 0.0;
                    self.hops = (self.hops + 1) % 7;
                    let aimed = matches!(self.hops, 0 | 3 | 5);
                    let (dir, deg) = if aimed {
                        let dy = py - self.y;
                        let deg = if dy > 36.0 {
                            73.1
                        } else if dy > 16.0 {
                            45.0
                        } else {
                            18.3
                        };
                        (if px < self.x { -1 } else { 1 }, deg)
                    } else {
                        let r = self.rand(8);
                        let deg = match r {
                            0 => 73.1,
                            1..=3 => 45.0,
                            _ => 18.3,
                        };
                        (if self.rand(2) == 0 { -1 } else { 1 }, deg)
                    };
                    self.dir = dir;
                    let reach = 51.2 * s + 4.0;
                    let (sn, cs) = (deg as f32).to_radians().dsin_cos();
                    // lunge target (centre) in `trail[0]`
                    self.trail = vec![(stake.0 + reach * cs * dir as f32, stake.1 + reach * sn)];
                }
            }
            1 => {
                self.timer -= 1;
                if self.timer <= 0 {
                    self.phase = 2;
                    self.speed = 0.0;
                }
            }
            2 => {
                let (tx, ty) = self.trail.first().copied().unwrap_or(stake);
                let (cx, cy) = (self.x, self.y + 8.0 * s);
                let (dx, dy) = (tx - cx, ty - cy);
                let d = (dx * dx + dy * dy).sqrt();
                self.speed = (self.speed + 1.0).min(8.0);
                if d <= self.speed {
                    self.x = tx;
                    self.y = ty - 8.0 * s;
                    self.phase = 3;
                    self.timer = 61;
                } else {
                    self.x += dx / d * self.speed;
                    self.y += dy / d * self.speed;
                }
            }
            3 => {
                self.timer -= 1;
                if self.timer <= 0 {
                    // AttackEnd: the curve's start, its unit direction to the stake and length
                    // (centres: the Chomp's and the stake's)
                    self.phase = 4;
                    self.heading = 0.0; // curve parameter
                    let c = (self.x, self.y + 8.0 * s);
                    let (dx, dy) = (stake.0 - c.0, stake.1 - c.1);
                    let d = dx.dhypot(dy);
                    let u = if d > 0.0 {
                        (dx / d, dy / d)
                    } else {
                        (0.0, 0.0)
                    };
                    self.trail = vec![c, u];
                    self.speed = d;
                }
            }
            _ => {
                // AttackEnd: back onto the stake along a cubic Bézier, P1 / P2 at 0.2 / 0.8 of the
                // way plus 16 px across it (the direction turned ±90°, toward the top), the
                // parameter stepping |1 − t|·0.1 clamped to 0.03…0.05; done within 0.01 of the
                // end [obs v3.0.3 exec 0x71012b56f0, turn table 0x7102319cc0; the target is the
                // stake's position, v1.0.0 0x71007b0350]. (v3.0.3's faster return behind a moving
                // stake in a mode flag's branch, and the halved steps in water, aren't modelled.)
                let (p0, u) = match self.trail[..] {
                    [p0, u, ..] => (p0, u),
                    _ => ((self.x, self.y + 8.0 * s), (0.0, 0.0)),
                };
                let d = self.speed;
                let t0 = self.heading;
                self.heading = (t0 + ((1.0 - t0).abs() * 0.1).clamp(0.03, 0.05)).min(1.0);
                let t = self.heading;
                let turn = if u.0 >= 0.0 { 0x4000_0000 } else { 0xc000_0000 };
                let (sn, cs) = crate::fmath::table_sin_cos(turn);
                let w = ((u.0 * cs - sn * u.1) * 16.0, (u.0 * sn + u.1 * cs) * 16.0);
                let p1 = (p0.0 + u.0 * d * 0.2 + w.0, p0.1 + u.1 * d * 0.2 + w.1);
                let p2 = (p0.0 + u.0 * d * 0.8 + w.0, p0.1 + u.1 * d * 0.8 + w.1);
                let r = 1.0 - t;
                let (b0, b1, b2, b3) = (r * r * r, 3.0 * t * r * r, 3.0 * r * t * t, t * t * t);
                self.x = b3 * stake.0 + b0 * p0.0 + b1 * p1.0 + b2 * p2.0;
                self.y = b3 * stake.1 + b0 * p0.1 + b1 * p1.1 + b2 * p2.1 - 8.0 * s;
                if r.abs() < 0.01 {
                    (self.x, self.y) = (stake.0, stake.1 - 8.0 * s);
                    self.phase = 0;
                    self.timer = 120 + self.rand(60) as i32;
                    self.vy = 0.0;
                    self.collide(level);
                }
            }
        }
    }

    /// Winged Piranha Plant / Muncher (2-piranha-plant.md): wait 72 frames, jump at 4.5
    /// (gravity −0.1875); once the speed drops to 0.8 hover 120 frames (vy = 0.8·sin φ, φ from
    /// 90° at +5°/frame), then fall back and wait again.
    fn step_winged_plant(&mut self, level: &Level) {
        match self.phase {
            0 => {
                self.vy = (self.vy + ENEMY_GRAVITY).max(MAX_FALL);
                self.vx = 0.0;
                self.collide(level);
                if self.on_ground {
                    self.timer += 1;
                    if self.timer >= 72 {
                        self.phase = 1;
                        self.vy = 4.5;
                        self.on_ground = false;
                    }
                }
            }
            1 => {
                self.vy += ENEMY_GRAVITY;
                self.collide(level);
                if self.vy <= 0.8 {
                    self.phase = 2;
                    self.timer = 0;
                }
            }
            _ => {
                self.vy = 0.8 * (90.0 + self.timer as f32 * 5.0).to_radians().dsin();
                self.y += self.vy;
                self.timer += 1;
                if self.timer >= 120 {
                    self.phase = 0;
                    self.timer = 0;
                    self.vy = 0.0;
                }
            }
        }
    }

    /// Generic winged flight for the enemies without a wing behaviour of their own here (each
    /// has its own EnemyDB Wing action: Teresa, Bubble, Pukupuku, …) [inf, from the winged
    /// Thwomp]: drift ±0.3 px/frame (turning at walls), bob vy = 0.15·sin(2°·t). The items,
    /// coins, map objects and blocks fly with the shared fly helper (actors/fly.rs).
    fn step_generic_wings(&mut self, level: &Level) {
        let t = 0.3 * self.dir as f32;
        self.vx += (t - self.vx).clamp(-0.003, 0.003);
        self.vy = 0.15 * (self.age as f32 * 2.0).to_radians().dsin();
        let before = self.x;
        self.collide(level);
        if self.wall || (self.x - before).abs() < 1e-4 && self.vx.abs() > 0.05 {
            self.dir = -self.dir;
        }
    }

    /// Paragoomba hops (small, small, big; 80-frame pause after the big hop); green Paratroopa
    /// hops 3.5625 on every landing; red Paratroopa bobs around its spawn height.
    fn step_wings(&mut self, level: &Level, player_x: f32) {
        match self.kind {
            Kind::Goomba => {
                self.vx = WALK_SPEED * self.dir as f32;
                if self.pause > 0 {
                    self.pause -= 1;
                }
                if self.on_ground && self.vy <= 0.0 {
                    let behind = (player_x - self.x).signum() as i32 != self.dir;
                    if self.timer > 64 && behind {
                        self.turn();
                        self.timer = 0;
                        self.hops = 0;
                        self.pause = 0;
                    } else if self.pause == 0 {
                        self.hops = (self.hops + 1) % 3;
                        if self.hops == 0 {
                            self.pause = 80;
                        }
                        self.vy = if self.hops == 0 { 3.0 } else { 1.25 };
                        self.on_ground = false;
                    }
                }
                self.timer += 1;
                self.vy = (self.vy + ENEMY_GRAVITY).max(MAX_FALL);
                self.collide(level);
                if self.wall && self.turn_lock == 0 {
                    self.turn();
                }
            }
            Kind::Koopa { red: false } => {
                self.vx = 0.6875 * self.dir as f32;
                if self.on_ground && self.vy <= 0.0 {
                    self.vy = 3.5625;
                    self.on_ground = false;
                }
                self.vy = (self.vy + ENEMY_GRAVITY).max(MAX_FALL);
                self.collide(level);
                if self.wall && self.turn_lock == 0 {
                    self.turn();
                }
            }
            Kind::Koopa { red: true } | Kind::DryBones => {
                // vertical speed eases toward ±0.5 by 0.01/frame between home−32 and home+48
                let above = self.y + 8.0 > self.home_y + 48.0;
                let below = self.y + 8.0 < self.home_y - 32.0;
                let target = if above {
                    -0.5
                } else if below || self.vy >= 0.0 {
                    0.5
                } else {
                    -0.5
                };
                self.vy = if self.vy < target {
                    (self.vy + 0.01).min(target)
                } else {
                    (self.vy - 0.01).max(target)
                };
                self.vx = 0.0;
                self.y += self.vy;
                if self.timer <= 0 {
                    self.dir = if player_x <= self.x { -1 } else { 1 };
                    self.timer = 10;
                }
                self.timer -= 1;
            }
            _ => {}
        }
    }

    /// Axis-separated movement against the tile grid and slopes. Sets on_ground and wall.
    pub fn collide(&mut self, level: &Level) {
        let (hw, h) = (self.half_w(), self.height());
        // a shell's sides are 6 px out and it stops flush against a wall [obs: Ryujinx trace
        // M1_koopa koopa_kick: x 538.0 against a wall at 544, 342.0 against one at 336]
        let shell = matches!(self.kind, Kind::Shell { .. });
        // a Basic walker rests 0.1 px inside its half width from a wall it presses (its BG
        // sensor) [obs: Ryujinx trace W_3051605, a Goomba at x 327.9 against a wall at 320]
        let basic = matches!(
            self.kind,
            Kind::Goomba
                | Kind::Goombrat
                | Kind::Koopa { .. }
                | Kind::Spiny
                | Kind::Buzzy
                | Kind::BobOmb
                | Kind::DryBones
        ) && !self.big
            && self.mode == Mode::Normal;
        // a big Goomba's Shoe meets walls 14 px out [obs: Ryujinx trace W_3145588, a big Stiletto
        // held at x 862.0 off a wall at 848]
        let big_shoe = matches!(self.kind, Kind::ShoeGoomba { .. }) && self.big;
        let (side_hw, gap) = if shell {
            (6.0, 0.0)
        } else if big_shoe {
            (14.0, 0.0)
        } else if basic {
            (hw, -0.1)
        } else if matches!(self.kind, Kind::Mushroom | Kind::OneUp) {
            // a walking mushroom rests 0.1 px into a wall, as a Basic walker does [obs: Ryujinx
            // traces W_3010660 / X_mushstart, mushrooms in one-tile boxes and against walls: x
            // 296.1 against a wall at 304]
            (hw, -0.1)
        } else {
            (hw, 0.001)
        };
        let was_ground = self.on_ground;
        let solid = |px: f32, py: f32| level.cell_at(px, py).is_solid();

        // horizontal: on a slope's incline the walk speed is along it (x · cos θ, the player's
        // rule) [obs: Ryujinx trace W_3051605, a Goomba at 0.5 on a steep slope: x −0.3535 a
        // frame]
        let cos = match level.slope_at(self.x) {
            // (the incline's ends inclusive: a Goomba at its foot's tile edge already walks it
            // [obs: W_3051605])
            Some((surf, sl))
                if was_ground
                    && self.x >= (sl.left + 1) as f32 * TILE
                    && self.x <= (sl.left + sl.width - 1) as f32 * TILE
                    && (surf - self.y).abs() < 1.0 =>
            {
                if sl.steep {
                    std::f32::consts::FRAC_1_SQRT_2
                } else {
                    0.894_400_9
                }
            }
            _ => 1.0,
        };
        self.x += self.vx * cos;
        if self.vx != 0.0 {
            let side = self.vx.signum();
            // (a mushroom's sensor reaches only as far as it rests into a wall: flush against a
            // wall's face doesn't meet it [obs: Ryujinx trace X_mushstart, a mushroom stepping
            // to x 200.0 by a wall at 208 walks on into it the next frame]; a Basic walker's
            // meets the face [obs: W_3051605])
            let reach = if matches!(self.kind, Kind::Mushroom | Kind::OneUp) {
                side_hw + gap
            } else {
                side_hw
            };
            let wx = self.x + side * reach;
            let ys = [self.y + 2.0, self.y + (h - 2.0).max(2.0)];
            // one-way walls block only motion into them from their blocking side
            let prev_wx = wx - self.vx * cos;
            let oneway = ys.iter().any(|&wy| {
                let c = level.cell_at(wx, wy);
                let cell_x = (wx / TILE).floor() * TILE;
                (side < 0.0 && c == Cell::PassRight && prev_wx >= cell_x + TILE - 0.01)
                    || (side > 0.0 && c == Cell::PassLeft && prev_wx <= cell_x + 0.01)
            });
            // a slope is walked onto, except where its body stands higher than a step over the
            // feet of an actor not yet on it (a slope set a row above the ground: its low cap is
            // a wall) [obs: Ryujinx trace W_3051605, Goombas turning at a steep slope's low cap
            // 16 px above the floor]
            let slope_wall = level.slope_at(wx).is_none_or(|(surf, _)| {
                surf > self.y + 4.0 && level.slope_at(self.x - self.vx * cos).is_none()
            });
            if (ys.iter().any(|&wy| solid(wx, wy)) || oneway) && slope_wall {
                let edge = if side > 0.0 {
                    (wx / TILE).floor() * TILE
                } else {
                    ((wx / TILE).floor() + 1.0) * TILE
                };
                self.x = edge - side * (side_hw + gap);
                self.wall = true;
            }
        }
        // (a shell slides on past the course's left edge [obs: Ryujinx trace M1_shell
        // shell_kick, x 5.0 → −14.5 at −3.25 a frame])
        if self.x - hw < level.left && !shell {
            self.x = level.left + hw;
            self.wall = self.vx < 0.0 || self.wall;
        }

        // vertical
        let prev_y = self.y;
        self.y += self.vy;
        // feet: 3 px in from the sides; a shell stands on its centre ±1 (it drops off a ledge
        // once its centre is 1 px past it) [obs: Ryujinx trace M1_koopa koopa_kick]
        // a mushroom too: off a block's edge once its centre is 2 px past it [obs: Ryujinx traces
        // M1_blocks blk_mush, M1_shellq]
        let fx = if matches!(self.kind, Kind::Shell { .. } | Kind::Mushroom | Kind::OneUp) {
            1.0
        } else if basic {
            4.0
        } else {
            (hw - 3.0).max(1.0)
        };
        if self.vy > 0.0 {
            let hy = self.y + h;
            if [-fx, fx].iter().any(|&dx| solid(self.x + dx, hy)) {
                self.y = (hy / TILE).floor() * TILE - h - 0.001;
                self.vy = match self.kind {
                    Kind::Mushroom | Kind::OneUp => -1.0,
                    Kind::Star => -2.0,
                    _ => 0.0,
                };
            }
            self.on_ground = false;
        } else {
            let mut landed: Option<f32> = None;
            // (a fall ending exactly on a tile's top lands on it [obs: Ryujinx trace W_3051605,
            // a Paragoomba's hop down to y 32.0 at −2.8125: vy 0 that frame])
            let py = if self.vy < 0.0 && self.y == (self.y / TILE).floor() * TILE {
                self.y - 0.001
            } else {
                self.y
            };
            for dx in [-fx, fx] {
                let px = self.x + dx;
                let top = ((py / TILE).floor() + 1.0) * TILE;
                // a slope's columns are probed at the centre only (below), unless a block (or a
                // one-way top: a cloud row high over a slope [obs: Ryujinx trace W_3049200])
                // stands above its surface (as the player's feet)
                if let Some((surf, _)) = level.slope_at(px)
                    && !(top > surf + 0.01
                        && matches!(level.cell_at(px, py), c if c.is_solid() || c == Cell::SemiTop))
                {
                    continue;
                }
                let mut cell = level.cell_at(px, py);
                // a foot exactly on a column boundary also stands on the column to its left, as
                // the player's do [obs: Ryujinx trace M1_blocks blk_mush, a mushroom at x 401
                // still on a block ending at 400]
                if !cell.is_solid() && px == (px / TILE).floor() * TILE {
                    let left = level.cell_at(px - 0.01, py);
                    if left.is_solid() {
                        cell = left;
                    }
                }
                if cell.is_solid() || (cell == Cell::SemiTop && prev_y >= top - 0.01) {
                    landed = Some(landed.map_or(top, |t: f32| t.max(top)));
                }
            }
            // (slopes are probed at the centre only, as the player's feet [obs: Ryujinx trace
            // W_3051605, a Goomba landing on a steep slope at the surface under its centre])
            if let Some((surf, _)) = level.slope_at(self.x) {
                let reach = if was_ground { 6.0 } else { 0.0 };
                if self.y <= surf + reach
                    && prev_y >= surf - 6.0
                    && landed.is_none_or(|t| surf >= t)
                {
                    landed = Some(surf);
                }
            }
            match landed {
                Some(top) => {
                    self.y = top;
                    self.vy = 0.0;
                    self.on_ground = true;
                }
                None => self.on_ground = false,
            }
        }
    }
}

/// Non-interacting effects ("simple objects").
#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    /// Coin popped from a block (B10): credited when its life ends.
    CoinPop {
        x: f32,
        y: f32,
        vy: f32,
        g: f32,
        cap: f32,
        life: i32,
    },
    /// Brick debris (B8): 4 per brick, 127 frames.
    Debris {
        x: f32,
        y: f32,
        vx: f32,
        vy: f32,
        life: i32,
    },
    /// Score popup rising above its source.
    Score {
        x: f32,
        y: f32,
        points: u32,
        life: i32,
    },
}

impl Effect {
    pub fn coin_pop(x: f32, block_y: f32, down: bool) -> Self {
        if down {
            Effect::CoinPop {
                x,
                y: block_y - 16.0,
                vy: 0.0,
                g: -0.0625,
                cap: -2.0,
                life: 28,
            }
        } else {
            Effect::CoinPop {
                x,
                y: block_y + 16.0,
                vy: 3.0,
                g: -0.1875,
                cap: -8.0,
                life: 28,
            }
        }
    }

    /// Four debris pieces for a brick broken at block origin (bx, by). `h`, `r` are the shared
    /// random offsets (B8: h ∈ {0} ∪ [0, 0.5), r ∈ [0, 0.2)).
    pub fn debris(bx: f32, by: f32, h: f32, r: f32) -> [Effect; 4] {
        let starts = [(-4.0, 8.0), (4.0, 8.0), (-4.0, 4.0), (4.0, 4.0)];
        let vxs = [-1.0, 1.0, -1.0, 1.0];
        let vys = [4.5, 4.5, 3.0, 3.0];
        std::array::from_fn(|i| Effect::Debris {
            x: bx + starts[i].0,
            y: by + starts[i].1,
            vx: vxs[i] + if vxs[i] > 0.0 { h } else { -h },
            vy: vys[i] + r,
            life: 127,
        })
    }

    /// Advance; returns false when finished.
    pub fn step(&mut self) -> bool {
        match self {
            Effect::CoinPop {
                y,
                vy,
                g,
                cap,
                life,
                ..
            } => {
                *vy = (*vy + *g).max(*cap);
                *y += *vy;
                *life -= 1;
                *life > 0
            }
            Effect::Debris {
                x, y, vx, vy, life, ..
            } => {
                *vy = (*vy - 0.1875).max(-4.0);
                *y += *vy;
                *x += *vx;
                *life -= 1;
                *life > 0
            }
            Effect::Score { y, life, .. } => {
                if *life > 20 {
                    *y += 1.0;
                }
                *life -= 1;
                *life > 0
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ground_actor(kind: Kind) -> (Level, Actor) {
        let level = Level::flat(100);
        let mut a = Actor::new(kind, 20.0 * TILE, 32.0);
        a.on_ground = true;
        a.dir = 1;
        (level, a)
    }

    #[test]
    fn a_chain_chomp_curves_back_onto_its_stake_over_the_top() {
        let level = Level::flat(40);
        let mut a = Actor::new(Kind::ChainChomp { chained: true }, 250.0, 70.0);
        (a.home_x, a.home_y) = (200.0, 32.0);
        a.phase = 3;
        a.timer = 1;
        let mut frames = 0;
        let mut top: f32 = 0.0;
        a.step(&level, 0.0);
        while a.phase == 4 && frames < 100 {
            a.step(&level, 0.0);
            top = top.max(a.y);
            frames += 1;
        }
        assert_eq!(a.phase, 0);
        assert!((20..=30).contains(&frames), "{frames}");
        assert!(
            (a.x - 200.0).abs() < 1e-3 && (a.y - 32.0).abs() < 0.5,
            "{} {}",
            a.x,
            a.y
        );
        // the control points sit 16 px above the straight path: it rises first
        assert!(top > 70.0, "{top}");
    }

    #[test]
    fn a_side_thwomp_sees_a_big_players_head_in_its_box() {
        let level = Level::flat(40);
        let mut a = Actor::new(
            Kind::Thwomp {
                side: true,
                right: true,
            },
            100.0,
            100.0,
        );
        // the box spans y 84…148; feet at 60: a small player (16 tall) is under it, a big one's
        // 32 px reach into it
        a.target = (150.0, 60.0);
        a.target_h = 16.0;
        assert!(!a.side_thwomp_sees(&level, true));
        a.target_h = 32.0;
        assert!(a.side_thwomp_sees(&level, true));
    }

    #[test]
    fn a_parachute_held_at_a_wall_swings_twice_as_fast_until_it_turns() {
        let mut level = Level::flat(100);
        for y in 2..20 {
            level.set(21, y, Cell::Solid);
        }
        // 2 px from the wall on its right, swinging right
        let mut a = Actor::new(Kind::Goomba, 21.0 * TILE - 8.0 - 2.0, 15.0 * TILE);
        a.mode = Mode::parachute(false);
        let mut held = 0;
        let mut xs = vec![];
        for _ in 0..80 {
            a.step(&level, 0.0);
            if let Mode::Parachute {
                held: true, rate, ..
            } = a.mode
            {
                assert_eq!(rate, 5.0);
                held += 1;
            }
            xs.push(a.x);
        }
        // φ from ≈ 3° to 90° at 5°/frame: about 18 frames against the wall
        assert!((15..22).contains(&held), "{held}");
        assert!(xs.iter().all(|&x| x <= 21.0 * TILE - 8.0 + 0.01));
        assert!(matches!(
            a.mode,
            Mode::Parachute {
                rate: 2.5,
                held: false,
                ..
            }
        ));
        // a left shot swings left first
        assert!(matches!(
            Mode::parachute(true),
            Mode::Parachute { rate: -2.5, .. }
        ));
    }

    #[test]
    fn goomba_walks_half_pixel_per_frame() {
        let (level, mut a) = ground_actor(Kind::Goomba);
        let x0 = a.x;
        for _ in 0..60 {
            a.step(&level, 0.0);
        }
        assert!((a.x - x0 - 30.0).abs() < 1e-3, "moved {}", a.x - x0);
        assert!(a.on_ground);
    }

    #[test]
    fn goomba_turns_at_wall() {
        let mut level = Level::flat(100);
        level.set_solid(25, 2);
        let mut a = Actor::new(Kind::Goomba, 22.0 * TILE, 32.0);
        a.on_ground = true;
        for _ in 0..200 {
            a.step(&level, 0.0);
        }
        assert_eq!(a.dir, -1);
        assert!(a.x < 25.0 * TILE);
    }

    #[test]
    fn red_koopa_turns_at_ledge_green_walks_off() {
        let level = Level::flat(30); // ground ends at x = 480
        let mut red = Actor::new(Kind::Koopa { red: true }, 28.0 * TILE, 32.0);
        red.on_ground = true;
        let mut green = red.clone();
        green.kind = Kind::Koopa { red: false };
        for _ in 0..200 {
            red.step(&level, 0.0);
            green.step(&level, 0.0);
        }
        assert_eq!(red.dir, -1);
        assert!(red.on_ground);
        assert!(!green.on_ground || green.mode == Mode::Dead);
    }

    #[test]
    fn a_red_koopas_ledge_probe_on_the_cell_edge_still_stands() {
        // ground ends at x 480: the foot sensor's end 4 px ahead turns it once past 480 (a
        // twentieth of a pixel is enough), not when exactly on it
        let level = Level::flat(30);
        let probe = |x: f32| {
            let mut a = Actor::new(Kind::Koopa { red: true }, x, 32.0);
            a.on_ground = true;
            a.dir = 1;
            a.step(&level, 0.0);
            (a.x, a.dir)
        };
        let (x, dir) = probe(476.0);
        assert!(x + 4.0 > 480.0 && x + 4.0 < 480.6, "{x}");
        assert_eq!(dir, -1);
        let step = probe(476.0).0 - 476.0;
        let (x, dir) = probe(476.0 - step);
        assert_eq!(x, 476.0);
        assert_eq!(dir, 1, "on the edge it stands");
        let (x, dir) = probe(476.05 - step);
        assert!((x - 476.05).abs() < 1e-3);
        assert_eq!(dir, -1);
    }

    #[test]
    fn stomped_goomba_lasts_30_frames() {
        let (level, mut a) = ground_actor(Kind::Goomba);
        assert!(a.stomp());
        for i in 0..30 {
            assert_eq!(a.mode, Mode::Stomped, "frame {i}");
            a.step(&level, 0.0);
        }
        assert_eq!(a.mode, Mode::Dead);
    }

    #[test]
    fn koopa_shell_wakes_after_511() {
        let (level, mut a) = ground_actor(Kind::Koopa { red: false });
        a.stomp();
        assert_eq!(a.kind, Kind::Shell { red: false });
        for _ in 0..510 {
            a.step(&level, 0.0);
        }
        assert_eq!(a.mode, Mode::ShellWait);
        a.step(&level, 0.0);
        assert_eq!(a.kind, Kind::Koopa { red: false });
    }

    #[test]
    fn kicked_shell_keeps_speed_and_bounces() {
        let mut level = Level::flat(100);
        level.set_solid(30, 2);
        let mut a = Actor::new(Kind::Shell { red: false }, 20.0 * TILE, 32.0);
        a.on_ground = true;
        a.mode = Mode::ShellWait;
        a.kick(1, 1.5);
        assert!((a.vx - 3.25).abs() < 1e-6);
        let mut reversed = false;
        for _ in 0..120 {
            a.step(&level, 0.0);
            if a.vx < 0.0 {
                reversed = true;
            }
        }
        assert!(reversed);
        assert!((a.vx.abs() - 3.25).abs() < 1e-6);
    }

    #[test]
    fn mushroom_emerges_in_32_frames_then_walks() {
        let level = Level::flat(100);
        let mut a = Actor::new(Kind::Mushroom, 20.0 * TILE + 8.0, 4.0 * TILE);
        a.vy = 0.5;
        a.mode = Mode::Emerge { until: 5.0 * TILE };
        let mut n = 0;
        while matches!(a.mode, Mode::Emerge { .. }) {
            a.step(&level, 0.0);
            n += 1;
        }
        assert_eq!(n, 32);
        // it stands on the frame it comes out and walks from the next
        assert_eq!(a.vx, 0.0);
        a.step(&level, 0.0);
        assert_eq!(a.vx, ITEM_SPEED);
    }

    #[test]
    fn coin_pop_peaks_near_38_5() {
        let mut e = Effect::coin_pop(0.0, 0.0, false);
        let mut top: f32 = 0.0;
        let mut n = 0;
        while e.step() {
            n += 1;
            if let Effect::CoinPop { y, .. } = e {
                top = top.max(y);
            }
        }
        assert_eq!(n + 1, 28);
        assert!((top - 38.5).abs() < 0.2, "peak {top}");
    }

    #[test]
    fn skewer_cycle() {
        let level = Level::flat(40);
        let mut s = Actor::new(Kind::Skewer { dir: 0 }, 200.0, 100.0);
        let mut max_ext: f32 = 0.0;
        let mut first_full = None;
        for f in 0..400 {
            s.step(&level, 0.0);
            max_ext = max_ext.max(s.heading);
            if s.heading >= 144.0 && first_full.is_none() {
                first_full = Some(f);
            }
        }
        assert_eq!(max_ext, 144.0);
        // 100 wait + wind-back (~25) + 40 hold + thrust (~30)
        let f = first_full.unwrap();
        assert!((180..=210).contains(&f), "extended at frame {f}");
        assert!(s.heading <= 144.0);
    }

    #[test]
    fn spike_top_circles_a_floating_block() {
        // one block at tile (20, 6): px 320..336 × 96..112
        let mut level = Level::flat(60);
        level.set_solid(20, 6);
        let mut a = Actor::new(Kind::SpikeTop { surface: 3 }, 326.0, 112.0);
        a.on_ground = true;
        a.dir = 1;
        let mut seen = [false; 4];
        let mut corner_frames = 0;
        for _ in 0..1200 {
            a.step(&level, 0.0);
            assert_eq!(a.mode, Mode::Normal);
            // the body stays wrapped round the block
            let (cx, cy) = (a.x, a.y + 8.0);
            assert!(
                (300.0..=356.0).contains(&cx) && (76.0..=132.0).contains(&cy),
                "{cx} {cy}"
            );
            if matches!(a.phase, 2 | 3) {
                corner_frames += 1;
            }
            if a.phase <= 1 {
                seen[(a.heading as i32 / 90) as usize & 3] = true;
            }
        }
        assert_eq!(seen, [true; 4]);
        assert!(corner_frames > 0);
    }

    #[test]
    fn spike_top_corner_takes_31_frames_and_climbs_walls() {
        let mut level = Level::flat(60);
        for y in 2..8 {
            level.set_solid(30, y);
        }
        let mut a = Actor::new(Kind::SpikeTop { surface: 3 }, 28.0 * TILE, 32.0);
        a.on_ground = true;
        a.dir = 1;
        let mut turning = 0;
        let mut climbed = false;
        for _ in 0..400 {
            a.step(&level, 0.0);
            if a.phase == 2 {
                turning += 1;
            }
            if a.phase == 1 && a.heading == 90.0 && a.y > 48.0 {
                climbed = true;
                break;
            }
        }
        assert_eq!(turning, 31); // the corner frame, then 30 turning; the 31st settles
        assert!(climbed);
        assert!(a.x + 8.0 <= 30.0 * TILE + 0.01);
    }

    /// Off a ledge (tiles 17..=20 × 3..=6: top 112, right face 336), crawling right.
    fn spike_top_at_a_ledge(big: bool) -> (Level, Actor) {
        let mut level = Level::flat(60);
        for x in 17..=20 {
            for y in 3..=6 {
                level.set_solid(x, y);
            }
        }
        let mut a = Actor::new(Kind::SpikeTop { surface: 3 }, 316.0, 112.0);
        a.on_ground = true;
        a.dir = 1;
        a.big = big;
        (level, a)
    }

    #[test]
    fn spike_top_rounds_a_ledge_on_the_binarys_quarter_circle() {
        for (big, h, drop) in [(false, 8.0, 6.056), (true, 16.0, 12.825)] {
            let (level, mut a) = spike_top_at_a_ledge(big);
            while a.phase != 3 {
                a.step(&level, 0.0);
            }
            // the centre just past the edge, where the walker left it
            let x0 = a.x;
            assert!((336.0..336.5).contains(&x0), "{x0}");
            let mut frames = 0;
            let mut prev = (a.x, a.y);
            while a.phase == 3 {
                a.step(&level, 0.0);
                frames += 1;
                // 0.34 px/f (big 0.72) a frame
                let step = (a.x - prev.0).dhypot(a.y - prev.1);
                if a.phase == 3 {
                    let v = if big { 0.72 } else { 0.34 };
                    assert!((step - v).abs() < 1e-3, "{step}");
                }
                prev = (a.x, a.y);
            }
            assert_eq!(frames, 31);
            assert_eq!((a.phase, a.heading), (1, 270.0));
            // flush with the face, 6.06 px (big 12.8) below where it started, not h
            assert_eq!(a.x, 336.0 + h);
            assert!((a.y - (112.0 - drop)).abs() < 0.01, "{}", a.y);
            // and it walks down the face
            for _ in 0..20 {
                a.step(&level, 0.0);
            }
            assert_eq!(a.phase, 1);
            assert!(a.y < 112.0 - drop - 5.0);
        }
    }

    #[test]
    fn spike_top_drops_when_the_corner_goes_mid_turn() {
        let (mut level, mut a) = spike_top_at_a_ledge(false);
        while a.phase != 3 {
            a.step(&level, 0.0);
        }
        for _ in 0..10 {
            a.step(&level, 0.0);
        }
        assert_eq!(a.phase, 3);
        // the probe finds no ground under its feet: InAir
        for x in 17..=20 {
            level.set(x, 6, Cell::Empty);
        }
        a.step(&level, 0.0);
        assert_eq!(a.phase, 0);
        assert!(!a.on_ground);
        let y = a.y;
        for _ in 0..5 {
            a.step(&level, 0.0);
        }
        assert!(a.y < y);
    }

    #[test]
    fn spike_top_turns_at_half_rate_and_speed_in_water() {
        let (mut level, mut a) = spike_top_at_a_ledge(false);
        level.theme = 4;
        while a.phase != 3 {
            a.step(&level, 0.0);
        }
        let mut frames = 0;
        while a.phase == 3 {
            a.step(&level, 0.0);
            frames += 1;
        }
        assert_eq!(frames, 61);
        assert_eq!((a.phase, a.x), (1, 344.0));
        assert!((a.y - (112.0 - 5.98)).abs() < 0.01, "{}", a.y);
    }

    #[test]
    fn spike_top_placed_on_a_ceiling_crawls_along_it() {
        let mut level = Level::flat(60);
        for x in 10..40 {
            level.set_solid(x, 10);
        }
        // under the ceiling: body in tile row 9 (px 144..160)
        let mut a = Actor::new(Kind::SpikeTop { surface: 2 }, 20.0 * TILE + 8.0, 144.0);
        a.dir = 1;
        for _ in 0..100 {
            a.step(&level, 0.0);
        }
        assert_eq!(a.phase, 1);
        assert_eq!(a.heading, 180.0);
        assert!((a.y - 144.0).abs() < 0.01);
        assert!(
            a.x > 20.0 * TILE + 8.0 + 40.0,
            "moves on in its screen direction: {}",
            a.x
        );
    }

    #[test]
    fn side_thwomp_charges_only_the_way_it_looks() {
        let mut level = Level::flat(60);
        for y in 2..12 {
            level.set_solid(10, y);
        }
        let mut t = Actor::new(
            Kind::Thwomp {
                side: true,
                right: false,
            },
            30.0 * TILE,
            64.0,
        );
        // a player level with it but behind it: nothing
        t.target = (t.x + 60.0, 64.0);
        for _ in 0..30 {
            t.step_spawning(&level);
        }
        assert_eq!(t.phase, 0);
        // in front, 130 px away: it charges left at up to 3.0 and stops at the wall
        t.target = (t.x - 130.0, 70.0);
        t.step_spawning(&level);
        assert_eq!(t.phase, 1);
        let x0 = t.x;
        let mut top: f32 = 0.0;
        for _ in 0..200 {
            t.step_spawning(&level);
            top = top.max(t.vx.abs());
            if t.phase == 2 {
                break;
            }
        }
        assert_eq!(t.phase, 2);
        assert!((top - 3.0).abs() < 1e-4);
        assert!(
            (t.x - 16.0 - 11.0 * TILE).abs() < 0.01,
            "against the wall: {}",
            t.x
        );
        // 64 frames, then back to where it started
        t.target = (x0 + 60.0, 64.0);
        for _ in 0..64 + 400 {
            t.step_spawning(&level);
        }
        assert_eq!(t.phase, 0);
        assert_eq!(t.x, x0);
    }

    #[test]
    fn lakitu_throws_for_ever_but_holds_at_ten_live_spinies() {
        let level = Level::flat(60);
        let mut l = Actor::new(Kind::Lakitu, 200.0, 160.0);
        l.id = 7;
        l.target = (200.0, 32.0);
        let mut thrown = 0;
        for _ in 0..3000 {
            if let Some(egg) = l.step_spawning(&level) {
                assert_eq!(egg.parent, Some(7));
                thrown += 1;
            }
        }
        assert!(thrown > 10, "no lifetime budget: {thrown}");
        l.children = 10;
        for _ in 0..600 {
            assert!(l.step_spawning(&level).is_none());
        }
        l.children = 9;
        assert!((0..300).any(|_| l.step_spawning(&level).is_some()));
    }

    #[test]
    fn super_leaf_shoots_out_then_sways_down() {
        let mut level = Level::flat(60);
        level.style = "M3".into();
        // out of a block at tile (20, 6)
        let mut a = Actor::new(Kind::StyleItem, 20.0 * TILE + 8.0, 96.0);
        a.vy = 0.5;
        a.mode = Mode::Emerge { until: 112.0 };
        let mut top: f32 = 0.0;
        let (mut xmin, mut xmax) = (f32::MAX, f32::MIN);
        for _ in 0..400 {
            a.step(&level, 0.0);
            top = top.max(a.y);
            if a.phase == 2 {
                xmin = xmin.min(a.x);
                xmax = xmax.max(a.x);
                assert!(a.vx.abs() <= 2.0 + 1e-4);
            }
        }
        // 4.0 under −0.25: 30 px up (16 frames), then ±16 px swings down, through the floor
        // (the block too) at ~0.57 px/f
        assert!((top - 126.0).abs() < 0.01, "peak {top}");
        assert!((xmax - xmin - 32.0).abs() < 1.0, "swing {}", xmax - xmin);
        assert_eq!(a.phase, 2);
        // gone once 40 px under the view's bottom (at 0 here)
        assert!(
            a.mode == Mode::Dead && (a.y + 40.5248).abs() < 0.01,
            "y {}",
            a.y
        );
    }

    #[test]
    fn a_placed_super_leaf_sways_down_through_the_floor() {
        let mut level = Level::flat(60);
        level.style = "MW".into();
        let mut a = Actor::new(Kind::StyleItem, 200.0, 80.0);
        for _ in 0..200 {
            a.step(&level, 0.0);
        }
        // ~0.57 px/f down: past the ground (top 32) and on, never landing
        assert_eq!(a.phase, 2);
        assert!(a.y < 0.0 && !a.on_ground, "y {}", a.y);
    }

    #[test]
    fn super_bell_and_hammer_wait_where_they_land() {
        let mut level = Level::flat(60);
        level.style = "3W".into();
        for kind in [Kind::StyleItem, Kind::SuperHammer] {
            let mut a = Actor::new(kind, 200.0, 64.0);
            for _ in 0..120 {
                a.step(&level, 0.0);
            }
            assert_eq!((a.x, a.y), (200.0, 32.0), "{kind:?}");
        }
    }

    fn bro_on_platform_level() -> Level {
        // ground (top y 32) with a one-tile solid platform at tile row 5 (top y 96, 4 tiles up)
        let mut level = Level::flat(60);
        for x in 14..27 {
            level.set_solid(x, 5);
        }
        level
    }

    #[test]
    fn hammer_bro_jumps_up_through_a_platform_and_lands_on_it() {
        let level = bro_on_platform_level();
        let mut b = Actor::new(Kind::HammerBro, 20.0 * TILE + 8.0, 32.0);
        b.on_ground = true;
        b.pause = 1; // jump now; the ground is too thick to drop through, so up
        b.timer = 10_000;
        let mut through = false;
        for _ in 0..200 {
            b.step_spawning(&level);
            through |= b.y > 80.0 && b.y < 96.0;
        }
        assert!(through, "passes through the platform");
        assert_eq!(b.y, 96.0);
        assert!(b.on_ground);
        assert_eq!(b.hops, 0);
    }

    #[test]
    fn hammer_bro_near_the_view_top_drops_through_the_floor() {
        let level = bro_on_platform_level();
        let mut b = Actor::new(Kind::HammerBro, 20.0 * TILE + 8.0, 96.0);
        b.on_ground = true;
        b.pause = 1;
        b.timer = 10_000;
        // the view top is 4 px above its feet: the jump must go down
        b.camera = (0.0, 100.0 - crate::world::VIEW_H);
        for _ in 0..300 {
            b.step_spawning(&level);
        }
        assert_eq!(b.y, 32.0);
        assert!(b.on_ground);
    }

    /// A Sledge Bro on flat ground about to jump, with the throw timer out of the way.
    fn sledge(wings: bool) -> (Level, Actor) {
        let level = Level::flat(60);
        let mut b = Actor::new(Kind::HammerBro, 20.0 * TILE + 8.0, 32.0);
        b.alt = true;
        b.wings = wings;
        b.on_ground = true;
        b.home_x = b.x;
        b.pause = 1;
        b.timer = 10_000;
        (level, b)
    }

    /// Frames spent hovering, and how far it sank in the hover.
    fn sledge_hover(level: &Level, b: &mut Actor) -> (usize, f32) {
        let (mut frames, mut top, mut low) = (0, None, f32::MAX);
        for _ in 0..200 {
            b.step_spawning(level);
            if b.heading > 0.0 {
                frames += 1;
                top.get_or_insert(b.y);
                low = low.min(b.y);
            }
            if b.kick_lock > 0 {
                break;
            }
        }
        (frames, top.unwrap_or(0.0) - low)
    }

    #[test]
    fn a_sledge_bro_hovers_nine_frames_then_hip_drops() {
        let (level, mut b) = sledge(false);
        let (frames, sank) = sledge_hover(&level, &mut b);
        // the setting frame and 9 held ones
        assert_eq!(frames, 10);
        assert_eq!(sank, 0.0);
        assert_eq!(b.y, 32.0);
        assert_eq!(b.kick_lock, 50, "quakes on landing");
    }

    #[test]
    fn a_winged_sledge_bro_keeps_its_action_and_bobs_in_the_hover() {
        let (level, mut b) = sledge(true);
        b.step_spawning(&level);
        assert!(!b.bro_wing.on, "no WingWalk");
        assert_eq!(b.hops, 2);
        assert!(b.vy > 6.0, "the winged jump 6.5");
        let (frames, sank) = sledge_hover(&level, &mut b);
        assert_eq!(frames, 10);
        // 0.4 · cos(90° + 8°k) for k = 0..8: about 1.8 px down
        assert!((1.7..1.9).contains(&sank), "{sank}");
        // and its helper throws on its timer as a wingless one does
        let (level, mut b) = sledge(true);
        b.timer = 0;
        assert!(b.step_spawning(&level).is_some());
    }

    #[test]
    fn fireball_bounces_and_dies_at_wall() {
        let mut level = Level::flat(100);
        level.set_solid(26, 2);
        let mut f = Actor::new(Kind::Fireball, 20.0 * TILE, 40.0);
        f.vx = FIREBALL_SPEED;
        f.vy = -3.0;
        f.timer = FIREBALL_LIFE;
        let mut bounced = false;
        for _ in 0..60 {
            f.step(&level, 0.0);
            if f.vy > 3.0 {
                bounced = true;
            }
            if f.mode == Mode::Dead {
                break;
            }
        }
        assert!(bounced);
        assert_eq!(f.mode, Mode::Dead);
    }
}
