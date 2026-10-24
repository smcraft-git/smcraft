//! The parts the maker offers, by the part window's four categories (Terrain, Items, Enemies,
//! Gizmos: `CategoryName_*` of `Edit_MainPage_00` [obs data]). Which part sits on which page and
//! in what order is built by the game's code from its editor part kinds and wasn't decoded
//! (research/specs/maker.md C3); this order is ours [inf], one entry per part, alternate forms
//! left to the part itself. Names come from the import's `PartsIconName` table by label
//! (`<style>_<label>` first, as the game's own per-style names).

use mmk_editor::rules;

/// The part window's categories, in tab order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Category {
    Terrain,
    Items,
    Enemies,
    Gizmos,
}

pub const CATEGORIES: [Category; 4] = [
    Category::Terrain,
    Category::Items,
    Category::Enemies,
    Category::Gizmos,
];

impl Category {
    /// The `CategoryName_<x>` message key.
    pub fn key(self) -> &'static str {
        match self {
            Category::Terrain => "Ground",
            Category::Items => "Item",
            Category::Enemies => "Enemy",
            Category::Gizmos => "Trap",
        }
    }

    pub fn index(self) -> usize {
        CATEGORIES.iter().position(|&c| c == self).unwrap_or(0)
    }
}

/// How a part is put into the course.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    /// Ground is painted tile by tile (drag paints).
    Ground,
    /// An ordinary part (`Editor::place`).
    Part(i16),
    /// A pipe, facing up.
    Pipe,
    /// A pair of doors / warp boxes: the second one goes a few tiles to the right [inf].
    Doors,
    WarpBoxes,
}

impl Tool {
    /// The course object id the tool puts down.
    pub fn part(self) -> i16 {
        match self {
            Tool::Ground => 7,
            Tool::Part(p) => p,
            Tool::Pipe => 9,
            Tool::Doors => 55,
            Tool::WarpBoxes => 97,
        }
    }

    /// Dragging puts down one per tile (ground, blocks, coins) rather than one per press.
    pub fn paints(self) -> bool {
        matches!(
            self,
            Tool::Ground | Tool::Part(4 | 5 | 6 | 8 | 21 | 22 | 23 | 29 | 63 | 92 | 100 | 108)
        )
    }
}

/// One part of the catalog.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Entry {
    pub tool: Tool,
    /// `PartsIconName` label.
    pub label: &'static str,
    pub category: Category,
}

const fn e(tool: Tool, label: &'static str, category: Category) -> Entry {
    Entry {
        tool,
        label,
        category,
    }
}

use Category::*;
use Tool::{Doors, Ground as G, Part as P, Pipe, WarpBoxes};

pub const ENTRIES: &[Entry] = &[
    // Terrain
    e(G, "Ground", Terrain),
    e(P(87), "slope_l30", Terrain),
    e(P(88), "slope_l45", Terrain),
    e(Pipe, "Dokan", Terrain),
    e(P(43), "Toge", Terrain),
    e(P(16), "GroundBox", Terrain),
    e(P(14), "GroundMushroom", Terrain),
    e(P(71), "GroundBox", Terrain),
    e(P(17), "Bridge", Terrain),
    e(P(4), "RengaBlock", Terrain),
    e(P(5), "HatenaBlock", Terrain),
    e(P(6), "HardBlock", Terrain),
    e(P(29), "ClearBlock", Terrain),
    e(P(21), "ChikuwaBlock", Terrain),
    e(P(23), "OnpuBlock_00", Terrain),
    e(P(22), "KumoBlock", Terrain),
    e(P(63), "IceBlock", Terrain),
    e(P(64), "TsutaBlock", Terrain),
    e(P(110), "TogeBlock", Terrain),
    // Items
    e(P(8), "Coin", Items),
    e(P(70), "10Coin", Items),
    e(P(92), "PinkCoin", Items),
    e(P(20), "SuperKinoko", Items),
    e(P(34), "FireFlower", Items),
    e(P(44), "SpecialItem", Items),
    e(P(35), "SuperStar", Items),
    e(P(33), "1upKinoko", Items),
    e(P(116), "SuperHammer", Items),
    e(P(45), "KutsuKuribo_00", Items),
    e(P(19), "PowBlock", Items),
    e(P(18), "PSwitch", Items),
    e(P(95), "Key", Items),
    e(P(10), "JumpStep", Items),
    // Enemies
    e(P(0), "Kuribo", Enemies),
    e(P(1), "Nokonoko", Enemies),
    e(P(2), "Pakkun_00", Enemies),
    e(P(3), "Bros", Enemies),
    e(P(12), "Dossun", Enemies),
    e(P(13), "KillerHoudai", Enemies),
    e(P(32), "MagnumKiller", Enemies),
    e(P(15), "Bombhei_00", Enemies),
    e(P(25), "Togezo_00", Enemies),
    e(P(28), "Met_00", Enemies),
    e(P(40), "TogeMet", Enemies),
    e(P(41), "Teresa_00", Enemies),
    e(P(46), "Karon", Enemies),
    e(P(103), "FishBone", Enemies),
    e(P(48), "Gesso_00", Enemies),
    e(P(56), "Pukupuku", Enemies),
    e(P(57), "BlackPakkun", Enemies),
    e(P(58), "Poo", Enemies),
    e(P(60), "Bubble", Enemies),
    e(P(61), "Wanwan_00", Enemies),
    e(P(39), "Kameck", Enemies),
    e(P(102), "ChoroPoo", Enemies),
    e(P(52), "Hanachan_00", Enemies),
    e(P(30), "Jugem", Enemies),
    e(P(42), "KoopaClown_00", Enemies),
    e(P(96), "Arihei_00", Enemies),
    e(P(50), "Hopper", Enemies),
    e(P(51), "Pyonchu", Enemies),
    e(P(65), "Hacchin", Enemies),
    e(P(114), "Fugumannen", Enemies),
    e(P(117), "Donketsu", Enemies),
    e(P(77), "Bunbun", Enemies),
    e(P(98), "KoopaJr", Enemies),
    e(P(62), "Koopa", Enemies),
    // Gizmos
    e(P(11), "Lift_00", Gizmos),
    e(P(36), "YouganLift_00", Gizmos),
    e(P(91), "Seesaw", Gizmos),
    e(P(24), "FireBar", Gizmos),
    e(P(54), "Burner", Gizmos),
    e(P(47), "Houdai", Gizmos),
    e(P(68), "Saw", Gizmos),
    e(P(82), "Marumaru", Gizmos),
    e(P(83), "TogeKonbo", Gizmos),
    e(P(76), "Tornado", Gizmos),
    e(P(105), "BurankoCrane", Gizmos),
    e(P(53), "BeltConveyor_00", Gizmos),
    e(P(84), "SnakeBlock_00", Gizmos),
    e(P(119), "BikkuriBlock", Gizmos),
    e(P(85), "OrbitBlock", Gizmos),
    e(P(99), "OnOffSwitch", Gizmos),
    e(P(100), "OnOffBlock", Gizmos),
    e(P(108), "Chikachika", Gizmos),
    e(P(113), "Trampoline", Gizmos),
    e(P(112), "WoodBox", Gizmos),
    e(P(106), "BellTree", Gizmos),
    e(Doors, "Door_00", Gizmos),
    e(WarpBoxes, "WarpBox_00", Gizmos),
    e(P(90), "MiddleFlag", Gizmos),
    e(P(66), "Yajirushi", Gizmos),
    e(P(67), "Hanatari", Gizmos),
    e(P(104), "SunMoon", Gizmos),
    e(P(72), "KoopaCar", Gizmos),
];

/// The entries of `category` that game style `style` has.
pub fn of(category: Category, style: &str) -> Vec<Entry> {
    ENTRIES
        .iter()
        .filter(|x| x.category == category && available(x, style))
        .copied()
        .collect()
}

/// The style has the part (and, for the two semisolid entries, it is that style's one).
pub fn available(x: &Entry, style: &str) -> bool {
    let p = x.tool.part();
    p == 7 || rules::style_allows(style, p)
}

/// The entry that puts down `tool`, if the catalog has it.
pub fn entry(tool: Tool) -> Option<Entry> {
    ENTRIES.iter().find(|x| x.tool == tool).copied()
}

/// A new course's palette [inf: SMM2's own wasn't read].
pub const DEFAULT_PALETTE: [Tool; 12] = [
    G,
    P(4),
    P(5),
    P(6),
    P(8),
    P(0),
    P(1),
    Pipe,
    P(20),
    P(16),
    P(10),
    P(18),
];

/// Pages of at most `per` entries.
pub fn pages(entries: &[Entry], per: usize) -> Vec<Vec<Entry>> {
    entries.chunks(per.max(1)).map(|c| c.to_vec()).collect()
}

/// The style's part name labels: `<S>_<label>` first (`MW_Kuribo` is a Galoomba), then the
/// plain label; `SpecialItem` only exists per style.
pub fn labels(x: &Entry, style: &str) -> [String; 2] {
    [format!("{style}_{}", x.label), x.label.to_string()]
}

/// The Course Maker's sound user of course object `part`: `<style>_Edit<this>` (the parts'
/// edit sounds, research/specs/maker.md C9). Matched from the database's user names to the
/// object ids [inf for the less obvious ones: one-way wall, arrow sign, dotted-line and
/// blinking blocks]; a part without its own user takes the ground's (the editor's common keys
/// — `set`, `drag`, `eraser`, `invalid`, `partsMax`, `Block_in` … — sound the same for every
/// part, only the voices differ).
pub fn edit_user(part: i16) -> &'static str {
    match part {
        0 => "Kuribo",
        1 => "Nokonoko",
        2 => "Pakkun",
        3 => "HammerBros",
        4 => "RengaBlock",
        5 => "HatenaBlock",
        6 => "HardBlock",
        8 => "Coin",
        9 | 93 => "Dokan",
        10 => "JumpStep",
        11 => "Lift",
        12 => "Dossun",
        13 => "KillerHoudai",
        14 => "GroundMushroom",
        15 => "Bombhei",
        16 | 71 => "GroundBox",
        17 => "Bridge",
        18 => "PSwitch",
        19 => "PowBlock",
        20 => "SuperKinoko",
        21 => "ChikuwaBlock",
        22 => "KumoBlock",
        23 => "OnpuBlock",
        24 => "FireBar",
        25 => "Togezo",
        26 => "GroundGoal",
        27 => "GoalPole",
        28 => "Met",
        29 => "ClearBlock",
        30 => "Jugem",
        31 => "JugemCloud",
        32 => "MagnumKiller",
        33 => "1upKinoko",
        34 => "FireFlower",
        35 => "SuperStar",
        36 => "YouganLift",
        37 => "GroundStart",
        38 => "StartSignBoard",
        39 => "Kameck",
        40 => "Togemet",
        41 => "Teresa",
        42 => "KoopaClown",
        43 | 110 => "Toge",
        44 => "SpecialItem",
        45 => "KutsuKuribo",
        46 => "Karon",
        47 => "SenkanHoudai",
        48 => "Gesso",
        49 => "CastleBridge",
        50 => "Hopper",
        51 => "Pyonchu",
        52 => "Hanachan",
        53 | 94 => "BeltConveyor",
        54 => "Burner",
        55 => "Door",
        56 => "Pukupuku",
        57 => "BlackPakkun",
        58 => "Poo",
        59 => "Rail",
        60 => "Bubble",
        61 => "Wanwan",
        62 => "Koopa",
        63 => "IceBlock",
        64 => "Tsuta",
        65 => "Hacchin",
        66 => "AirSignBoard",
        67 => "HalfHitWall",
        68 => "Saw",
        69 => "Player",
        70 => "Coin10",
        72 => "KoopaCar",
        74 => "Gabon",
        76 => "Tornado",
        77 => "Bunbun",
        78 => "Sambo",
        79 => "PBlock",
        80 => "DashBlock",
        81 => "KinokoFunny",
        82 => "Marumaru",
        83 => "TogeKonbo",
        84 => "SnakeBlock",
        85 => "OrbitBlock",
        86 => "UganFish",
        89 => "AutoScrollCamera",
        90 => "CheckpointFlag",
        91 => "Seesaw",
        92 => "PinkCoin",
        95 => "Key",
        96 => "Arihei",
        97 => "WarpBox",
        98 => "KoopaJr",
        99 => "OnOffSwitch",
        100 | 108 => "RedBluePanel",
        101 => "LavaEditor",
        102 => "Choropoo",
        103 => "FishBone",
        104 => "SunAndMoon",
        105 => "SwingCrane",
        106 => "BellTree",
        107 => "NobinobiPakkun",
        111 => "MechaKoopa",
        112 => "WoodBox",
        113 => "KinokoTrampoline",
        114 => "Fugumannen",
        116 => "SuperHammer",
        117 => "Donketsu",
        119 => "BikkuriBlock",
        120 => "Lemmy",
        121 => "Morton",
        122 => "Larry",
        123 => "Wendy",
        124 => "Iggy",
        125 => "Roy",
        126 => "Ludwig",
        127 => "HeaddressHoudai",
        128 => "HeaddressPropeller",
        129 => "HeaddressKuribo",
        130 => "HeaddressKiller",
        131 => "HeaddressRedPow",
        132 => "OnOffTrampoline",
        _ => "Ground",
    }
}

/// The edit sound key of part `item` going into placed part `into` (research/specs/maker.md
/// C9): into a pipe, a Bill Blaster, a Lakitu, a Clown Car, a Lakitu's Cloud, else a block
/// (and the other containers); a key into a part has its own.
pub fn put_in_key(into: i16, item: i16) -> &'static str {
    match (into, item) {
        (9, _) => "Dokan_in",
        (_, 95) => "Key_In",
        (13 | 32, _) => "Killer_in",
        (30, _) => "Jugem_in",
        (42, _) => "Clown_in",
        (31, _) => "Cloud_in",
        _ => "Block_in",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_style_has_parts_in_every_category_and_the_default_palette() {
        for style in ["M1", "M3", "MW", "WU", "3W"] {
            for c in CATEGORIES {
                assert!(of(c, style).len() >= 8, "{style} {c:?}");
            }
            // the semisolid of the style: 16 outside 3D World, 71 in it
            let terrain = of(Category::Terrain, style);
            assert_eq!(
                terrain.iter().any(|x| x.tool == Tool::Part(71)),
                style == "3W"
            );
        }
        for t in DEFAULT_PALETTE {
            assert!(entry(t).is_some(), "{t:?}");
        }
        // every catalog part has its own edit sound user (ground and slopes: the ground's)
        for x in ENTRIES {
            let p = x.tool.part();
            assert!(
                matches!(p, 7 | 87 | 88) || edit_user(p) != "Ground",
                "{x:?}"
            );
        }
        let p = pages(&of(Category::Enemies, "M1"), 13);
        assert!(p.len() >= 2 && p.iter().all(|q| q.len() <= 13));
    }
}
