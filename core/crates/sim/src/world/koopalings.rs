//! World-side rules for the Koopalings (actors/koopalings.rs): the player touching them and
//! their projectiles, the volleys they fire, Morton's and Roy's ground quake, Wendy's ring
//! limit, the projectiles vanishing with their owner, and the Key a Koopaling drops.

use super::{Event, World, overlap};
use crate::actors::{Actor, Kind, Mode};
use crate::render::Sprite;

/// A quake stuns for 80 frames (player state 0x58, QuakeDamage). Morton's Quake enter (and
/// Roy's LandOnQuake) runs only while the Koopaling's in-view flag (+0x520 bit 3) is set and
/// asks each player whose camera view its box touches (edges included); the request
/// 0x71015a6880 does nothing unless the player is on the ground (+0x2308 bit 0, in the place of
/// v1's on-ground bit +0x22D0 in the Sledge Bro request 0x71009d5520), has no Star (+0x27D4,
/// v1 +0x2770) and no damage invincibility (+0x2798, v1 +0x2734), and is not a Power Balloon
/// (form 12) [obs 0x71011eab30, 0x71015a6880].
const QUAKE_STUN: i32 = 80;

impl World {
    /// The player touching a Koopaling or one of its projectiles at `i`; true when hurt.
    pub(super) fn koopaling_contact(&mut self, i: usize, jump_held: bool) -> bool {
        let hurt = self.player.hurt_box();
        let a = &self.actors[i];
        if a.koopaling_hidden() || !overlap(hurt, a.bbox()) {
            return false;
        }
        let (ax, ay) = (a.x, a.y);
        let px = self.player.x;
        let star = self.player.star > 0;
        if let Kind::KoopalingShot { what } = a.kind {
            if star {
                return false;
            }
            // a player can bounce on Lemmy's balls [?: the ball's "bounce callbacks" once cited
            // (0x7100de8560 … 0x7100de86b0) are its own spring-ride slots (vtable 0x1C0–0x1D8
            // and 0x1F0–0x200: start, launch with its own vy 4.0 / 4.15, riding?, cancel; v1's
            // trampoline slots 0x1A0/0x1A8 four slots on), so they say nothing about the player;
            // the player-vs-projectile stomp rule is not traced]
            if what == crate::actors::koopaling_shot::BALL && self.v3_from_above_pub(i) {
                self.player.bounce(jump_held);
                self.events.push(Event::Bump);
                return false;
            }
            return self.v3_hurt();
        }
        if star {
            // star: DieFall at once [obs EnemyDB]
            self.actors[i].die_fall(px);
            self.v3_defeated(ax, ay, false);
            return false;
        }
        if self.actors[i].kick_lock > 0 && !self.actors[i].koopaling_shell_moving() {
            return false;
        }
        let above = self.v3_from_above_pub(i);
        if above {
            let pound = self.player.pound == -1;
            if self.actors[i].koopaling_guarded() {
                // in the shell: stomps bounce off [obs]
                if !pound {
                    self.player.bounce(jump_held);
                }
                self.events.push(Event::Bump);
                return false;
            }
            let (_, down) = self.actors[i].koopaling_damage(px, true);
            self.actors[i].kick_lock = 20;
            if !pound {
                self.player.bounce(jump_held);
            }
            if down {
                self.v3_defeated(ax, ay, true);
                self.events.push(Event::EnemyDown);
            } else {
                self.events.push(Event::Stomp);
            }
            return false;
        }
        if self.player.sliding && !self.actors[i].koopaling_guarded() {
            let (_, down) = self.actors[i].koopaling_damage(px, false);
            self.actors[i].kick_lock = 20;
            if down {
                self.v3_defeated(ax, ay, false);
            }
            return false;
        }
        self.v3_hurt()
    }

    pub(super) fn v3_from_above_pub(&self, i: usize) -> bool {
        let a = &self.actors[i];
        self.player.vy < 0.0 && self.player.y >= a.y + a.height() * 0.5
    }

    /// Per frame: volleys, Morton's flames, quakes, Wendy's ring limit, orphaned projectiles,
    /// dropped Keys.
    pub(super) fn koopaling_world(&mut self) {
        let mut born: Vec<Actor> = Vec::new();
        let mut quake = false;
        let mut keys = Vec::new();
        let v = self.view_rect(0.0);
        for i in 0..self.actors.len() {
            if !matches!(self.actors[i].kind, Kind::Koopaling { .. }) {
                continue;
            }
            if self.actors[i].v3.shots > 0 {
                self.actors[i].v3.shots = 0;
                let level = &self.level;
                born.extend(self.actors[i].koopaling_shots(level));
            }
            let a = &mut self.actors[i];
            if a.v3.flames {
                a.v3.flames = false;
                born.extend(a.morton_flames());
            }
            if a.v3.quake {
                a.v3.quake = false;
                let (x0, y0, x1, y1) = a.bbox();
                quake |= x1 >= v.0 && x0 <= v.2 && y1 >= v.1 && y0 <= v.3;
            }
            if !a.alive() && a.koopaling_key() {
                keys.push((a.x, a.y + a.height() / 2.0 - 8.0));
            }
        }
        if quake {
            if self.player.on_ground && self.player.star == 0 && self.player.invuln == 0 {
                self.player.stun = QUAKE_STUN;
                self.player.vx = 0.0;
            }
            self.events.push(Event::Bump);
        }
        for a in born {
            self.add_actor(a);
        }
        for (x, y) in keys {
            self.add_actor(Actor::new(Kind::Key, x, y));
        }
        // projectiles go when their Koopaling is defeated or gone [obs 0x7100dd9770]
        let live: Vec<(u32, u8)> = self
            .actors
            .iter()
            .filter(|a| matches!(a.kind, Kind::Koopaling { .. }) && a.alive() && a.v3.hp > 0)
            .map(|a| (a.id, a.v3.hp))
            .collect();
        let ring = crate::actors::koopaling_shot::RING;
        for (id, hp) in &live {
            // Wendy: 1 / 2 / 3 rings at 3 / 2 / 1 HP; beyond that the oldest goes 60 frames
            // later [obs 0x71012bf260]
            let limit = (4 - hp) as usize;
            let rings: Vec<usize> = (0..self.actors.len())
                .filter(|&k| {
                    let r = &self.actors[k];
                    r.kind == (Kind::KoopalingShot { what: ring })
                        && r.v3.owner == *id
                        && r.mode == Mode::Normal
                })
                .collect();
            if rings.len() > limit {
                let k = rings[0];
                if self.actors[k].timer > 60 {
                    self.actors[k].timer = 60;
                }
            }
        }
        for a in &mut self.actors {
            if let Kind::KoopalingShot { .. } = a.kind
                && a.mode == Mode::Normal
                && !live.iter().any(|(id, _)| *id == a.v3.owner)
            {
                a.mode = Mode::Dead;
            }
        }
    }

    /// Lemmy's ball under him while he rides it.
    pub(super) fn koopaling_sprites(&self, out: &mut Vec<Sprite>) {
        for a in &self.actors {
            if let Some((x, cy, r)) = a.lemmy_ball()
                && a.mode == Mode::Normal
            {
                out.push(Self::model_sprite(
                    "M1_Enemy_lemmy",
                    "ball.0",
                    x - r,
                    cy,
                    2.0 * r,
                    2.0 * r,
                    false,
                    39,
                ));
                // Lemmy himself stands on top of it
                if let Some((model, tex, w, h, flip)) = a.koopaling_body_look() {
                    let (w, h) = (w * a.v3_scale(), h * a.v3_scale());
                    out.push(Self::model_sprite(
                        model,
                        tex,
                        x - w / 2.0,
                        cy + r,
                        w,
                        h,
                        flip,
                        40,
                    ));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::*;
    use crate::actors::koopaling_who as who;

    fn spawn(kind: Kind, x: f32, y: f32, link: i16, clown: bool) -> Spawn {
        Spawn {
            kind,
            x,
            y,
            big: false,
            wings: false,
            pipe: None,
            parachute: false,
            rail: None,
            link,
            clown,
            alt: false,
        }
    }

    const C_MOVE: u8 = 40;
    const C_DAMAGE: u8 = 41;
    const C_DAMAGE_ANIM: u8 = 42;
    const C_SHELL_IN: u8 = 43;
    const C_SHELL_MOVE: u8 = 44;
    const C_SHELL_OUT: u8 = 45;

    /// A Koopaling (`who`, holding a Key) in a Koopa Clown Car floating at x = 200 in `style`;
    /// the player parked at the left, invulnerable.
    fn car_world(who: u8, style: &str) -> World {
        let mut lvl = Level::flat(80);
        lvl.style = style.to_string();
        lvl.spawns
            .push(spawn(Kind::ClownCar { fire: false }, 200.0, 80.0, 4, false));
        lvl.spawns.push(spawn(
            Kind::Koopaling {
                who,
                key: true,
                clown: true,
            },
            200.0,
            104.0,
            4,
            true,
        ));
        let mut w = World::with_level(lvl);
        w.player.x = 40.0;
        w.player.invuln = 1_000_000;
        run(&mut w, 2);
        w
    }

    fn run(w: &mut World, frames: usize) {
        for _ in 0..frames {
            w.step(Input { buttons: 0 });
        }
    }

    fn koop(w: &World) -> &Actor {
        w.actors
            .iter()
            .find(|a| matches!(a.kind, Kind::Koopaling { .. }))
            .expect("koopaling")
    }

    fn car(w: &World) -> &Actor {
        w.actors
            .iter()
            .find(|a| matches!(a.kind, Kind::ClownCar { .. }))
            .expect("car")
    }

    /// Projectiles fired so far (`seen` collects their ids).
    fn shots(w: &World, seen: &mut Vec<u32>) -> usize {
        for a in &w.actors {
            if matches!(a.kind, Kind::KoopalingShot { .. }) && !seen.contains(&a.id) {
                seen.push(a.id);
            }
        }
        seen.len()
    }

    /// Drop the player onto the Koopaling's head (then park him again).
    fn stomp(w: &mut World) {
        let (x, top) = (koop(w).x, koop(w).y + koop(w).height());
        w.player.x = x;
        w.player.y = top + 2.0;
        w.player.vy = -2.0;
        w.player.on_ground = false;
        run(w, 1);
        w.player.x = 40.0;
        w.player.y = 32.0;
        w.player.vy = 0.0;
    }

    #[test]
    fn a_koopaling_rides_its_car_and_fires_from_it() {
        let mut w = car_world(who::LARRY, "M1");
        let (k, c) = (koop(&w), car(&w));
        assert!(k.koopaling_in_car());
        assert_eq!(k.koopaling_state(), C_MOVE);
        assert_eq!(
            (k.x, k.y),
            (c.x, c.y + 24.0),
            "24 px above the car's origin"
        );
        assert_eq!(c.phase, 1, "the car chases");
        // the first volley after [100,150) frames, throw_st 5, the 30-frame hold and the
        // throw_ed shot frame 3
        let mut first = None;
        let mut seen = Vec::new();
        for f in 0..260 {
            run(&mut w, 1);
            assert_eq!(koop(&w).y, car(&w).y + 24.0);
            if shots(&w, &mut seen) > 0 {
                first = Some(f + 2);
                break;
            }
        }
        let first = first.expect("fires from the car");
        assert!((100 + 38..150 + 42).contains(&first), "{first}");
        // one shot per volley at 3 HP: none for the next 100 frames
        for _ in 0..100 {
            run(&mut w, 1);
            shots(&w, &mut seen);
        }
        assert_eq!(seen.len(), 1);
        assert!(car(&w).x < 200.0, "the car has flown toward the player");
    }

    #[test]
    fn a_stomp_stops_the_car_and_the_koopaling_ducks_into_his_shell() {
        let mut w = car_world(who::LARRY, "M1");
        run(&mut w, 30);
        stomp(&mut w);
        assert_eq!(koop(&w).koopaling_hp(), 2);
        assert_eq!(koop(&w).koopaling_state(), C_DAMAGE);
        assert!(koop(&w).koopaling_guarded());
        let at = car(&w).x;
        // squash 4 + 3, 18, ducking 10, 36 hidden, 10 back up in the shell
        let mut n = 1;
        while koop(&w).koopaling_state() == C_DAMAGE {
            assert_eq!(car(&w).phase, 3, "stopped");
            assert_eq!(car(&w).x, at);
            run(&mut w, 1);
            n += 1;
            assert!(n < 200);
        }
        assert!((78..=84).contains(&n), "{n}");
        assert_eq!(koop(&w).koopaling_state(), C_SHELL_MOVE);
        run(&mut w, 1);
        assert_eq!(car(&w).phase, 1, "chasing again");
        // stomps bounce off the shell
        stomp(&mut w);
        assert_eq!(koop(&w).koopaling_hp(), 2);
        run(&mut w, 300);
        assert_eq!(koop(&w).koopaling_state(), C_SHELL_OUT);
        run(&mut w, 25);
        assert_eq!(koop(&w).koopaling_state(), C_MOVE);
        assert!(!koop(&w).koopaling_guarded());
    }

    #[test]
    fn the_third_stomp_knocks_him_out_of_the_car_and_frees_it() {
        let mut w = car_world(who::LUDWIG, "M3");
        for hp in [2, 1] {
            stomp(&mut w);
            assert_eq!(koop(&w).koopaling_hp(), hp);
            for _ in 0..600 {
                if !koop(&w).koopaling_guarded() {
                    break;
                }
                run(&mut w, 1);
            }
        }
        stomp(&mut w);
        assert!(!koop(&w).alive(), "knocked out at once outside NSMBU");
        run(&mut w, 2);
        assert_eq!(car(&w).phase, 0, "the empty car floats (Free)");
        assert_eq!(
            w.actors.iter().filter(|a| a.kind == Kind::Key).count() + w.keys as usize,
            1,
            "his Key drops"
        );
    }

    #[test]
    fn nsmbu_flinches_in_the_car_then_spins_in_the_shell() {
        let mut w = car_world(who::LEMMY, "WU");
        assert!(koop(&w).lemmy_ball().is_none(), "no ball in the car");
        stomp(&mut w);
        assert_eq!(koop(&w).koopaling_state(), C_DAMAGE_ANIM);
        run(&mut w, 59);
        assert_eq!(koop(&w).koopaling_state(), C_DAMAGE_ANIM);
        assert_eq!(car(&w).phase, 3);
        run(&mut w, 1);
        assert_eq!(koop(&w).koopaling_state(), C_SHELL_IN);
        run(&mut w, 16);
        assert_eq!(koop(&w).koopaling_state(), C_SHELL_MOVE);
        run(&mut w, 300 + 30);
        assert_eq!(koop(&w).koopaling_state(), C_MOVE);
        // at 0 HP NSMBU flinches first, then falls out
        koop_mut(&mut w).v3.hp = 1;
        stomp(&mut w);
        assert_eq!(koop(&w).koopaling_state(), C_DAMAGE_ANIM);
        assert!(koop(&w).alive());
        run(&mut w, 61);
        assert!(!koop(&w).alive());
    }

    #[test]
    fn lemmy_throws_his_balls_from_the_car_five_at_1_hp() {
        let mut w = car_world(who::LEMMY, "M1");
        koop_mut(&mut w).v3.hp = 1;
        // the first volley: 5 balls back to back, then a pause of [60,90) frames
        let mut seen = Vec::new();
        let mut at = Vec::new();
        for f in 0..900 {
            run(&mut w, 1);
            if shots(&w, &mut seen) > at.len() {
                at.push(f);
            }
        }
        assert!(at.len() >= 6, "{at:?}");
        // within the volley one throw apart: throw_ed after the shot frame (60 − 4), throw_st
        // 5 + 1, the hold 30, throw_ed to the shot frame 4
        assert!(
            at[1..5].iter().zip(&at[..4]).all(|(b, a)| b - a == 96),
            "{at:?}"
        );
        assert!(at[5] - at[4] >= 96 + 60, "{at:?}");
        assert!(w.actors.iter().any(|a| a.kind
            == Kind::KoopalingShot {
                what: crate::actors::koopaling_shot::BALL
            }));
    }

    #[test]
    fn without_a_car_he_falls_out() {
        let mut lvl = Level::flat(80);
        lvl.spawns.push(spawn(
            Kind::Koopaling {
                who: who::IGGY,
                key: false,
                clown: true,
            },
            200.0,
            104.0,
            9,
            true,
        ));
        let mut w = World::with_level(lvl);
        w.player.x = 40.0;
        w.player.invuln = 1_000_000;
        run(&mut w, 120);
        assert!(!koop(&w).koopaling_in_car());
        assert!(koop(&w).on_ground && koop(&w).y < 40.0, "{}", koop(&w).y);
    }

    fn koop_mut(w: &mut World) -> &mut Actor {
        w.actors
            .iter_mut()
            .find(|a| matches!(a.kind, Kind::Koopaling { .. }))
            .expect("koopaling")
    }
}
