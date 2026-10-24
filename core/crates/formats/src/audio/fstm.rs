//! BFSTM (`FSTM`): the Wii U stream container (SMM1's `Sound/Resource/Stream/*.bfstm`),
//! big-endian. Read into an [`Fwav`] (channels de-interleaved) so the same BWAV conversion plays
//! it.
//!
//! Layout (from the public descriptions of the NW4F stream format):
//!
//! ```text
//! 0x00 "FSTM"  u16 BOM (FE FF)  u16 header size  u32 version  u32 file size
//! 0x10 u16 section count  u16 pad, then per section: u16 type (0x4000 INFO, 0x4001 SEEK,
//!      0x4002 DATA), u16 pad, u32 offset, u32 size
//! INFO +0x08: references (u16 type, u16 pad, i32 offset from INFO + 8) to the stream info,
//!      the track info table and the channel info table
//! stream info: u8 encoding (0 PCM8, 1 PCM16, 2 DSP-ADPCM), u8 loop flag, u8 channels, u8,
//!      u32 sample rate, u32 loop start, u32 loop end (= sample count), u32 block count,
//!      u32 block size, u32 block samples, u32 last block size, u32 last block samples,
//!      u32 last block padded size, u32 seek size, u32 seek interval, then a reference to the
//!      samples (offset from DATA + 8)
//! channel info table: u32 count, references (offset from the table) to channel infos; a
//!      channel info is a reference (offset from it) to the ADPCM info: i16 coefficients[16], …
//! samples: per block, each channel's block in turn (the last block's channels are spaced by
//!      the last block padded size)
//! ```

use crate::audio::fwav::{Fwav, FwavChannel};
use crate::error::{Error, Result};

fn bad(detail: impl Into<String>) -> Error {
    Error::Format {
        what: "BFSTM",
        detail: detail.into(),
    }
}

fn u8_at(d: &[u8], o: usize) -> Result<u8> {
    d.get(o)
        .copied()
        .ok_or_else(|| bad(format!("truncated at {o:#x}")))
}

fn u16_at(d: &[u8], o: usize) -> Result<u16> {
    Ok(u16::from_be_bytes([u8_at(d, o)?, u8_at(d, o + 1)?]))
}

fn u32_at(d: &[u8], o: usize) -> Result<u32> {
    Ok(u32::from_be_bytes([
        u8_at(d, o)?,
        u8_at(d, o + 1)?,
        u8_at(d, o + 2)?,
        u8_at(d, o + 3)?,
    ]))
}

/// Target of a reference (u16 type, u16 pad, i32 offset) at `at`, relative to `base`.
fn reference(d: &[u8], at: usize, base: usize) -> Result<usize> {
    let off = u32_at(d, at + 4)? as i32;
    if off < 0 {
        return Err(bad(format!("null reference at {at:#x}")));
    }
    Ok(base + off as usize)
}

/// Parse a stream into its de-interleaved channels.
pub fn parse(d: &[u8]) -> Result<Fwav> {
    if d.get(..4) != Some(b"FSTM") {
        return Err(Error::Magic { what: "BFSTM" });
    }
    if u16_at(d, 4)? != 0xFEFF {
        return Err(bad("little-endian stream not supported"));
    }
    let (mut info, mut data) = (None, None);
    for i in 0..u16_at(d, 0x10)? as usize {
        let s = 0x14 + 12 * i;
        match u16_at(d, s)? {
            0x4000 => info = Some(u32_at(d, s + 4)? as usize),
            0x4002 => data = Some(u32_at(d, s + 4)? as usize),
            _ => {}
        }
    }
    let (Some(info), Some(data)) = (info, data) else {
        return Err(bad("missing INFO or DATA section"));
    };
    let refs = info + 8;
    let si = reference(d, refs, refs)?;
    let table = reference(d, refs + 16, refs)?;
    let encoding = u8_at(d, si)?;
    let looping = u8_at(d, si + 1)? != 0;
    let n_ch = u8_at(d, si + 2)? as usize;
    let sample_rate = u32_at(d, si + 4)?;
    let loop_start = u32_at(d, si + 8)?;
    let samples = u32_at(d, si + 0x0C)?;
    let blocks = u32_at(d, si + 0x10)? as usize;
    let block = u32_at(d, si + 0x14)? as usize;
    let last = u32_at(d, si + 0x1C)? as usize;
    let last_padded = u32_at(d, si + 0x24)? as usize;
    let start = reference(d, si + 0x30, data + 8)?;
    if !(1..=8).contains(&n_ch) || blocks == 0 || encoding > 2 {
        return Err(bad(format!(
            "{n_ch} channels, {blocks} blocks, encoding {encoding}"
        )));
    }
    if u32_at(d, table)? as usize != n_ch {
        return Err(bad("channel table count"));
    }
    let mut channels = Vec::with_capacity(n_ch);
    for c in 0..n_ch {
        let ci = reference(d, table + 4 + 8 * c, table)?;
        let mut coefs = [0i16; 16];
        if encoding == 2 {
            let adpcm = reference(d, ci, ci)?;
            for (k, v) in coefs.iter_mut().enumerate() {
                *v = u16_at(d, adpcm + 2 * k)? as i16;
            }
        }
        let mut bytes = Vec::with_capacity(block * blocks);
        for b in 0..blocks {
            let (at, len) = if b + 1 == blocks {
                (start + b * block * n_ch + c * last_padded, last)
            } else {
                (start + b * block * n_ch + c * block, block)
            };
            let chunk = d
                .get(at..at + len)
                .ok_or_else(|| bad(format!("block {b} of channel {c} outside the file")))?;
            bytes.extend_from_slice(chunk);
        }
        channels.push(FwavChannel { data: bytes, coefs });
    }
    Ok(Fwav {
        encoding,
        sample_rate,
        samples,
        loop_range: (looping && loop_start < samples).then_some((loop_start, samples)),
        channels,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::Bwav;

    /// A stereo PCM16 stream of 5 samples in blocks of 2 samples (4 bytes): 3 blocks, the last
    /// holding one sample padded to 4 bytes.
    fn pcm_stream(l: &[i16], r: &[i16]) -> Vec<u8> {
        let mut f = vec![0u8; 0x40];
        f[..4].copy_from_slice(b"FSTM");
        f[4..6].copy_from_slice(&[0xFE, 0xFF]);
        f[0x10..0x12].copy_from_slice(&2u16.to_be_bytes());
        let info = 0x40usize;
        let put = |f: &mut Vec<u8>, at: usize, v: u32| {
            if f.len() < at + 4 {
                f.resize(at + 4, 0);
            }
            f[at..at + 4].copy_from_slice(&v.to_be_bytes());
        };
        f[0x14..0x16].copy_from_slice(&0x4000u16.to_be_bytes());
        put(&mut f, 0x18, info as u32);
        f.resize(info + 0x100, 0);
        f[info..info + 4].copy_from_slice(b"INFO");
        let refs = info + 8;
        // stream info at refs + 0x18, channel table at refs + 0x60
        put(&mut f, refs + 4, 0x18);
        put(&mut f, refs + 16 + 4, 0x60);
        let si = refs + 0x18;
        f[si] = 1;
        f[si + 2] = 2;
        put(&mut f, si + 4, 32000);
        put(&mut f, si + 0x0C, l.len() as u32);
        put(&mut f, si + 0x10, 3);
        put(&mut f, si + 0x14, 4);
        put(&mut f, si + 0x18, 2);
        put(&mut f, si + 0x1C, 2);
        put(&mut f, si + 0x20, 1);
        put(&mut f, si + 0x24, 4);
        put(&mut f, si + 0x30 + 4, 0);
        let table = refs + 0x60;
        put(&mut f, table, 2);
        put(&mut f, table + 4 + 4, 0x14);
        put(&mut f, table + 12 + 4, 0x1C);
        let data = f.len();
        f[0x20..0x22].copy_from_slice(&0x4002u16.to_be_bytes());
        put(&mut f, 0x24, data as u32);
        f.extend_from_slice(b"DATA\0\0\0\0");
        for b in 0..3 {
            for ch in [l, r] {
                for k in 0..2 {
                    let s = ch.get(b * 2 + k).copied().unwrap_or(0);
                    f.extend_from_slice(&s.to_be_bytes());
                }
            }
        }
        f
    }

    #[test]
    fn deinterleaves_blocks() {
        let l = [1i16, 2, 3, 4, 5];
        let r = [-1i16, -2, -3, -4, -5];
        let w = parse(&pcm_stream(&l, &r)).unwrap();
        assert_eq!((w.encoding, w.samples, w.channels.len()), (1, 5, 2));
        let b = Bwav::parse(&w.to_bwav())
            .unwrap()
            .decode_channel(1)
            .unwrap();
        assert_eq!(b, r);
        assert!(parse(b"FWAV").is_err());
    }
}
