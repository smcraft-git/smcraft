//! SMM1's beat bounces (research/specs/smm1-100mario-screens.md §3): every layout has a beat
//! component (0x0205c02c) with the animations `Beat_<x>_<y>` (x 0–3, y 0–1), each paired with a
//! pane `N_Beat_<x>_<y>` whose user data `BeatOffset` delays it, and the named ones `Anm_<n>_00`
//! (`N_Anm_<n>_00`). The scene's music drives them from its stream's markers (0x02089898):
//! `beat:<n>` beats end at `beatend` (else the loop's end; without either there are none), so a
//! beat is that end ÷ n samples; each tick the half-beat under the play position is counted
//! (0x02089a8c) and when it changes the layouts get (x = the beat in its bar, y = 0 on the beat,
//! 1 off it); `Beat_x_y` then starts `BeatOffset + 31` ticks later at 1.0 (0x0205cc00). A named
//! marker the music passes starts `Anm_<name>_00` the same way (0x0205cea0): Course World's
//! `Koopa` markers wake the castle's Bowser [obs].
//!
//! The music's position is the front end's count from the tick it started the tune (the host
//! plays it at once) [inf]; the half-beat mode is the maps' (their grass bounces are `y = 1`)
//! [inf: the mode switch's writer isn't traced].

use std::collections::HashMap;

use crate::layout::{Layout, Ui};

/// What the import says of a tune's beats (`ui/sound/bgm.json`, importer 6).
#[derive(Clone, Debug, PartialEq)]
pub struct Tune {
    pub rate: f32,
    /// Samples per beat (None: no beats).
    pub beat: Option<f32>,
    pub looping: Option<(f32, f32)>,
    /// The named markers (not the beat ones) by sample.
    pub marks: Vec<(String, f32)>,
}

impl Tune {
    pub const FILE: &str = "ui/sound/bgm.json";

    pub fn load(ui: &Ui, name: &str) -> Option<Self> {
        let doc = ui.cache_mut().json_file(Self::FILE)?;
        Self::from_json(doc.get(name)?)
    }

    pub fn from_json(v: &serde_json::Value) -> Option<Self> {
        let rate = v["rate"].as_f64()? as f32;
        let looping = v["loop"]
            .as_array()
            .filter(|a| a.len() == 2)
            .and_then(|a| Some((a[0].as_f64()? as f32, a[1].as_f64()? as f32)));
        let mut count = None;
        let mut end = None;
        let mut marks = Vec::new();
        for (k, p) in v["marks"].as_object().into_iter().flatten() {
            let p = p.as_f64().unwrap_or(0.0) as f32;
            if let Some(n) = k.strip_prefix("beat:") {
                count = n.parse::<f32>().ok();
            } else if k == "beatend" {
                end = Some(p);
            } else if !matches!(k.as_str(), "loopstart" | "loopend") {
                marks.push((k.clone(), p));
            }
        }
        let end = end.or(looping.map(|l| l.1));
        let beat = match (count, end) {
            (Some(n), Some(e)) if n > 0.0 && e > 0.0 => Some(e / n),
            _ => None,
        };
        Some(Self {
            rate,
            beat,
            looping,
            marks,
        })
    }
}

/// What the music's clock gives in a tick.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BeatEvents {
    /// A new half-beat: (x, y).
    pub beat: Option<(usize, usize)>,
    /// Named markers passed.
    pub marks: Vec<String>,
}

/// The music's beat clock, counting the ticks since its tune started (it travels with the
/// scenes that keep the tune playing).
#[derive(Clone, Debug)]
pub struct BeatClock {
    tune: Tune,
    elapsed: i32,
    last: Option<i64>,
    last_pos: f32,
}

impl BeatClock {
    pub fn new(tune: Tune) -> Self {
        Self {
            tune,
            elapsed: 0,
            last: None,
            last_pos: -1.0,
        }
    }

    /// The clock of tune `name`, started now (None without its beat data).
    pub fn for_music(ui: &Ui, name: &str) -> Option<Self> {
        Tune::load(ui, name).map(Self::new)
    }

    /// The sample under the play position `ticks` after the start (looping back at the loop's
    /// end).
    pub fn position(&self, ticks: i32) -> f32 {
        let p = ticks.max(0) as f32 * self.tune.rate / 60.0;
        match self.tune.looping {
            Some((s, e)) if e > s && p >= e => s + (p - s) % (e - s),
            _ => p,
        }
    }

    /// One tick of the music: what it passed.
    pub fn step(&mut self) -> BeatEvents {
        let pos = self.position(self.elapsed);
        self.elapsed += 1;
        let mut ev = BeatEvents::default();
        for (name, at) in &self.tune.marks {
            let passed = if pos >= self.last_pos {
                self.last_pos < *at && *at <= pos
            } else {
                // looped back
                *at > self.last_pos || *at <= pos
            };
            if passed && self.last_pos >= 0.0 {
                ev.marks.push(name.clone());
            }
        }
        self.last_pos = pos;
        let Some(spb) = self.tune.beat else {
            return ev;
        };
        let beats = pos / spb;
        let half = (beats * 2.0) as i64;
        if self.last != Some(half) {
            self.last = Some(half);
            let bar = (beats * 0.25) as i64;
            ev.beat = Some(((beats as i64 - 4 * bar) as usize, (half % 2) as usize));
        }
        ev
    }
}

/// The animations a beat starts later.
pub const BEAT_DELAY: i32 = 31;

/// A layout's (and its parts') beat animations with their countdowns.
#[derive(Clone, Debug, Default)]
pub struct BeatBounce {
    /// (anim path, x, y, BeatOffset), and the named ones (anim path, marker, BeatOffset).
    beats: Vec<(String, usize, usize, i32)>,
    named: Vec<(String, String, i32)>,
    countdown: HashMap<String, i32>,
}

impl BeatBounce {
    /// The beat animations of `l` and of its parts `parts` (`L_…` panes holding layouts that
    /// have their own).
    pub fn new(l: &mut Layout, parts: &[String]) -> Self {
        let mut b = Self::default();
        for prefix in std::iter::once(String::new()).chain(parts.iter().map(|p| format!("{p}/"))) {
            for x in 0..4 {
                for y in 0..2 {
                    let anim = format!("{prefix}Beat_{x:02}_{y:02}");
                    if l.has(&anim) {
                        let off = l
                            .user_ints(&format!("{prefix}N_Beat_{x:02}_{y:02}"), "BeatOffset")
                            .and_then(|v| v.first().copied())
                            .unwrap_or(0);
                        b.beats.push((anim, x, y, off));
                    }
                }
            }
            // the named markers' animations (`Koopa` is the only one the data has)
            let name = "Koopa";
            let anim = format!("{prefix}Anm_{name}_00");
            if l.has(&anim) {
                let off = l
                    .user_ints(&format!("{prefix}N_Anm_{name}_00"), "BeatOffset")
                    .and_then(|v| v.first().copied())
                    .unwrap_or(0);
                b.named.push((anim, name.to_string(), off));
            }
        }
        b
    }

    pub fn is_empty(&self) -> bool {
        self.beats.is_empty() && self.named.is_empty()
    }

    /// The tick's events arm the matching animations; those due start at `tick`.
    pub fn step(&mut self, l: &mut Layout, ev: &BeatEvents, tick: i32) {
        if let Some((x, y)) = ev.beat {
            for (anim, bx, by, off) in &self.beats {
                if (*bx, *by) == (x, y) {
                    self.countdown.insert(anim.clone(), off + BEAT_DELAY);
                }
            }
        }
        for m in &ev.marks {
            for (anim, name, off) in &self.named {
                if name == m {
                    self.countdown.insert(anim.clone(), off + BEAT_DELAY);
                }
            }
        }
        let mut due = Vec::new();
        for (anim, c) in &mut self.countdown {
            *c -= 1;
            if *c <= 0 {
                due.push(anim.clone());
            }
        }
        for anim in due {
            self.countdown.remove(&anim);
            l.play(&anim, tick);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn course_world() -> Tune {
        Tune::from_json(&json!({
            "rate": 32000, "loop": [150587, 1054117],
            "marks": { "beat:56": 0, "loopstart": 150587, "Koopa": 376470, "beatend": 1054117,
                       "loopend": 1054117 }
        }))
        .unwrap()
    }

    #[test]
    fn beats_come_from_the_markers() {
        let t = course_world();
        // 1054117 / 56 = 18823.5 samples, 102 BPM
        assert!((t.beat.unwrap() - 18823.52).abs() < 0.1);
        assert_eq!(t.marks, vec![("Koopa".to_string(), 376470.0)]);
        // no end and no loop: no beats (the between-course maps' tunes)
        let map = Tune::from_json(&json!({ "rate": 32000, "loop": null,
            "marks": { "beat:56": 0 } }))
        .unwrap();
        assert_eq!(map.beat, None);
    }

    #[test]
    fn the_clock_counts_half_beats() {
        let mut c = BeatClock::new(course_world());
        let mut seen = Vec::new();
        let mut koopa = None;
        for t in 0..200 {
            let ev = c.step();
            if let Some(b) = ev.beat {
                seen.push((t, b));
            }
            if !ev.marks.is_empty() {
                koopa.get_or_insert(t);
            }
        }
        // a beat is 35.29 ticks: half-beats at 0, 18, 36, 53, …; x counts the beats in the bar
        assert_eq!(seen[0], (0, (0, 0)));
        assert_eq!(seen[1], (18, (0, 1)));
        assert_eq!(seen[2], (36, (1, 0)));
        assert_eq!(seen[8], (142, (0, 0)));
        assert_eq!(koopa, None, "Koopa is at 11.8 s");
        let mut c = BeatClock::new(course_world());
        let koopa = (0..1000).find(|_| !c.step().marks.is_empty());
        // 376470 / 32000 × 60
        assert_eq!(koopa, Some(706));
    }

    /// The owner's import (skipped without): the Easy map's grass (`Beat_0x_01`, BeatOffset
    /// −16) starts 15 ticks after its half-beat, the castles' (`L_Castle_00/Beat_0x_00`, −10)
    /// 21 after theirs, Bowser 11 after a `Koopa` marker.
    #[test]
    fn owner_map_bounces_after_their_offsets() {
        let Some(ui) = crate::testing::smm1() else {
            return;
        };
        let Some(mut l) = ui.layout("Nwk_WorldMapEsy_00") else {
            return;
        };
        let castles = ["L_Castle_00".to_string(), "L_Castle_01".to_string()];
        let mut b = BeatBounce::new(&mut l, &castles);
        assert!(!b.is_empty());
        let beat = |x, y| BeatEvents {
            beat: Some((x, y)),
            marks: Vec::new(),
        };
        b.step(&mut l, &beat(2, 1), 0);
        for t in 1..14 {
            b.step(&mut l, &BeatEvents::default(), t);
        }
        assert!(!l.is_playing("Beat_02_01"));
        b.step(&mut l, &BeatEvents::default(), 14);
        assert!(l.is_playing("Beat_02_01"), "15 ticks after");
        assert!(!l.is_playing("Beat_01_01"));
        b.step(&mut l, &beat(1, 0), 20);
        for t in 21..41 {
            b.step(&mut l, &BeatEvents::default(), t);
        }
        assert!(l.is_playing("L_Castle_00/Beat_01_00"), "21 ticks after");
        let koopa = BeatEvents {
            beat: None,
            marks: vec!["Koopa".into()],
        };
        b.step(&mut l, &koopa, 50);
        for t in 51..61 {
            b.step(&mut l, &BeatEvents::default(), t);
        }
        assert!(l.is_playing("L_Castle_01/Anm_Koopa_00"), "11 ticks after");
    }
}
