//! SMM1's sound banks (`Pack/Sound.pack` → `Sound/Resource/<bank>.bars`): unlike SMM2's BARS
//! these are SARC archives holding, per sound, `<name>.bameta` (an AMTA, big-endian: the name
//! and whether the sound is a wave or a stream) and, for a wave, `<name>.bfwav`. A stream's
//! samples are a separate `Sound/Resource/Stream/<name>.bfstm` ([`crate::audio::fstm`]).
//!
//! AMTA (Wii U), from inspection of the owner's files:
//!
//! ```text
//! 0x00  "AMTA"  u16 BOM (FE FF)  u16 version (0x0100)  u32 size
//! 0x0C  u32 offsets of DATA, MARK and STRG (from the AMTA's start)
//! DATA: "DATA" u32 size, +0x08 u32 ×2, +0x10 u8 kind (0 wave, 1 stream), u8 channels, u16,
//!       f32, +0x18 u32 sample rate, u32 loop start, u32 loop end
//! STRG: "STRG" u32 length, the name
//! ```

use crate::archive::Sarc;
use crate::error::{Error, Result};

fn bad(detail: impl Into<String>) -> Error {
    Error::Format {
        what: "AMTA",
        detail: detail.into(),
    }
}

fn u32_at(d: &[u8], o: usize) -> Result<u32> {
    d.get(o..o + 4)
        .map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
        .ok_or_else(|| bad(format!("truncated at {o:#x}")))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SoundKind {
    Wave,
    Stream,
    Other(u8),
}

/// A sound's metadata.
#[derive(Clone, Debug, PartialEq)]
pub struct Amta {
    pub name: String,
    pub kind: SoundKind,
    pub channels: u8,
    pub sample_rate: u32,
}

impl Amta {
    pub fn parse(d: &[u8]) -> Result<Self> {
        if d.get(..4) != Some(b"AMTA") {
            return Err(Error::Magic { what: "AMTA" });
        }
        if d.get(4..6) != Some(&[0xFE, 0xFF]) {
            return Err(bad("little-endian AMTA (SMM2's) not read here"));
        }
        let data = u32_at(d, 0x0C)? as usize;
        let strg = u32_at(d, 0x14)? as usize;
        if d.get(data..data + 4) != Some(b"DATA") || d.get(strg..strg + 4) != Some(b"STRG") {
            return Err(bad("missing DATA or STRG"));
        }
        let len = u32_at(d, strg + 4)? as usize;
        let raw = d
            .get(strg + 8..strg + 8 + len)
            .ok_or_else(|| bad("name past end"))?;
        let end = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
        let kind = match d.get(data + 0x10).copied().unwrap_or(0) {
            0 => SoundKind::Wave,
            1 => SoundKind::Stream,
            k => SoundKind::Other(k),
        };
        Ok(Self {
            name: String::from_utf8_lossy(&raw[..end]).into_owned(),
            kind,
            channels: d.get(data + 0x11).copied().unwrap_or(0),
            sample_rate: u32_at(d, data + 0x18)?,
        })
    }

    /// A minimal AMTA (tests).
    pub fn build(name: &str, kind: u8, channels: u8, sample_rate: u32) -> Vec<u8> {
        let mut d = vec![0u8; 0x18];
        d[..4].copy_from_slice(b"AMTA");
        d[4..6].copy_from_slice(&[0xFE, 0xFF]);
        d[6..8].copy_from_slice(&0x0100u16.to_be_bytes());
        let data = d.len();
        d.extend_from_slice(b"DATA");
        d.extend_from_slice(&0x60u32.to_be_bytes());
        d.resize(data + 8 + 0x60, 0);
        d[data + 0x10] = kind;
        d[data + 0x11] = channels;
        d[data + 0x18..data + 0x1C].copy_from_slice(&sample_rate.to_be_bytes());
        let strg = d.len();
        d.extend_from_slice(b"STRG");
        d.extend_from_slice(&(name.len() as u32 + 1).to_be_bytes());
        d.extend_from_slice(name.as_bytes());
        d.push(0);
        while !d.len().is_multiple_of(4) {
            d.push(0);
        }
        let size = d.len() as u32;
        d[8..12].copy_from_slice(&size.to_be_bytes());
        d[0x0C..0x10].copy_from_slice(&(data as u32).to_be_bytes());
        d[0x14..0x18].copy_from_slice(&(strg as u32).to_be_bytes());
        d
    }
}

/// One sound of a bank: its metadata (if readable) and its wave (BFWAV bytes; none for a stream).
#[derive(Clone, Debug)]
pub struct BankSound<'a> {
    pub name: String,
    pub amta: Option<Amta>,
    pub wave: Option<&'a [u8]>,
}

/// The sounds of a bank SARC, by the members' names (`<name>.bameta`, `<name>.bfwav`).
pub fn bank_sounds(sarc: &Sarc) -> Vec<BankSound<'_>> {
    let mut out: Vec<BankSound> = Vec::new();
    for (member, data) in sarc.files() {
        let base = member.rsplit('/').next().unwrap_or(member);
        let (stem, wave) = if let Some(s) = base.strip_suffix(".bfwav") {
            (s, true)
        } else if let Some(s) = base.strip_suffix(".bameta") {
            (s, false)
        } else {
            continue;
        };
        let i = match out.iter().position(|s| s.name == stem) {
            Some(i) => i,
            None => {
                out.push(BankSound {
                    name: stem.to_string(),
                    amta: None,
                    wave: None,
                });
                out.len() - 1
            }
        };
        if wave {
            out[i].wave = Some(data);
        } else {
            out[i].amta = Amta::parse(data).ok();
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pairs_waves_with_their_metadata() {
        let amta = Amta::build("SE_Ok", 0, 1, 32000);
        let a = Amta::parse(&amta).unwrap();
        assert_eq!(
            (a.name.as_str(), a.kind, a.channels, a.sample_rate),
            ("SE_Ok", SoundKind::Wave, 1, 32000)
        );
        let stream = Amta::build("SE_Long", 1, 2, 48000);
        let sarc = crate::archive::sarc::build(&[
            ("SE_Ok.bameta", &amta),
            ("SE_Ok.bfwav", b"FWAV...."),
            ("SE_Long.bameta", &stream),
        ]);
        let sarc = Sarc::parse(sarc).unwrap();
        let mut s = bank_sounds(&sarc);
        s.sort_by(|a, b| a.name.cmp(&b.name));
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].name, "SE_Long");
        assert_eq!(s[0].amta.as_ref().unwrap().kind, SoundKind::Stream);
        assert!(s[0].wave.is_none());
        assert_eq!(s[1].wave, Some(&b"FWAV...."[..]));
        assert!(Amta::parse(b"AMTA\xff\xfe").is_err());
    }
}
