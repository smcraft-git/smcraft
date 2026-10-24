//! The placed sound effects' ids (research/specs/sound-effect-parts.md, private): a spot's,
//! a part's `sound_effect_id` and the header's `player_sound_effect` hold an effect's course id
//! 0–55. The maker offers 47 of them (by icon: their labels, pages and course icons); 21, 22,
//! 27, 28, 33, 34, 39, 40 and 55 have sound users but are never offered. What they play is
//! `mmk_sim::otoasobi`'s.

/// The effects the maker offers, in icon order (the order of the game's label table; the
/// window's pages group them, [`PAGES`]): their course ids [obs 0x71023ca370].
pub const ICON_IDS: [u8; 47] = [
    0, 11, 6, 25, 3, 14, 7, 23, 24, 26, 1, 12, 2, 13, 17, 5, 29, 30, 31, 32, 8, 18, 4, 15, 35, 36,
    37, 38, 16, 41, 42, 43, 44, 45, 9, 19, 10, 20, 48, 49, 50, 51, 52, 53, 54, 46, 47,
];

/// The icons' labels, in icon order: the `SoundIconName` message labels and the palette
/// pictures `Lyt_E_P_SE_<label>` [obs: the label table built in _INIT_1888 0x7100be56e0].
pub const ICON_LABELS: [&str; 47] = [
    "Shock",
    "Scream",
    "Laughter",
    "LoudLaughter",
    "Applause",
    "Cheer",
    "Baby",
    "Cracker",
    "Boo",
    "Incident",
    "Clatter",
    "Drama",
    "Kick",
    "Jump",
    "Honk",
    "Punch",
    "Whoopee",
    "Focus",
    "Glitch",
    "Uproar",
    "Ding",
    "Bzzzt",
    "Glory",
    "Doom",
    "Discord",
    "Halo",
    "Admiration",
    "Anguish",
    "Fireworks",
    "Audience",
    "Scat",
    "Firecracker",
    "Ohayasi",
    "ElectricGuitar",
    "Boss",
    "Bonus",
    "Heartbeat",
    "Silence",
    "LastBoss",
    "Calm",
    "Unrest",
    "Mario00",
    "Mario01",
    "Mario02",
    "Mario03",
    "Filter",
    "SoundEffect",
];

/// The effect window's five categories (the ring's `Edit_RingCategory_01`), each one page: the
/// icons (indices into [`ICON_IDS`]) in their order. They are the rows of the course icons'
/// sheet `Otoasobi_CategoryPanel` (yellow, orange, green, purple, blue), which put Flash and the
/// two filters with the animations [obs data: the sheet's cells against the palette pictures].
pub const PAGES: [&[usize]; 5] = [
    &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9],
    &[10, 11, 12, 13, 14, 15, 16, 17, 18, 19],
    &[20, 21, 22, 23, 24, 26, 27],
    &[28, 29, 30, 31, 32, 33, 45, 46, 25],
    &[34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44],
];

/// The categories' names: `CategoryName_<key>` of `Edit_MainPage_00` (Feelings, Stingers,
/// Reactions, Animations, Music) [obs data].
pub const CATEGORY_KEYS: [&str; 5] = ["Emotion", "OneShot", "Eval", "Visual", "BGM"];

/// The icon (index into [`ICON_IDS`]) of course id `id`, None for an id the maker never offers.
pub fn icon(id: u8) -> Option<usize> {
    ICON_IDS.iter().position(|&i| i == id)
}

/// An id the game offers (and plays).
pub fn valid(id: u8) -> bool {
    icon(id).is_some()
}

/// The effect's message / picture label.
pub fn label(id: u8) -> Option<&'static str> {
    icon(id).map(|i| ICON_LABELS[i])
}

/// The page (0–4) the effect is on and its place there.
pub fn page_of(id: u8) -> Option<(usize, usize)> {
    let i = icon(id)?;
    PAGES
        .iter()
        .enumerate()
        .find_map(|(p, page)| page.iter().position(|&k| k == i).map(|at| (p, at)))
}

/// The effect's cells in `Otoasobi_CategoryPanel` (1536 × 1290, 128-px cells): its big icon at
/// (column, row) = (place on its page, page), its small framed one five rows lower (an 88-px
/// picture 20 px into the cell) [obs data].
pub fn sheet_cell(id: u8) -> Option<(usize, usize)> {
    page_of(id).map(|(p, at)| (at, p))
}
