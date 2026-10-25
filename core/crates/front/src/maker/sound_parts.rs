//! The Course Maker's sound effects (research/specs/sound-effect-parts.md, private): the palette
//! bar's sound button (`L_PaletteSet_00/L_SoundBtn_00`) turns the palette into the effects'
//! (their pictures `Lyt_E_P_SE_<label>`, names from `SoundIconName`), the search button opens
//! the effect window (the ring with the five sound categories, `L_Category_01`: Feelings,
//! Stingers, Reactions, Animations, Music, over the game's eight pages), and a press puts the
//! effect on the tile: on Mario, on a part that can hold one, else on the empty tile
//! (`mmk_editor::sound_effects`). The eraser (or the back button) takes it off again.
//!
//! The course shows every effect with its icon from the sheet `Otoasobi_CategoryPanel`: a
//! spot's big icon on its tile, a part's (and Mario's) small framed icon at the part's top
//! right [inf: which size goes where; capture `maker-sound-icons`].
//!
//! Each effect sounds its Course Maker user's `cue` (`EditOtoasobi<id>`) when it is chosen (the
//! sound palette's choice, 0x71018e6710; a new selection in the window, 0x71018e0c40) or put
//! down (0x7100f0eaa0: the effect icon's put-down virtual, after the parts' common one
//! 0x7100e92b10, unless the icon's +0x52 or its cursor's +0xf47e flag is set) [obs v3.0.3]; the
//! maker's calls run the effect without its picture.
//! The sound button's balloon names the palette it switches to (`Mode_Disp_Sound` /
//! `Mode_Disp_Parts`, 0x71018e2fe0) [obs v3.0.3].

use mmk_editor::EditError;
use mmk_editor::sound_effects::SoundTarget;
use mmk_formats::smm2::sound_effects::{
    CATEGORY_KEYS, ICON_IDS, RING_CATEGORY_PAGE, RING_PAGES, label, ring_category, ring_place,
    sheet_cell,
};
use mmk_ui::{Quad, TEXTURE_NONE};

use super::view::{self, quad};
use super::{Buttons, MakerFlow, Mode, PAGE, RING, Ring, SLOTS, hit, keys, slot_part};
use crate::sound::Event;

/// The course icons' sheet (in the import's `EditModel` group).
pub const SHEET: &str = "Otoasobi_CategoryPanel";
const SHEET_W: f32 = 1536.0;
const SHEET_H: f32 = 1290.0;
const CELL: f32 = 128.0;
/// The small framed icons: five rows lower, an 88-px picture 20 px into its cell.
const SMALL_ROW: usize = 5;
const SMALL_INSET: f32 = 20.0;
const SMALL: f32 = 88.0;

/// The maker's effect tools.
#[derive(Clone, Debug)]
pub struct SoundTools {
    /// The palette shows the effects (the sound button is on).
    pub on: bool,
    /// The palette's effects (course ids), most recent first, and the selected slot.
    pub palette: Vec<u8>,
    pub slot: usize,
    pub(super) ring: Ring,
    /// Each window page's selection, kept while the maker is open.
    pub(super) ring_sel: [usize; RING_PAGES.len()],
    /// Effects to sound this tick (their `EditOtoasobi<id>` cue).
    pub previews: Vec<u8>,
}

impl Default for SoundTools {
    /// The palette starts with the window's first twelve effects [inf: the game's start wasn't
    /// read; 0x71019227b0's lists are the window's pages, not a palette].
    fn default() -> Self {
        Self {
            on: false,
            palette: RING_PAGES
                .iter()
                .flat_map(|p| p.iter())
                .take(SLOTS)
                .map(|&i| ICON_IDS[i])
                .collect(),
            slot: 0,
            ring: Ring::default(),
            ring_sel: [0; RING_PAGES.len()],
            previews: Vec::new(),
        }
    }
}

/// The palette picture of an effect.
pub fn picture(id: u8) -> Option<String> {
    label(id).map(|l| format!("Lyt_E_P_SE_{l}"))
}

/// The Course Maker's sound user of an effect.
pub fn edit_user(id: u8) -> String {
    format!("EditOtoasobi{id:02}")
}

impl MakerFlow {
    /// The selected effect.
    pub fn sound_effect(&self) -> Option<u8> {
        self.se.palette.get(self.se.slot).copied()
    }

    /// The sound button: the palette between the parts and the effects.
    pub(super) fn toggle_sound_mode(&mut self) {
        self.se.on = !self.se.on;
        self.eraser = false;
        if let Some(p) = &mut self.page {
            let b = "L_PaletteSet_00/L_SoundBtn_00";
            p.play(&format!("{b}/Decide"), self.tick);
            p.hold(&format!("{b}/Toggle"), if self.se.on { 1.0 } else { 0.0 });
            if self.se.on {
                p.play(&format!("{b}/ButtonLoop"), self.tick);
            } else {
                p.stop(&format!("{b}/ButtonLoop"));
            }
            p.hold("L_RightParts_00/L_EraserBtn_00/Toggle", 0.0);
        }
        self.ui
            .sound_button(PAGE, "L_PaletteSet_00/L_SoundBtn_00", Event::Decide);
        if self.se.on
            && let Some(id) = self.sound_effect()
        {
            self.se.previews.push(id);
        }
        self.update_page();
    }

    /// The palette slots' effect pictures (from `update_page` while the sound button is on).
    pub(super) fn layout_sound_slots(&mut self) {
        let (palette, slot, eraser) = (self.se.palette.clone(), self.se.slot, self.eraser);
        let Some(p) = &mut self.page else { return };
        for i in 0..SLOTS {
            let part = format!("L_PaletteSet_00/{}", slot_part(i));
            let icon = format!("{part}/P_Icon_00");
            match palette.get(i).and_then(|&id| picture(id)) {
                Some(name) => {
                    p.show(&part, true);
                    p.hold(&format!("{part}/Invisible"), 0.0);
                    p.hold(
                        &format!("{part}/Check"),
                        if i == slot && !eraser { 1.0 } else { 0.0 },
                    );
                    p.hold(&format!("{part}/Select"), if i == slot { 2.0 } else { 0.0 });
                    p.show(&icon, true);
                    p.image(&icon, view::GROUP, &name);
                }
                None => {
                    p.hold(&format!("{part}/Invisible"), 1.0);
                    p.show(&icon, false);
                }
            }
        }
    }

    pub(super) fn select_sound_slot(&mut self, i: usize) {
        if i >= self.se.palette.len() {
            return;
        }
        if i != self.se.slot {
            self.ui.sound_button(
                PAGE,
                &format!("L_PaletteSet_00/{}", slot_part(i)),
                Event::Select,
            );
        }
        self.se.slot = i;
        self.eraser = false;
        if let Some(id) = self.sound_effect() {
            self.se.previews.push(id);
        }
        if let Some(p) = &mut self.page {
            p.hold("L_RightParts_00/L_EraserBtn_00/Toggle", 0.0);
        }
        self.update_page();
    }

    /// The effect's name (`SoundIconName`).
    pub fn sound_effect_name(&self, id: u8) -> String {
        label(id)
            .and_then(|l| self.ui.message("SoundIconName", l))
            .unwrap_or_else(|| label(id).unwrap_or("?").to_string())
    }

    /// A press on the course in sound mode: the effect onto the tile.
    pub(super) fn press_place_sound(&mut self, cx: &mut crate::Cx) {
        let (tx, ty) = self.cur;
        let Some(id) = self.sound_effect() else {
            return;
        };
        self.editor.checkpoint();
        match self.editor.put_sound_effect(id, tx, ty) {
            Ok(_) => {
                self.se.previews.push(id);
                self.changed();
            }
            Err(e) => {
                self.editor.discard_checkpoint();
                self.refuse_sound(cx, &e);
            }
        }
    }

    fn refuse_sound(&mut self, cx: &mut crate::Cx, e: &EditError) {
        super::refuse(cx, &self.ui, e);
        let part = match self.editor.object_at(self.cur.0, self.cur.1) {
            Some(i) => self.editor.object(i).map_or(7, |o| o.id),
            None => 7,
        };
        self.sound(part, "invalid");
    }

    /// The eraser in sound mode: the effect off the tile.
    pub(super) fn erase_sound_here(&mut self) -> bool {
        let (tx, ty) = self.cur;
        let Some((_, at)) = self.editor.sound_effect_at(tx, ty) else {
            return false;
        };
        let part = match at {
            SoundTarget::Part(i) => self.editor.object(i).map_or(7, |o| o.id),
            _ => 7,
        };
        // (the erase press keeps one checkpoint, as `erase_here`)
        if !self.editor.erase_sound_effect(tx, ty) {
            return false;
        }
        self.sound(part, "eraser");
        if let Some(super::Drag::Erase { changed }) = &mut self.drag {
            *changed = true;
        }
        self.changed();
        true
    }

    // ------------------------------------------------------------ the effect window

    /// The window's effects on ring page `page`.
    fn ring_page(page: usize) -> Vec<u8> {
        RING_PAGES
            .get(page)
            .map(|p| p.iter().map(|&i| ICON_IDS[i]).collect())
            .unwrap_or_default()
    }

    pub(super) fn open_sound_ring(&mut self) {
        // on the selected effect's page, else the first; each page keeps its own selection
        // (the page's +0x16b8, by page +0x16a8) [obs v3.0.3 0x71018e4960]
        let mut ring = Ring::default();
        if let Some((pg, at)) = self.sound_effect().and_then(ring_place) {
            self.se.ring_sel[pg] = at;
            ring.page = pg;
        }
        ring.category = ring_category(ring.page);
        ring.sel = self.se.ring_sel[ring.page];
        self.se.ring = ring;
        self.mode = Mode::Ring;
        self.ui
            .sound_button(PAGE, "L_PaletteSet_00/L_SearchBtn_00", Event::Decide);
        if let Some(p) = &mut self.page {
            p.show("L_RingMenu_00", true);
            p.show("L_RingMenu_00/PF_Base_00", false);
            p.stop("L_RingMenu_00/Out");
            p.play("L_RingMenu_00/In", self.tick);
            p.stop("L_PaletteSet_00/In");
            p.play("L_PaletteSet_00/Out", self.tick);
        }
        self.layout_sound_ring(true);
    }

    /// Show sound category `c` in the open window, on its first page (captures).
    pub(super) fn se_ring_category(&mut self, c: usize) {
        self.se_ring_page(
            RING_CATEGORY_PAGE[c.min(RING_CATEGORY_PAGE.len() - 1)],
            true,
        );
    }

    /// Ring page `page` (its remembered selection), `change` when the category changed.
    fn se_ring_page(&mut self, page: usize, change: bool) {
        let page = page.min(RING_PAGES.len() - 1);
        self.se.ring = Ring {
            category: ring_category(page),
            page,
            sel: self.se.ring_sel[page],
        };
        self.layout_sound_ring(change);
    }

    /// A new selection in the window sounds its effect (0x71018e0c40: the effect's play call
    /// with trigger kind 3 at the view's centre when the page's selection changes) [obs v3.0.3].
    fn se_ring_select(&mut self, sel: usize) {
        let page = self.se.ring.page;
        if sel != self.se.ring.sel
            && let Some(&id) = Self::ring_page(page).get(sel)
        {
            self.se.previews.push(id);
        }
        self.se.ring.sel = sel;
        self.se.ring_sel[page] = sel;
    }

    /// The ring with the current page: its effects' pictures, the selection, the five-tab
    /// category bar (`L_Category_01`) with the category's page dots [obs data]. The ring's
    /// `Category` colour is frame 4 + the category (0x71018e4960: pages 0–1 frame 4, then
    /// 0x71023d8620's 5, 5, 6, 7, 8, 8) [obs v3.0.3].
    pub(super) fn layout_sound_ring(&mut self, change: bool) {
        let pg = self.se.ring.page.min(RING_PAGES.len() - 1);
        let cat = ring_category(pg);
        self.se.ring.category = cat;
        let page = Self::ring_page(pg);
        let n = page.len().max(1);
        self.se.ring.sel = self.se.ring.sel.min(n - 1);
        let sel = self.se.ring.sel;
        // the category's pages, and which of them this is
        let first = RING_CATEGORY_PAGE[cat];
        let pages = RING_CATEGORY_PAGE
            .get(cat + 1)
            .copied()
            .unwrap_or(RING_PAGES.len())
            - first;
        let name = page.get(sel).map(|&id| self.sound_effect_name(id));
        let title = self
            .ui
            .message(PAGE, &format!("CategoryName_{}", CATEGORY_KEYS[cat]));
        let tick = self.tick;
        let Some(p) = &mut self.page else { return };
        let set = "L_RingMenu_00/L_RingIconSet_00";
        p.hold(&format!("{set}/Type"), (n.max(2) - 1) as f32);
        p.hold(&format!("{set}/Select"), sel as f32);
        p.hold(&format!("{set}/Category"), (4 + cat) as f32);
        for k in 0..RING {
            let icon = format!("{set}/L_RingIcon_{:02}", k + 1);
            match page.get(k).and_then(|&id| picture(id)) {
                Some(name) => {
                    p.visible(&icon, None);
                    let pic = format!("{icon}/P_Dummy_00");
                    p.show(&pic, true);
                    p.image(&pic, view::GROUP, &name);
                    p.show(&format!("{icon}/L_NewIcon_00"), false);
                }
                None => p.show(&icon, false),
            }
        }
        let centre = format!("{set}/L_RingIcon_00");
        p.show(&format!("{centre}/L_NewIcon_00"), false);
        match page.get(sel).and_then(|&id| picture(id)) {
            Some(name) => {
                p.show(&format!("{centre}/P_Dummy_00"), true);
                p.image(&format!("{centre}/P_Dummy_00"), view::GROUP, &name);
            }
            None => p.show(&format!("{centre}/P_Dummy_00"), false),
        }
        p.show("L_RingMenu_00/L_Category_00", false);
        p.show("L_RingMenu_00/L_Category_01", true);
        let bar = "L_RingMenu_00/L_Category_01";
        for c in 0..CATEGORY_KEYS.len() {
            let a = format!("{bar}/ChangeCategory{c:02}");
            if c == cat {
                if change {
                    p.play(&a, tick);
                } else {
                    p.hold_end(&a);
                }
            } else {
                p.stop(&a);
            }
        }
        p.hold(&format!("{bar}/PageVolume"), (pages.max(1) - 1) as f32);
        for d in 0..6 {
            let dot = format!("{bar}/P_Page_{d:02}");
            p.show(&dot, d < pages && pages > 1);
            p.set_pane(
                &dot,
                mmk_ui::PaneOverride {
                    scale: Some(if d == pg - first {
                        [1.0, 1.0]
                    } else {
                        [0.6, 0.6]
                    }),
                    ..Default::default()
                },
            );
        }
        if let Some(t) = title {
            p.text(&format!("{bar}/T_Category_00"), &t);
        }
        let balloon = "L_RingMenu_00/L_Balloon_00";
        p.show(&format!("{balloon}/T_Balloon_01"), false);
        if let Some(n) = name {
            p.text(&format!("{balloon}/T_Balloon_00"), &n);
        }
    }

    pub(super) fn sound_ring_tick(&mut self, pressed: Buttons, keys: u16, click: Option<[f32; 2]>) {
        let n = Self::ring_page(self.se.ring.page).len().max(1);
        if keys & keys::PARTS != 0 || pressed.has(Buttons::BACK) {
            self.close_ring();
            return;
        }
        if let Some(p) = click {
            self.sound_ring_click(p);
            return;
        }
        if pressed.has(Buttons::RIGHT) || pressed.has(Buttons::LEFT) {
            let step = if pressed.has(Buttons::RIGHT) {
                1
            } else {
                n - 1
            };
            self.se_ring_select((self.se.ring.sel + step) % n);
            self.ui.sound_key(PAGE, "p_MoveRightRingMenu", &[]);
            self.layout_sound_ring(false);
        } else if pressed.has(Buttons::UP) || pressed.has(Buttons::DOWN) {
            // page by page, round from the last to the first, through the categories
            let k = RING_PAGES.len();
            let pg = self.se.ring.page;
            let next = if pressed.has(Buttons::DOWN) {
                (pg + 1) % k
            } else {
                (pg + k - 1) % k
            };
            let change = ring_category(next) != ring_category(pg);
            self.ui.sound_key(PAGE, "p_MoveLeftRingMenu", &[]);
            self.se_ring_page(next, change);
        } else if pressed.has(Buttons::CONFIRM) {
            self.choose_sound_ring();
        }
    }

    fn sound_ring_click(&mut self, p: [f32; 2]) {
        let tick = self.tick;
        let Some(page) = &mut self.page else { return };
        let set = "L_RingMenu_00/L_RingIconSet_00";
        let icons: Vec<String> = (0..RING)
            .map(|k| format!("{set}/L_RingIcon_{:02}/W_Square_00", k + 1))
            .collect();
        if let Some(k) = hit(page, tick, &icons, p) {
            self.se_ring_select(k);
            self.layout_sound_ring(false);
            self.choose_sound_ring();
            return;
        }
        let tabs: Vec<String> = (0..CATEGORY_KEYS.len())
            .map(|c| format!("L_RingMenu_00/L_Category_01/L_BdBase_{c:02}"))
            .collect();
        if let Some(c) = hit(page, tick, &tabs, p) {
            if c != self.se.ring.category {
                self.se_ring_category(c);
            }
            return;
        }
        let close = ["L_RingMenu_00/L_CloseBtn_00".to_string()];
        if hit(page, tick, &close, p).is_some() {
            self.close_ring();
            return;
        }
        let sides = [
            "L_RingMenu_00/L_BdRingIconSet_01".to_string(),
            "L_RingMenu_00/L_BdRingIconSet_02".to_string(),
        ];
        if let Some(s) = hit(page, tick, &sides, p) {
            let k = RING_PAGES.len();
            let pg = self.se.ring.page;
            let next = if s == 1 {
                (pg + 1) % k
            } else {
                (pg + k - 1) % k
            };
            let change = ring_category(next) != ring_category(pg);
            self.ui.sound_key(PAGE, "p_MoveLeftRingMenu", &[]);
            self.se_ring_page(next, change);
        }
    }

    /// The chosen effect goes to the front of the palette (a history, as the parts') and is
    /// selected.
    fn choose_sound_ring(&mut self) {
        let Some(&id) = Self::ring_page(self.se.ring.page).get(self.se.ring.sel) else {
            return;
        };
        self.se.palette.retain(|&x| x != id);
        self.se.palette.insert(0, id);
        self.se.palette.truncate(SLOTS);
        self.se.slot = 0;
        self.se.previews.push(id);
        self.eraser = false;
        if let Some(p) = &mut self.page {
            p.play("L_PaletteSet_00/Sort", self.tick);
            p.hold("L_RightParts_00/L_EraserBtn_00/Toggle", 0.0);
        }
        self.ui
            .sound_button(PAGE, "L_PaletteSet_00/L_PaletteBtn_", Event::Decide);
        self.close_ring();
        self.update_page();
    }

    // ------------------------------------------------------------ on the course

    /// The course's effects as layout quads: their icons from the sheet (a flat square where
    /// the import lacks it).
    pub fn sound_effect_quads(&self, q: &mut Vec<Quad>) {
        let tex = self.model_texture(SHEET);
        for e in self.editor.placed_effects() {
            let Some((col, row)) = sheet_cell(e.id) else {
                continue;
            };
            let (x, y) = (e.tile.0 as f32 * 16.0, e.tile.1 as f32 * 16.0);
            let (r, uv) = match e.target {
                SoundTarget::Spot(_) => {
                    let [x0, y0] = self.to_layout(x, y);
                    let [x1, y1] = self.to_layout(x + 16.0, y + 16.0);
                    let (u, v) = (col as f32 * CELL, row as f32 * CELL);
                    ([x0, y0, x1, y1], [u, v, u + CELL, v + CELL])
                }
                // a part's and Mario's: the small framed icon over the tile's top-right corner
                _ => {
                    let [x0, y0] = self.to_layout(x + 8.0, y + 8.0);
                    let [x1, y1] = self.to_layout(x + 20.0, y + 20.0);
                    let u = col as f32 * CELL + SMALL_INSET;
                    let v = (row + SMALL_ROW) as f32 * CELL + SMALL_INSET;
                    ([x0, y0, x1, y1], [u, v, u + SMALL, v + SMALL])
                }
            };
            if r[2] < -640.0 || r[0] > 640.0 || r[3] < -360.0 || r[1] > 360.0 {
                continue;
            }
            match tex {
                Some(t) => q.push(quad(
                    r,
                    t,
                    [
                        uv[0] / SHEET_W,
                        uv[1] / SHEET_H,
                        uv[2] / SHEET_W,
                        uv[3] / SHEET_H,
                    ],
                    [255; 4],
                )),
                None => q.push(quad(
                    r,
                    TEXTURE_NONE,
                    [0.0, 0.0, 1.0, 1.0],
                    [255, 220, 60, 200],
                )),
            }
        }
    }

    /// The tick's effect sounds, by their Course Maker users.
    pub(super) fn sound_effect_previews(&mut self, host: &mut dyn crate::host::Host) {
        let style = self.style();
        for id in std::mem::take(&mut self.se.previews) {
            host.edit_sound(&style, &edit_user(id), "cue", None);
        }
    }
}
