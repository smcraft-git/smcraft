//! Mario's Trail (`L_LeftParts_00/L_ZanzoBtn_00`, "zanzo" = afterimage; research/specs/maker.md
//! C12, private): the game's `SnapshotTextureMgr` keeps up to 40 pictures of player 1 taken
//! during the last test play, each where it was taken, and draws them back in the edit view,
//! the older the fainter [obs]:
//! - a picture each time the player is more than 22 px from where the last one was taken (none
//!   below the area's bottom) [obs 0x710158de40];
//! - back in the editor the oldest are dropped when the trail is short or stays round one
//!   screen, then the pictures come in one every 2 frames, oldest first, alpha 0.3 → 0.7
//!   [obs 0x71013333a0, 0x7101333660, 0x7101333890];
//! - emptied when a test play starts, on an area change and on a style / theme change, not on
//!   an edit [obs where the reset bit is set; inf what the callers are].
//!
//! The pictures are the host's ([`Host::player_snapshot`]); they go into the UI cache as the
//! textures `Trail/snap_00…39`, reused round the pool as the game's "snap shot %d" are.

use crate::host::{Host, Icon};
use crate::layout::Ui;

/// Pictures kept (the game's pool of 40 snapshot textures) [obs].
pub const KEEP: usize = 40;
/// A picture once the player is more than this far from the last one (px) [obs].
const SPACING: f32 = 22.0;
/// None while the feet are more than this below the area's bottom (3D World 24) [obs
/// `DAT_7102318d6c`; inf: the area's bottom is y 0].
const BELOW: f32 = 32.0;
const BELOW_3W: f32 = 24.0;
/// A trail whose oldest and newest pictures are less than 128 px apart keeps its last 12
/// [obs `DAT_7102318f68`].
const SHORT: f32 = 128.0;
const SHORT_KEEP: usize = 12;
/// A run of 27 pictures within a screen (|dx| < 384, |dy| < 216) of its first: the older ones go
/// [obs `DAT_7102318acc`, `DAT_71023190a4`].
const RUN: usize = 27;
const SCREEN: [f32; 2] = [384.0, 216.0];
/// Alpha of the oldest picture drawn, what the newest adds over it, the most [obs
/// `DAT_71023195ec`, `DAT_71023192bc`, `DAT_7102318b94`].
const ALPHA_OLDEST: f32 = 0.3;
const ALPHA_SPAN: f32 = 0.4;
const ALPHA_MAX: f32 = 0.7;
/// The UI cache group of the pictures.
pub const GROUP: &str = "Trail";

/// One picture: its area, the world rectangle it shows (x0, y0, x1, y1, px, y up), where the
/// player's feet were, and its texture (`Trail/<name>`).
#[derive(Clone, Debug, PartialEq)]
pub struct Shot {
    pub area: usize,
    pub rect: [f32; 4],
    pub feet: [f32; 2],
    pub texture: String,
}

#[derive(Clone, Debug, Default)]
pub struct Trail {
    /// Oldest first.
    pub shots: Vec<Shot>,
    /// Where the last picture was taken (the player's +0x2a28).
    last: Option<[f32; 2]>,
    /// The area being recorded.
    area: Option<usize>,
    /// The next texture of the pool (+0xbc).
    next: usize,
    /// Pictures there were when the editor came back (+0x124: the alpha's steps).
    kept: usize,
    /// Pictures shown so far (+0x120) and the count-down to the next (+0x128).
    pub shown: usize,
    clock: i32,
}

impl Trail {
    /// Empty it (`cState_ResetHistory`).
    pub fn clear(&mut self) {
        self.shots.clear();
        self.last = None;
        self.area = None;
        self.kept = 0;
        self.shown = 0;
        self.clock = 1;
    }

    /// A test-play tick: a picture of player 1 when it has moved on far enough.
    pub fn record(&mut self, host: &mut dyn Host, ui: &Ui, style: &str) {
        let Some((area, p)) = host.player_position() else {
            return;
        };
        if self.area.is_some_and(|a| a != area) {
            // through a pipe or a door to the other area (`cState_AreaChange`)
            self.clear();
        }
        self.area = Some(area);
        if let Some(l) = self.last
            && (p[0] - l[0]).hypot(p[1] - l[1]) <= SPACING
        {
            return;
        }
        self.last = Some(p);
        let below = if style == "3W" { BELOW_3W } else { BELOW };
        if p[1] + below < 0.0 {
            return;
        }
        let Some((icon, rect)) = host.player_snapshot() else {
            return;
        };
        let texture = format!("snap_{:02}", self.next);
        self.next = (self.next + 1) % KEEP;
        ui.cache_mut().set_texture_file(GROUP, &texture, mtex(icon));
        if self.shots.len() >= KEEP {
            self.shots.remove(0);
        }
        self.shots.push(Shot {
            area,
            rect,
            feet: p,
            texture,
        });
    }

    /// Back in the editor (`cState_DrawTexture`'s enter): drop what the game drops, then show
    /// the pictures again from the first.
    pub fn enter(&mut self) {
        let n = self.shots.len().min(KEEP);
        self.kept = n;
        if n > SHORT_KEEP {
            let (a, b) = (self.shots[0].feet, self.shots[n - 1].feet);
            if (a[0] - b[0]).hypot(a[1] - b[1]) < SHORT {
                self.shots.drain(..n - SHORT_KEEP);
                self.kept = SHORT_KEEP;
            } else if n > RUN {
                let near = |s: usize, k: usize| {
                    let (a, b) = (self.shots[s].feet, self.shots[k].feet);
                    (a[0] - b[0]).abs() < SCREEN[0] && (a[1] - b[1]).abs() < SCREEN[1]
                };
                // the latest run of 27 that stays round its first picture (the alpha keeps
                // its steps over the n there were)
                if let Some(s) = (0..=n - RUN)
                    .rev()
                    .find(|&s| (s..s + RUN).all(|k| near(s, k)))
                {
                    self.shots.drain(..s);
                }
            }
        }
        self.shown = 0;
        self.clock = 1;
    }

    /// An editor tick (`cState_DrawTexture`'s exec): one more picture every 2 ticks while the
    /// trail is on; off, none (switched on again, they come in from the first).
    pub fn tick(&mut self, on: bool) {
        if self.clock < 1 {
            self.shown = (self.shown + 1).min(self.shots.len().min(KEEP));
            self.clock = 1;
        } else {
            self.clock -= 1;
        }
        if !on {
            self.shown = 0;
            self.clock = 1;
        }
    }

    /// The pictures to draw in `area` (oldest first): (world rectangle, texture, alpha).
    pub fn visible(&self, area: usize) -> Vec<([f32; 4], &str, f32)> {
        let step = ALPHA_SPAN / (self.kept - usize::from(self.kept > 1)).max(1) as f32;
        let mut alpha = ALPHA_OLDEST;
        let mut out = Vec::new();
        for s in self.shots.iter().take(self.shown) {
            if s.area == area {
                out.push((s.rect, s.texture.as_str(), alpha));
            }
            alpha = (alpha + step).min(ALPHA_MAX);
        }
        out
    }
}

/// A picture as a UI texture.
fn mtex(icon: Icon) -> mmk_assets::Mtex {
    mmk_assets::Mtex {
        format: mmk_assets::MtexFormat::Rgba8,
        variant: mmk_assets::MtexVariant::Srgb,
        width: icon.width,
        height: icon.height,
        channel_select: [2, 3, 4, 5],
        data: icon.rgba,
    }
}
