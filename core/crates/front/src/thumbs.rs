//! Course World thumbnails: the host fetches a course's JPEG (its one-screen picture, or the
//! whole-course map), we decode it into a texture of the SMM2 import's cache (group
//! [`GROUP`]) and the screens put it into their thumbnail pictures (`Cmn_Thumbnail_00`'s
//! `P_DummyThumbnail_00`). One request at a time, the ones on screen first; a course without a
//! picture is not asked for again.

use std::collections::{HashMap, VecDeque};

use crate::Cx;
use crate::layout::Ui;

pub const GROUP: &str = "CourseWorldThumbnail";

/// (code, entire).
pub type Key = (String, bool);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Thumb {
    /// Asked for (or waiting its turn).
    Waiting,
    /// The texture's stem in [`GROUP`].
    Ready(String),
    /// None to be had.
    Missing,
}

#[derive(Default)]
pub struct Thumbs {
    state: HashMap<Key, Thumb>,
    /// Wanted next, the latest first.
    queue: VecDeque<Key>,
    in_flight: Option<Key>,
    /// The host has no thumbnails (asking returned false).
    unavailable: bool,
}

impl Thumbs {
    pub fn get(&self, code: &str, entire: bool) -> Option<&Thumb> {
        self.state.get(&(code.to_string(), entire))
    }

    /// The texture stem when it is loaded.
    pub fn ready(&self, code: &str, entire: bool) -> Option<String> {
        match self.get(code, entire) {
            Some(Thumb::Ready(s)) => Some(s.clone()),
            _ => None,
        }
    }

    /// Ask for these (the screen's rows, in order of need) ahead of the ones asked before.
    pub fn want(&mut self, keys: impl IntoIterator<Item = Key>) {
        if self.unavailable {
            return;
        }
        let keys: Vec<Key> = keys.into_iter().collect();
        for k in keys.into_iter().rev() {
            if self.state.contains_key(&k) && !self.queue.contains(&k) {
                continue;
            }
            self.queue.retain(|q| *q != k);
            self.queue.push_front(k.clone());
            self.state.insert(k, Thumb::Waiting);
        }
    }

    /// One tick: take what arrived, ask for the next. True when a thumbnail became ready.
    pub fn step(&mut self, cx: &mut Cx, ui: &Ui) -> bool {
        let mut changed = false;
        while let Some((code, entire, jpeg)) = cx.host.poll_thumbnail() {
            let k = (code, entire);
            if self.in_flight.as_ref() == Some(&k) {
                self.in_flight = None;
            }
            let t = match texture(ui, &k, &jpeg) {
                Some(stem) => {
                    changed = true;
                    Thumb::Ready(stem)
                }
                None => Thumb::Missing,
            };
            self.state.insert(k, t);
        }
        if self.in_flight.is_none()
            && !self.unavailable
            && let Some(k) = self.queue.pop_front()
        {
            if cx.host.request_thumbnail(&k.0, k.1) {
                self.in_flight = Some(k);
            } else {
                self.unavailable = true;
                self.state.insert(k, Thumb::Missing);
                for q in std::mem::take(&mut self.queue) {
                    self.state.insert(q, Thumb::Missing);
                }
            }
        }
        changed
    }
}

/// Decode a JPEG into a texture of the UI cache: its stem in [`GROUP`].
pub fn texture(ui: &Ui, key: &Key, jpeg: &[u8]) -> Option<String> {
    let (width, height, rgba) = decode(jpeg)?;
    let stem: String = format!("{}{}", key.0, if key.1 { "_entire" } else { "" })
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    let tex = mmk_assets::Mtex {
        format: mmk_assets::MtexFormat::Rgba8,
        variant: mmk_assets::MtexVariant::Srgb,
        width,
        height,
        channel_select: [2, 3, 4, 5],
        data: rgba,
    };
    ui.cache_mut().add_texture_file(GROUP, &stem, tex);
    Some(stem)
}

/// JPEG → (width, height, RGBA8).
pub fn decode(jpeg: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    let mut d = jpeg_decoder::Decoder::new(jpeg);
    let pixels = d.decode().ok()?;
    let info = d.info()?;
    let (w, h) = (info.width as usize, info.height as usize);
    let rgba: Vec<u8> = match info.pixel_format {
        jpeg_decoder::PixelFormat::RGB24 => pixels
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        jpeg_decoder::PixelFormat::L8 => pixels.iter().flat_map(|&l| [l, l, l, 255]).collect(),
        _ => return None,
    };
    (rgba.len() == w * h * 4 && w > 0 && h > 0).then_some((w as u32, h as u32, rgba))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wants_go_newest_first_and_once() {
        let mut t = Thumbs::default();
        t.want([("A".to_string(), false), ("B".to_string(), false)]);
        t.want([("C".to_string(), false)]);
        assert_eq!(
            t.queue.iter().map(|k| k.0.as_str()).collect::<Vec<_>>(),
            ["C", "A", "B"]
        );
        t.state.insert(("A".into(), false), Thumb::Missing);
        t.queue.retain(|k| k.0 != "A");
        t.want([("A".to_string(), false)]);
        assert!(!t.queue.iter().any(|k| k.0 == "A"), "not asked for again");
    }

    #[test]
    fn not_a_jpeg_is_no_texture() {
        assert!(decode(b"nope").is_none());
    }
}
