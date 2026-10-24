//! The SMM1 layouts' own sounds (`mmk import-smm1` → `ui/sound/sounds.json` + WAVs; see
//! `mmk_import::smm1_ui_sound` and research/specs/smm1-sound.md), resolved the way SMM1's
//! SLink database says:
//! - each layout and each part of it is an SLink user, `UI_<layout>[_<part>…]` (a nested part
//!   path joined with `_`); the game fires event keys on it (`Decide`, `In`, `Out`,
//!   `TouchSelect`, `Countdown1`, `CouseInWalk100`, …) and enters states that start keys at a
//!   frame (`State::cState_Walk` → `walk`, `State::c1stDemoAnimB` → `peachHelp`);
//! - a key plays a sound, or a container: a switch plays its first child whose condition holds
//!   (else an unconditioned one), a blend / sequence all its children, a random one by weight;
//!   volumes and delays (frames) are the data's.
//!
//! [inf] A switch's condition is checked against the values the screen passes ([`Ctx`]: the
//! game style "M1", the number of Marios, …), not by the property's name (the database's
//! property names are Japanese). A part without its own user, or without the key, falls back to
//! the layout's user, then to the scene root's (`UI_Nwk_Root`, `UI_Play_Root`, `UI_Cmn_Root`).

use std::cell::Cell;
use std::collections::HashMap;

use serde_json::Value;

use crate::layout::UiKind;
use crate::sound::SoundCue;

/// The database keeps names in at most this many bytes (`State::cState_TouchSele`).
const NAME_MAX: usize = 23;

/// A value a switch's condition is checked against.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Ctx<'a> {
    Enum(&'a str),
    Int(i32),
}

/// `sounds.json` of the SMM1 import, parsed.
#[derive(Clone, Debug, Default)]
pub struct Smm1Sounds {
    users: HashMap<String, Value>,
    /// The random containers' generator state.
    seed: Cell<u32>,
}

/// SLink user of `layout`'s part `part` (a `/`-separated part path), or of the layout.
pub fn user_name(layout: &str, part: Option<&str>) -> String {
    match part {
        Some(p) if !p.is_empty() => format!("UI_{layout}_{}", p.replace('/', "_")),
        _ => format!("UI_{layout}"),
    }
}

fn state_matches(stored: &str, wanted: &str) -> bool {
    stored == wanted || (stored.len() == NAME_MAX && wanted.starts_with(stored))
}

impl Smm1Sounds {
    pub fn from_json(s: &str) -> Option<Self> {
        let doc: Value = serde_json::from_str(s).ok()?;
        if doc.get("game").and_then(Value::as_str) != Some("SMM1") {
            return None;
        }
        let users = doc
            .get("users")?
            .as_object()?
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        Some(Self {
            users,
            seed: Cell::new(0x2545_F491),
        })
    }

    /// The users to look in, most specific first.
    fn chain(&self, layout: &str, part: Option<&str>) -> Vec<&Value> {
        let mut names = Vec::new();
        if part.is_some() {
            names.push(user_name(layout, part));
        }
        names.push(user_name(layout, None));
        if let Some(scene) = layout.split('_').next() {
            names.push(format!("UI_{scene}_Root"));
        }
        names.iter().filter_map(|n| self.users.get(n)).collect()
    }

    /// Key `key` fired on `layout` (on its part `part`).
    pub fn key(&self, layout: &str, part: Option<&str>, key: &str, ctx: &[Ctx]) -> Vec<SoundCue> {
        let mut out = Vec::new();
        if let Some(node) = self
            .chain(layout, part)
            .into_iter()
            .find_map(|u| u["keys"].get(key))
        {
            self.node(node, ctx, 0.0, &mut out);
        }
        out
    }

    /// `layout` (its part `part`) entered state `state` (`cState_Walk`, without `State::`):
    /// the keys its triggers start, each delayed by its trigger frame.
    pub fn state(
        &self,
        layout: &str,
        part: Option<&str>,
        state: &str,
        ctx: &[Ctx],
    ) -> Vec<SoundCue> {
        let wanted = format!("State::{state}");
        let mut out = Vec::new();
        for u in self.chain(layout, part) {
            let Some(acts) = u["actions"].as_object() else {
                continue;
            };
            let Some((_, trigs)) = acts.iter().find(|(k, _)| state_matches(k, &wanted)) else {
                continue;
            };
            for t in trigs.as_array().into_iter().flatten() {
                let frame = t["frame"].as_f64().unwrap_or(0.0).max(0.0) as f32;
                if let Some(node) = t["key"].as_str().and_then(|k| u["keys"].get(k)) {
                    self.node(node, ctx, frame, &mut out);
                }
            }
            break;
        }
        out
    }

    fn holds(cond: &Value, ctx: &[Ctx]) -> bool {
        let cmp = cond["cmp"].as_str().unwrap_or("==");
        if let Some(e) = cond["enum"].as_str() {
            let has = ctx.contains(&Ctx::Enum(e));
            return if cmp == "!=" { !has } else { has };
        }
        let Some(want) = cond["int"].as_f64().or_else(|| cond["float"].as_f64()) else {
            return false;
        };
        let Some(v) = ctx.iter().find_map(|c| match c {
            Ctx::Int(n) => Some(*n as f64),
            Ctx::Enum(_) => None,
        }) else {
            return false;
        };
        match cmp {
            ">" => v > want,
            ">=" => v >= want,
            "<" => v < want,
            "<=" => v <= want,
            "!=" => v != want,
            _ => v == want,
        }
    }

    /// A pseudo-random number in [0, 1) (xorshift).
    fn random(&self) -> f32 {
        let mut x = self.seed.get();
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.seed.set(x);
        (x >> 8) as f32 / (1u32 << 24) as f32
    }

    fn node(&self, node: &Value, ctx: &[Ctx], delay: f32, out: &mut Vec<SoundCue>) {
        let d = delay + node["delay"].as_f64().unwrap_or(0.0) as f32;
        if let Some(sound) = node["sound"].as_str() {
            if sound != "@Blank" {
                out.push(SoundCue {
                    name: sound.to_string(),
                    volume: node["volume"].as_f64().unwrap_or(1.0) as f32,
                    delay: d,
                    ui: UiKind::Smm1,
                });
            }
            return;
        }
        let Some(children) = node["children"].as_array() else {
            return;
        };
        match node["kind"].as_str().unwrap_or("") {
            "switch" => {
                let pick = children
                    .iter()
                    .find(|c| c.get("if").is_some_and(|w| Self::holds(w, ctx)))
                    .or_else(|| children.iter().find(|c| c.get("if").is_none()));
                if let Some(c) = pick {
                    self.node(c, ctx, d, out);
                }
            }
            "random" | "random2" => {
                let w = |c: &Value| c["weight"].as_f64().unwrap_or(1.0).max(0.0) as f32;
                let total: f32 = children.iter().map(w).sum();
                let mut r = self.random() * total;
                for c in children {
                    r -= w(c);
                    if r < 0.0 {
                        self.node(c, ctx, d, out);
                        break;
                    }
                }
            }
            // blend, sequence: every child, with its own delay [inf: a sequence's children
            // are not chained by their lengths]
            _ => {
                for c in children {
                    self.node(c, ctx, d, out);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = r#"{"game": "SMM1", "users": {
      "UI_Map_00": {"keys": {
          "peachHelp": {"sound": "Help", "volume": 0.4},
          "Decide": {"sound": "Map_ok"}},
        "actions": {"State::c1stDemoAnimB": [{"key": "peachHelp", "frame": 0}]}},
      "UI_Map_00_L_Mario_00": {"keys": {
          "jump": {"sound": "Jump"}, "Move": {"sound": "Move"}},
        "actions": {"State::cState_TwoJump": [{"key": "jump", "frame": 20}, {"key": "jump", "frame": 0}],
                    "State::cState_TouchSele": [{"key": "Move", "frame": 0}]}},
      "UI_Map_00_L_Sub_00_L_Tab_10": {"keys": {
          "Decide": {"kind": "blend", "children": [
            {"key": "a", "sound": "Left"}, {"key": "b", "sound": "Open", "delay": 40}]}}},
      "UI_Map_Root": {"keys": {"TouchSelect": {"sound": "Cursor"}}},
      "UI_Card_00": {"keys": {
          "In": {"kind": "switch", "children": [
            {"key": "m1", "sound": "M1_Pose", "if": {"cmp": "==", "enum": "M1"}},
            {"key": "mw", "sound": "MW_Pose", "volume": 0.8, "if": {"cmp": "==", "enum": "MW"}}]},
          "Jump": {"kind": "blend", "delay": 2, "children": [
            {"key": "j", "sound": "Hop"},
            {"key": "w", "kind": "switch", "children": [
              {"key": "w80", "sound": "Walk80", "delay": 32, "if": {"cmp": ">=", "int": 80}},
              {"key": "w0", "sound": "Walk0", "delay": 32, "if": {"cmp": ">=", "int": 0}}]}]}}}
    }}"#;

    fn names(v: Vec<SoundCue>) -> Vec<(String, f32)> {
        v.into_iter().map(|c| (c.name, c.delay)).collect()
    }

    #[test]
    fn keys_states_and_fallbacks() {
        let s = Smm1Sounds::from_json(DOC).unwrap();
        assert_eq!(
            user_name("Map_00", Some("L_Sub_00/L_Tab_10")),
            "UI_Map_00_L_Sub_00_L_Tab_10"
        );
        // a nested part's blend, delays kept
        assert_eq!(
            names(s.key("Map_00", Some("L_Sub_00/L_Tab_10"), "Decide", &[])),
            [("Left".into(), 0.0), ("Open".into(), 40.0)]
        );
        // a part without the key: the layout's user, then the scene root's
        assert_eq!(
            names(s.key("Map_00", Some("L_Mario_00"), "Decide", &[])),
            [("Map_ok".into(), 0.0)]
        );
        assert_eq!(
            names(s.key("Map_00", Some("L_Castle_00"), "TouchSelect", &[])),
            [("Cursor".into(), 0.0)]
        );
        // states: trigger frames as delays; cut names match
        assert_eq!(
            names(s.state("Map_00", Some("L_Mario_00"), "cState_TwoJump", &[])),
            [("Jump".into(), 20.0), ("Jump".into(), 0.0)]
        );
        assert_eq!(
            names(s.state("Map_00", Some("L_Mario_00"), "cState_TouchSelect", &[])).len(),
            1
        );
        let help = s.state("Map_00", None, "c1stDemoAnimB", &[]);
        assert_eq!((help[0].volume, help[0].ui), (0.4, UiKind::Smm1));
        assert!(s.state("Map_00", None, "cState_Walk", &[]).is_empty());
        assert!(s.key("Nope_00", None, "In", &[]).is_empty());
    }

    #[test]
    fn switches_check_the_context() {
        let s = Smm1Sounds::from_json(DOC).unwrap();
        assert_eq!(
            names(s.key("Card_00", None, "In", &[Ctx::Enum("MW")])),
            [("MW_Pose".into(), 0.0)]
        );
        assert!(s.key("Card_00", None, "In", &[Ctx::Enum("3W")]).is_empty());
        assert_eq!(
            names(s.key("Card_00", None, "Jump", &[Ctx::Int(85)])),
            [("Hop".into(), 2.0), ("Walk80".into(), 34.0)]
        );
        assert_eq!(
            names(s.key("Card_00", None, "Jump", &[Ctx::Int(12)]))[1].0,
            "Walk0"
        );
        assert!(Smm1Sounds::from_json(r#"{"layouts": {}, "users": {}}"#).is_none());
    }
}
