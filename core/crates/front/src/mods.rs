//! The mod options (smm2recomp's mods, SMCraft's SMM1 extras): what each one is, where it is
//! kept and how it reaches the part of the game it changes. The Mods page (`mods_page`) lists
//! them; the hosts and the other screens read them from here.
//!
//! Every option is a small text value kept through [`Host::store_value`]: `mods.<name>`, except
//! Course World's source, which is the hosts' own `course_world` value. Engine options go to the
//! sim through [`Host::set_engine_option`] (their names are `mmk_sim::Options::NAMES`), at start
//! and when they change; the front end's own ones are read where they act (quick level return
//! and the SMW goal wipe in `play`, Challenge Modes in Course World); the Course Maker's
//! ([`overlap_pipes`]) when it opens (`maker::MakerFlow::new`).
//!
//! Options that change how a course plays ([`ModOption::changes_play`], the sim's
//! `Options::affects_play`) stay off in online play, and courses cleared with them send no
//! record or replay: the hosts enforce that (the shell's `Session::go_online`, the replay
//! recorders); the page says so.

use crate::challenge::Presentation;
use crate::host::Host;

/// Where an option acts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// An engine option (`mmk_sim::Options`), by its name.
    Engine(&'static str),
    /// Read by the front end's screens.
    Front,
    /// Read by the Course Maker.
    Editor,
    /// The 100 Mario Challenge's screens (the front end's start options).
    Hundred,
    /// Course World's source (the host's backend).
    CourseWorld,
}

/// One option of the Mods page.
#[derive(Clone, Copy, Debug)]
pub struct ModOption {
    /// The kept value's key.
    pub key: &'static str,
    pub target: Target,
    /// Short name (the page's label).
    pub label: &'static str,
    /// What it does, in a sentence or two.
    pub about: &'static str,
    /// The values: (kept text, shown text). Empty: the costume list (see [`ModOption::choices`]).
    pub values: &'static [(&'static str, &'static str)],
    /// The kept text when nothing is kept.
    pub default: &'static str,
    /// It changes how courses play (off online and in replays).
    pub changes_play: bool,
}

const OFF_ON: &[(&str, &str)] = &[("0", "Off"), ("1", "On")];
const ON_OFF: &[(&str, &str)] = &[("1", "On"), ("0", "Off")];

/// The key of Course World's source (the hosts read it at start).
pub const COURSE_WORLD_KEY: &str = "course_world";
/// The key of the 100 Mario Challenge's screens.
pub const HUNDRED_SCREENS_KEY: &str = "mods.hundred_screens";
/// Course World's Endless Challenge button opens Challenge Modes ("1", default) or SMM2's own
/// Endless Challenge only ("0").
pub const CHALLENGE_MODES_KEY: &str = "mods.challenge_modes";
/// The mode Challenge Modes opens on: the one used last (`hundred` or `endless`).
pub const CHALLENGE_MODE_KEY: &str = "mods.challenge_mode";
/// The Course Maker's overlapping pipes.
pub const OVERLAP_PIPES_KEY: &str = "mods.smm1_overlap_pipes";

/// The pages, each a list of [`OPTIONS`] indices.
pub const PAGES: [(&str, &[usize]); 2] = [
    ("Super Mario Maker extras", &[0, 1, 2, 3, 4, 5]),
    ("Screens and Course World", &[6, 7, 8, 9, 10]),
];

pub const OPTIONS: [ModOption; 11] = [
    ModOption {
        key: "mods.smm1_shell_kicks",
        target: Target::Engine("smm1_shell_kicks"),
        label: "Shell jumps (SMM1)",
        about: "Kick a shell in mid-air and jump off a shell you just kicked, the way the \
                first Super Mario Maker allowed.",
        values: OFF_ON,
        default: "0",
        changes_play: true,
    },
    ModOption {
        key: "mods.smm1_layering",
        target: Target::Engine("smm1_layering"),
        label: "Draw order (SMM1)",
        about: "Objects drawn in the first game's order: a player going through a pipe passes \
                between blocks and pipes, coins sit behind blocks, tracks in front of pipes.",
        values: OFF_ON,
        default: "0",
        changes_play: false,
    },
    ModOption {
        key: "mods.smb1_mushroom",
        target: Target::Engine("smb1_mushroom"),
        label: "Super Mushrooms (SMB1 style)",
        about: "In Super Mario Bros. style courses Super Mushrooms can become Mystery \
                Mushrooms (a costume) or Weird Mushrooms (Lanky Mario). Needs the SMM1 import.",
        values: &[
            ("0", "Super Mushroom"),
            ("1", "Mystery Mushroom"),
            ("2", "Weird Mushroom"),
        ],
        default: "0",
        changes_play: true,
    },
    ModOption {
        key: "mods.costume",
        target: Target::Engine("costume"),
        label: "Mystery Mushroom costume",
        about: "The costume a Mystery Mushroom gives: a different one each time, or always the \
                same one.",
        values: &[],
        default: "-1",
        changes_play: false,
    },
    ModOption {
        key: "mods.muncher_smm1_hitbox",
        target: Target::Engine("muncher_smm1_hitbox"),
        label: "Muncher hit box (SMM1)",
        about: "Munchers hurt the player only inside a slightly smaller box, so a one-tile gap \
                between two of them can be passed, as in the first game.",
        values: OFF_ON,
        default: "0",
        changes_play: true,
    },
    ModOption {
        key: OVERLAP_PIPES_KEY,
        target: Target::Editor,
        label: "Overlapping pipes (SMM1)",
        about: "Course Maker: a pipe may be placed across another pipe.",
        values: OFF_ON,
        default: "0",
        changes_play: false,
    },
    ModOption {
        key: crate::play::QUICK_RETURN_KEY,
        target: Target::Front,
        label: "Quick level return",
        about: "After a miss the course is back in well under a second: the miss plays fast \
                instead of in full.",
        values: OFF_ON,
        default: "0",
        changes_play: false,
    },
    ModOption {
        key: crate::play::SMW_GOAL_WIPE_KEY,
        target: Target::Front,
        label: "SMW goal wipe",
        about: "Super Mario World style courses end with that game's closing circle and its \
                sound before the clear screen.",
        values: ON_OFF,
        default: "1",
        changes_play: false,
    },
    ModOption {
        key: HUNDRED_SCREENS_KEY,
        target: Target::Hundred,
        label: "100 Mario Challenge screens",
        about: "The 100 Mario Challenge's menus, map and course cards: SMM2's Endless \
                Challenge look, or the first game's own (needs the SMM1 import).",
        values: &[("smm2", "SMM2 style"), ("smm1", "SMM1 (original)")],
        default: "smm2",
        changes_play: false,
    },
    ModOption {
        key: CHALLENGE_MODES_KEY,
        target: Target::Front,
        label: "Course World's Endless button",
        about: "Challenge Modes: the button opens the mode used last; ZL / ZR (or a tap on the \
                title bar) switches between the 100 Mario Challenge and the Endless Challenge.",
        values: &[("1", "Challenge Modes"), ("0", "Endless Challenge")],
        default: "1",
        changes_play: false,
    },
    ModOption {
        key: COURSE_WORLD_KEY,
        target: Target::CourseWorld,
        label: "Course World courses",
        about: "Where Course World finds courses: TGRCode's archive of SMM2's courses \
                (read-only), your own course server, or only the courses on this device.",
        values: &[
            ("tgrcode", "TGRCode"),
            ("server", "Course server"),
            ("library", "My courses only"),
        ],
        default: "tgrcode",
        changes_play: false,
    },
];

/// What a line says about an option that changes play.
pub const CHANGES_PLAY_NOTE: &str = "Changes play: it stays off online, and clears made with \
                                     it send no record or replay.";

impl ModOption {
    /// The values to choose from: (kept text, shown text).
    pub fn choices(&self, costumes: &[(u16, String)]) -> Vec<(String, String)> {
        if self.values.is_empty() {
            let mut v = vec![("-1".to_string(), "Random".to_string())];
            v.extend(costumes.iter().map(|(id, n)| (id.to_string(), n.clone())));
            return v;
        }
        self.values
            .iter()
            .map(|(k, s)| (k.to_string(), s.to_string()))
            .collect()
    }

    /// The kept value (the default when nothing is kept).
    pub fn get(&self, host: &mut dyn Host) -> String {
        if self.target == Target::CourseWorld {
            // the host's own setting, else the source it uses now
            return host
                .load_value(self.key)
                .map(|v| v.trim().trim_matches('"').to_string())
                .filter(|v| !v.is_empty())
                .unwrap_or_else(|| host.course_world().to_string());
        }
        host.load_value(self.key)
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| self.default.to_string())
    }

    /// Keep `value` and hand it on where it acts at once (engine options, Course World).
    pub fn set(&self, host: &mut dyn Host, value: &str) {
        match self.target {
            Target::CourseWorld => {
                host.store_value(self.key, &format!("\"{value}\""));
                host.set_course_world(value);
            }
            Target::Engine(name) => {
                host.store_value(self.key, value);
                if let Ok(v) = value.parse() {
                    host.set_engine_option(name, v);
                }
            }
            _ => host.store_value(self.key, value),
        }
    }
}

/// The option kept under `key`.
pub fn option(key: &str) -> Option<&'static ModOption> {
    OPTIONS.iter().find(|o| o.key == key)
}

/// Hand the kept engine options to the engine (at start). Options never set are left as the
/// host has them (its command line).
pub fn apply_engine(host: &mut dyn Host) {
    for o in &OPTIONS {
        if let Target::Engine(name) = o.target
            && let Some(v) = host.load_value(o.key)
            && let Ok(v) = v.trim().parse()
        {
            host.set_engine_option(name, v);
        }
    }
}

/// The kept 100 Mario screens, if one was chosen.
pub fn hundred_screens(host: &mut dyn Host) -> Option<Presentation> {
    match host.load_value(HUNDRED_SCREENS_KEY)?.trim() {
        "smm1" => Some(Presentation::Smm1),
        "smm2" => Some(Presentation::Smm2),
        _ => None,
    }
}

/// Course World's Endless Challenge button opens Challenge Modes (on unless turned off).
pub fn challenge_modes(host: &mut dyn Host) -> bool {
    host.load_value(CHALLENGE_MODES_KEY)
        .as_deref()
        .map(str::trim)
        != Some("0")
}

/// The Course Maker's SMM1 rule: a pipe may overlap another pipe (`Editor::smm1_overlap_pipes`).
pub fn overlap_pipes(host: &mut dyn Host) -> bool {
    host.load_value(OVERLAP_PIPES_KEY).as_deref().map(str::trim) == Some("1")
}

#[cfg(test)]
pub(crate) mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::host::{CourseEntry, CourseStatus, LoadedCourse, Source, WorldCourse};

    /// A host keeping values in memory and noting the engine options and Course World's source
    /// it is handed; its "engine" loads every course and clears nothing by itself.
    #[derive(Default)]
    pub struct MemHost {
        pub values: HashMap<String, String>,
        pub engine: Vec<(String, i32)>,
        pub course_world: String,
        pub library: Vec<CourseEntry>,
        pub loaded: Vec<Source>,
        pub status: CourseStatus,
        pub counters: Option<(u32, u32, u32)>,
    }

    impl Host for MemHost {
        fn library(&mut self) -> Vec<CourseEntry> {
            self.library.clone()
        }
        fn request_server_courses(&mut self) -> bool {
            false
        }
        fn poll_server_courses(&mut self) -> Option<Vec<WorldCourse>> {
            None
        }
        fn load_course(&mut self, s: &Source) -> Result<LoadedCourse, String> {
            self.loaded.push(s.clone());
            Ok(LoadedCourse::default())
        }
        fn title_courses(&mut self) -> usize {
            0
        }
        fn load_title_course(&mut self, _: usize) -> bool {
            false
        }
        fn unload_course(&mut self) {}
        fn step_course(&mut self, _: Option<u32>) {}
        fn course_status(&self) -> CourseStatus {
            self.status
        }
        fn restart_course(&mut self) {}
        fn set_character(&mut self, _: u32) {}
        fn set_run_counters(&mut self, score: u32, coins: u32, lives: u32) {
            self.counters = Some((score, coins, lives));
            self.status.lives = lives;
        }
        fn set_engine_option(&mut self, name: &str, value: i32) -> bool {
            self.engine.push((name.to_string(), value));
            true
        }
        fn set_course_world(&mut self, source: &str) -> bool {
            self.course_world = source.to_string();
            true
        }
        fn course_world(&self) -> &str {
            "tgrcode"
        }
        fn load_value(&mut self, key: &str) -> Option<String> {
            self.values.get(key).cloned()
        }
        fn store_value(&mut self, key: &str, value: &str) {
            self.values.insert(key.into(), value.into());
        }
        fn random(&mut self) -> u64 {
            7
        }
    }

    #[test]
    fn every_option_is_on_one_page_and_has_its_default() {
        let mut seen: Vec<usize> = PAGES.iter().flat_map(|p| p.1.iter().copied()).collect();
        seen.sort();
        assert_eq!(seen, (0..OPTIONS.len()).collect::<Vec<_>>());
        for o in &OPTIONS {
            let c = o.choices(&[]);
            assert!(c.iter().any(|(k, _)| k == o.default), "{}", o.key);
            assert!(o.about.len() < 200, "{}: keep it short", o.key);
        }
    }

    #[test]
    fn values_are_kept_and_engine_options_handed_on() {
        let mut h = MemHost::default();
        let kicks = option("mods.smm1_shell_kicks").unwrap();
        assert_eq!(kicks.get(&mut h), "0");
        kicks.set(&mut h, "1");
        assert_eq!(kicks.get(&mut h), "1");
        assert_eq!(h.engine, vec![("smm1_shell_kicks".to_string(), 1)]);
        h.engine.clear();
        apply_engine(&mut h);
        assert_eq!(h.engine, vec![("smm1_shell_kicks".to_string(), 1)]);
        let wipe = option(crate::play::SMW_GOAL_WIPE_KEY).unwrap();
        assert_eq!(wipe.get(&mut h), "1");
        assert!(challenge_modes(&mut h));
        assert!(!overlap_pipes(&mut h));
        let world = option(COURSE_WORLD_KEY).unwrap();
        world.set(&mut h, "library");
        assert_eq!(h.values.get(COURSE_WORLD_KEY).unwrap(), "\"library\"");
        assert_eq!(world.get(&mut h), "library");
        assert_eq!(h.course_world, "library");
        assert_eq!(hundred_screens(&mut h), None);
        option(HUNDRED_SCREENS_KEY).unwrap().set(&mut h, "smm1");
        assert_eq!(hundred_screens(&mut h), Some(Presentation::Smm1));
    }
}
