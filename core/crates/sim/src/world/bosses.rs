//! World-side boss rules: the player touching a boss or its shots, shells hitting bosses, the
//! Key a boss drops on defeat, and the Clown Car drawn under a rider. The bosses themselves are
//! in actors/bosses.rs; HP always changes through `Actor::boss_hit`.

use super::{Event, SCORE_TABLE, World, overlap};
use crate::actors::{Actor, BossAttack, BossHit, Mode};
use crate::render::Sprite;

impl World {
    /// The player touching a boss (or a Bowser flame / Pom Pom shuriken) at `i`. A Super Star
    /// knocks any boss off (DieFall). Boom Boom and Bowser Jr. take stomps (1 HP; guarded while
    /// stunned or spinning); Bowser cannot be stomped. Contact from above never hurts the player
    /// on a stompable boss (so the bounce out of a stomp is safe); otherwise the touch hurts.
    /// Returns true when the player was hurt (the caller stops checking this frame).
    pub(super) fn boss_contact(&mut self, i: usize, jump_held: bool) -> bool {
        let hurt = self.player.hurt_box();
        let a = &self.actors[i];
        if !overlap(hurt, a.bbox()) {
            return false;
        }
        let star = self.player.star > 0;
        let boss = a.kind.is_boss();
        let (ax, ay, ah) = (a.x, a.y, a.height());
        if star {
            if boss {
                let px = self.player.x;
                self.actors[i].die_fall(px);
                let n = self.player.stomp_chain;
                self.player.stomp_chain += 1;
                self.add_score(Self::chain_score(n), ax, ay + 24.0);
                self.events.push(Event::EnemyDown);
            }
            return false;
        }
        let p = &self.player;
        let above = p.y >= ay + ah * 0.5;
        if boss && above && !a.kind.unstompable() {
            if p.vy < 0.0 {
                let px = p.x;
                let hit = self.actors[i].boss_hit(BossAttack::Stomp, px);
                self.player.bounce(jump_held);
                match hit {
                    BossHit::Defeated => {
                        let n = self.player.stomp_chain;
                        self.player.stomp_chain += 1;
                        self.add_score(Self::chain_score(n), ax, ay + 24.0);
                        self.events.push(Event::Stomp);
                        self.events.push(Event::EnemyDown);
                    }
                    BossHit::Hurt => self.events.push(Event::Stomp),
                    _ => self.events.push(Event::Bump),
                }
            }
            return false;
        }
        if self.player.hurt() {
            self.events.push(if self.player.alive() {
                Event::PowerDown
            } else {
                Event::Death
            });
            return true;
        }
        false
    }

    /// A boss defeated while holding a Key (child object 95) hands it over at once: the boss
    /// helper (0x71006116d0, HP < 1) marks the Key collected and queues a key event of kind 1
    /// for the attacking player at its hitbox centre − 8 px — the queue the Key's own touch
    /// uses (area +0xe8, 0x7100896ab0). The area update then counts the key for the player
    /// and launches the flying key (Appear → Waft → Chase into the trail), or, with no lock
    /// left to open, a Drop key that is not counted (95-key.md). The engine counts it at once,
    /// as for a touched Key [the flying key is not drawn: the engine has no such actor].
    pub(super) fn boss_keys(&mut self) {
        for a in &mut self.actors {
            if a.kind.is_boss() && matches!(a.mode, Mode::DieFall | Mode::Dead) && a.take_boss_key()
            {
                self.keys += 1;
                self.events.push(Event::OneUp);
            }
        }
    }

    /// Koopa Clown Cars under the bosses riding them, animated as the car's wait_ftp (4 textures,
    /// 2 frames each). The car places its rider (0x71006d6ca0) at the car's origin plus the
    /// rider's ride height (`Actor::clown_ride_height`), so the car is drawn that far below
    /// the boss; the engine merges car and rider (the game links two actors) and takes the car
    /// as big when the rider is [inf: the car's own size comes from its placement]; a Fire car
    /// (the boss carries its fire record) is drawn as one.
    pub(super) fn clown_sprites(&self, out: &mut Vec<Sprite>) {
        for a in &self.actors {
            if !a.kind.is_boss() || !a.kind.boss_clown() || a.mode != Mode::Normal {
                continue;
            }
            let s = if a.big { 2.0 } else { 1.0 };
            let tex = ["wait.0", "wait.1", "wait.2", "wait.3"][(self.tick as usize / 2) % 4];
            out.push(Self::model_sprite(
                if a.clown_fire.is_some() {
                    "M1_Enemy_koopaclownfire"
                } else {
                    "M1_Enemy_koopaclown"
                },
                tex,
                a.x - 16.0 * s,
                a.y - a.clown_ride_height(&self.level.style),
                32.0 * s,
                32.0 * s,
                a.dir > 0,
                41,
            ));
        }
    }
}

/// A sliding shell meeting a boss: the boss takes the shell attack (attack 6; a big shell's
/// costs Bowser 5) and the shell slides on through him [obs v3.0.3]. Both colliders' callbacks
/// run for an overlapping pair (0x7100e28af0) and send nothing back to the shell:
/// - the shell's own (0x71011ff860): the bosses' colliders do carry +0x91 bit 0 (each frame's
///   update 0x7101208390 sets it from the actor's +0x534 bit 15, set by the Bowser, Boom Boom
///   and Bowser Jr ctors 0x7101147650, 0x71010a1860 / 0x71010a7ad0 / 0x71010af760,
///   0x7101193fc0), but its general branch (LAB_7101201db4) knocks the shell out (its own
///   reaction 0, kind 6) only for an attacker that isn't an enemy: 0x71012a55d0 holds for the
///   enemy class 0x7102853080 that a Koopa or Buzzy shell is, and leaves it the shell-against-
///   shell rule (both +0x2b8 = 3), which a boss isn't. The "blocker" branch (LAB_7101202450)
///   needs the shell's +0x534 bit 19 or 20, which nothing in v3.0.3 sets (pass 7). No boss
///   attacks with its body (+0x78 = 0), so the shell takes nothing either, and ShellSlide
///   (0x710105b390) turns back only at a BG wall (+0x658 & 0x24000 = 0x4000), not on the
///   walker's turn flag (+0x528 bit 27);
/// - the boss's (the same callback; 3D World's Boom Boom through its wrapper 0x71010a1c30):
///   the shell attack goes to its reaction (EnemyDB `shell` = ReactOnly for
///   Bowser) and the damage helper (Bowser 0x710115dcc0 → 0x710115e350: HP, the hurt effect,
///   the score), none of which answers the attacker (0x710123a500, 0x710123b1c0).
///
/// [inf: the shell hits a boss once while they touch — nothing found in the damage path keeps
/// a sliding shell from hitting Bowser again on the next overlapping frame (Boom Boom and Bowser
/// Jr. are guarded in their damage states).] Returns None when neither is a boss hit by a
/// sliding shell; otherwise the score to award, if the boss was defeated.
pub(super) fn boss_shell_hit(a: &mut Actor, b: &mut Actor) -> Option<Option<(u32, f32, f32)>> {
    let (shell, boss) = if a.mode == Mode::ShellSlide && b.kind.is_boss() {
        (a, b)
    } else if b.mode == Mode::ShellSlide && a.kind.is_boss() {
        (b, a)
    } else {
        return None;
    };
    if shell.boss_contact == Some(boss.id) {
        return Some(None);
    }
    shell.boss_contact = Some(boss.id);
    let attack = if shell.big {
        BossAttack::BigShell
    } else {
        BossAttack::Shell
    };
    let hit = boss.boss_hit(attack, shell.x);
    Some((hit == BossHit::Defeated).then_some((SCORE_TABLE[4], boss.x, boss.y + 24.0)))
}

/// A shell's boss contact ends when they no longer overlap (or either is gone).
pub(super) fn end_boss_contacts(actors: &mut [Actor]) {
    for i in 0..actors.len() {
        let Some(id) = actors[i].boss_contact else {
            continue;
        };
        let still = actors.iter().any(|b| {
            b.id == id
                && b.alive()
                && actors[i].mode == Mode::ShellSlide
                && super::overlap(actors[i].bbox(), b.bbox())
        });
        if !still {
            actors[i].boss_contact = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::*;
    use crate::actors::{BossAttack, BossHit};

    const BOWSER: Kind = Kind::Bowser {
        key: false,
        clown: false,
    };
    const BOOM_BOOM: Kind = Kind::BoomBoom {
        key: false,
        clown: false,
        pom: false,
    };
    const BOWSER_JR: Kind = Kind::BowserJr {
        key: false,
        clown: false,
    };

    fn spawn(kind: Kind, x: f32, big: bool) -> Spawn {
        Spawn {
            kind,
            x,
            y: 32.0,
            big,
            wings: false,
            pipe: None,
            parachute: false,
            rail: None,
            link: -1,
            clown: false,
            alt: false,
        }
    }

    /// A flat course with one boss at x = 240, the player parked far left (and invulnerable, so
    /// a boss running into it does not restart the course); run so the boss is active and set up.
    fn world_with(kind: Kind, big: bool) -> World {
        let mut lvl = Level::flat(60);
        lvl.spawns.push(spawn(kind, 240.0, big));
        let mut w = World::with_level(lvl);
        w.player.x = 40.0;
        w.player.invuln = 1_000_000;
        run(&mut w, 0, 2);
        assert!(w.actors[0].kind.is_boss());
        w
    }

    fn run(w: &mut World, buttons: u32, frames: usize) {
        for _ in 0..frames {
            w.step(Input { buttons });
        }
    }

    fn boss(w: &World) -> &Actor {
        w.actors.iter().find(|a| a.kind.is_boss()).expect("boss")
    }

    fn boss_mut(w: &mut World) -> &mut Actor {
        w.actors
            .iter_mut()
            .find(|a| a.kind.is_boss())
            .expect("boss")
    }

    /// Run until the boss can be hurt again (stunned / spinning / shell states over).
    fn wait_unguarded(w: &mut World) {
        for _ in 0..600 {
            if !boss(w).boss_guarding() {
                return;
            }
            run(w, 0, 1);
        }
        panic!("boss stayed guarded");
    }

    #[test]
    fn bowser_takes_20_fireballs_big_bowser_40() {
        for (big, hp) in [(false, 20u8), (true, 40)] {
            let mut w = world_with(BOWSER, big);
            assert_eq!(boss(&w).boss_hp(), hp);
            for n in 1..hp {
                assert_eq!(
                    boss_mut(&mut w).boss_hit(BossAttack::Fire, 0.0),
                    BossHit::Hurt
                );
                assert_eq!(boss(&w).boss_hp(), hp - n);
            }
            assert_eq!(
                boss_mut(&mut w).boss_hit(BossAttack::Fire, 0.0),
                BossHit::Defeated
            );
            assert_eq!(boss(&w).mode, Mode::DieFall);
        }
    }

    #[test]
    fn bowser_shells_cost_3_and_stomps_nothing() {
        let mut w = world_with(BOWSER, false);
        let b = boss_mut(&mut w);
        assert_eq!(b.boss_hit(BossAttack::Stomp, 0.0), BossHit::Ignored);
        for _ in 0..6 {
            assert_eq!(b.boss_hit(BossAttack::Shell, 0.0), BossHit::Hurt);
        }
        assert_eq!(b.boss_hp(), 2);
        assert_eq!(b.boss_hit(BossAttack::Shell, 0.0), BossHit::Defeated);
    }

    #[test]
    fn a_player_fireball_costs_bowser_one_hp() {
        let mut w = world_with(BOWSER, false);
        let (x, y) = (boss(&w).x, boss(&w).y);
        let mut f = Actor::new(Kind::Fireball, x, y + 8.0);
        f.timer = 600;
        w.add_actor(f);
        run(&mut w, 0, 1);
        assert_eq!(boss(&w).boss_hp(), 19);
        assert!(w.actors.iter().all(|a| a.kind != Kind::Fireball));
    }

    #[test]
    fn a_sliding_shell_costs_bowser_three_hp_once_and_slides_on() {
        for (big, hp) in [(false, 17u8), (true, 15)] {
            let mut w = world_with(BOWSER, false);
            let (x, y) = (boss(&w).x, boss(&w).y);
            let mut s = Actor::new(Kind::Shell { red: false }, x - 12.0, y);
            s.big = big;
            s.mode = Mode::ShellWait;
            s.kick(1, 0.0);
            s.kick_lock = 0;
            w.add_actor(s);
            let sid = w.actors.last().unwrap().id;
            run(&mut w, 0, 2);
            assert_eq!(boss(&w).boss_hp(), hp, "big {big}");
            // it slides on through him, hitting him once
            for _ in 0..8 {
                run(&mut w, 0, 1);
                let shell = w.actors.iter().find(|a| a.id == sid).expect("the shell");
                assert_eq!(shell.mode, Mode::ShellSlide);
            }
            assert_eq!(boss(&w).boss_hp(), hp);
        }
    }

    #[test]
    fn a_pow_quake_defeats_bowser_outright() {
        let mut lvl = Level::flat(40);
        lvl.spawns.push(spawn(Kind::Pow, 56.0, false));
        lvl.spawns[0].y = 64.0;
        lvl.spawns.push(spawn(BOWSER, 300.0, false));
        let mut w = World::with_level(lvl);
        run(&mut w, 0, 5);
        run(&mut w, input::JUMP, 20);
        assert!(w.actors.iter().all(|a| a.kind != Kind::Pow));
        assert!(
            w.actors
                .iter()
                .any(|a| matches!(a.kind, Kind::Bowser { .. }) && a.mode == Mode::DieFall)
        );
    }

    #[test]
    fn landing_on_bowser_hurts() {
        let mut w = world_with(BOWSER, false);
        let (x, y) = (boss(&w).x, boss(&w).y);
        w.player.invuln = 0;
        w.player.x = x;
        w.player.y = y + 30.0;
        w.player.vy = -2.0;
        w.player.on_ground = false;
        let mut hurt = false;
        for _ in 0..10 {
            run(&mut w, 0, 1);
            hurt |= w.events.contains(&Event::Death);
        }
        assert!(hurt, "small player dies on Bowser's head");
    }

    #[test]
    fn bowser_flames_hurt_the_player() {
        let mut w = world_with(BOWSER, false);
        w.player.invuln = 0;
        let mut f = Actor::new(Kind::BowserFire, w.player.x + 30.0, w.player.y + 4.0);
        f.vx = -1.5;
        f.home_y = w.player.y + 4.0;
        f.timer = 600;
        w.add_actor(f);
        run(&mut w, 0, 30);
        assert!(!w.player.alive());
    }

    #[test]
    fn a_star_knocks_a_boss_off_and_its_key_goes_to_the_player() {
        let mut lvl = Level::flat(60);
        let held = Kind::Bowser {
            key: true,
            clown: false,
        };
        lvl.spawns.push(spawn(held, 240.0, false));
        let mut w = World::with_level(lvl);
        w.player.x = 40.0;
        run(&mut w, 0, 2);
        w.player.star = 600;
        w.player.x = boss(&w).x;
        run(&mut w, 0, 1);
        let b = w.actors.iter().find(|a| a.kind.is_boss()).unwrap();
        assert_eq!(b.mode, Mode::DieFall);
        // the key event counts it for the player at once, and only once
        assert_eq!(w.keys, 1);
        assert!(w.actors.iter().all(|a| a.kind != Kind::Key));
        run(&mut w, 0, 5);
        assert_eq!(w.keys, 1);
    }

    #[test]
    fn boom_boom_dies_after_three_stomps() {
        let mut w = world_with(BOOM_BOOM, false);
        assert_eq!(boss(&w).boss_hp(), 3);
        for n in 1..=3u8 {
            wait_unguarded(&mut w);
            let (x, y) = (boss(&w).x, boss(&w).y);
            w.player.x = x;
            w.player.y = y + 28.0;
            w.player.vy = -1.0;
            w.player.on_ground = false;
            run(&mut w, 0, 1);
            assert!(w.player.vy > 3.0, "bounced off stomp {n}");
            if n < 3 {
                assert_eq!(boss(&w).boss_hp(), 3 - n);
                assert!(boss(&w).boss_guarding(), "stunned after stomp {n}");
                // a second stomp while stunned only bounces
                let b = boss_mut(&mut w);
                assert_eq!(b.boss_hit(BossAttack::Stomp, 0.0), BossHit::Guarded);
                w.player.x = 40.0;
                w.player.y = 32.0;
            } else {
                assert_eq!(boss(&w).mode, Mode::DieFall);
            }
            assert!(w.player.alive());
        }
    }

    #[test]
    fn boom_boom_fireball_counter_5_big_10() {
        for (big, shots) in [(false, 5), (true, 10)] {
            let mut w = world_with(BOOM_BOOM, big);
            for _ in 1..shots {
                let hit = boss_mut(&mut w).boss_hit(BossAttack::Fire, 0.0);
                assert_eq!(hit, BossHit::Absorbed);
            }
            assert_eq!(boss(&w).boss_hp(), 3);
            assert_eq!(
                boss_mut(&mut w).boss_hit(BossAttack::Fire, 0.0),
                BossHit::Hurt
            );
            assert_eq!(boss(&w).boss_hp(), 2);
            assert_eq!(boss(&w).boss_fire_counter(), shots as u8, "refilled");
        }
    }

    #[test]
    fn boom_boom_runs_faster_after_two_hits() {
        let mut w = world_with(BOOM_BOOM, false);
        for _ in 0..2 {
            wait_unguarded(&mut w);
            boss_mut(&mut w).boss_hit(BossAttack::Shell, 0.0);
        }
        wait_unguarded(&mut w);
        // after the slow walk, it runs at 1.5 px/f
        let mut top: f32 = 0.0;
        for _ in 0..200 {
            run(&mut w, 0, 1);
            if boss(&w).on_ground {
                top = top.max(boss(&w).vx.abs());
            }
        }
        assert!((top - 1.5).abs() < 1e-4, "top speed {top}");
    }

    #[test]
    fn bowser_jr_takes_three_stomps_and_shells_up_in_between() {
        let mut w = world_with(BOWSER_JR, false);
        for n in 1..=3u8 {
            wait_unguarded(&mut w);
            let hit = boss_mut(&mut w).boss_hit(BossAttack::Stomp, 0.0);
            if n < 3 {
                assert_eq!(hit, BossHit::Hurt);
                let mut shelled = false;
                for _ in 0..200 {
                    run(&mut w, 0, 1);
                    shelled |= boss(&w).boss_in_shell();
                }
                assert!(shelled, "went into its shell after hit {n}");
            } else {
                assert_eq!(hit, BossHit::Defeated);
            }
        }
    }

    #[test]
    fn defeated_bosses_count_for_their_clear_condition_class() {
        assert_eq!(
            BOWSER.defeat_class(false),
            Some(crate::clear::enemy::BOWSER)
        );
        assert_eq!(
            BOOM_BOOM.defeat_class(false),
            Some(crate::clear::enemy::BOOM_BOOM)
        );
        let pom = Kind::BoomBoom {
            key: false,
            clown: false,
            pom: true,
        };
        assert_eq!(pom.defeat_class(false), Some(crate::clear::enemy::POM_POM));
        assert_eq!(
            BOWSER_JR.defeat_class(false),
            Some(crate::clear::enemy::BOWSER_JR)
        );
    }
}
