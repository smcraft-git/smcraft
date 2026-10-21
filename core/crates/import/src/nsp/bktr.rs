//! BKTR: how an update's RomFS section patches the base game's. The section's FS header carries
//! two bucket trees (each a 0x10-byte header "BKTR", version, entry count, plus a table in the
//! section's data):
//! - the **indirect** table maps the patched section, range by range, to either the base
//!   section (storage 0) or the update's own data (storage 1); entries are (virtual offset,
//!   physical offset, storage index), 0x14 bytes;
//! - the **AES-CTR-EX** table splits the update's data into ranges encrypted with different
//!   counters: entries are (offset, type, generation), 0x10 bytes, and a range's counter is the
//!   section's with its low 32 upper bits replaced by the generation.
//!
//! A table is a 0x4000-byte node of entry set offsets (index, count, end offset, offsets), then
//! 0x4000-byte entry sets (index, count, end offset, entries). The tables themselves are past
//! the AES-CTR-EX ranges, encrypted with the section's own counter.

use std::io;
use std::sync::Arc;

use super::nca::CtrStorage;
use super::storage::{Shared, Storage, check_range, invalid, u32_at, u64_at};

const NODE: usize = 0x4000;

/// A bucket tree's entries (each `entry_size` bytes) and the end offset of the last one.
fn read_tree(
    storage: &dyn Storage,
    offset: u64,
    size: u64,
    header: &[u8],
    entry_size: usize,
) -> io::Result<(Vec<Vec<u8>>, u64)> {
    if &header[..4] != b"BKTR" {
        return Err(invalid("BKTR: bad bucket tree header"));
    }
    let count = u32_at(header, 8) as usize;
    if count == 0 {
        return Ok((Vec::new(), 0));
    }
    let per_set = (NODE - 0x10) / entry_size;
    let sets = count.div_ceil(per_set);
    if sets > (NODE - 0x10) / 8 {
        return Err(invalid(
            "BKTR: table too large (two-level trees are not read)",
        ));
    }
    if size < ((1 + sets) * NODE) as u64 {
        return Err(invalid("BKTR: table smaller than its entry count"));
    }
    let table = storage.read_vec(offset, ((1 + sets) * NODE) as u64)?;
    if u32_at(&table, 4) as usize != sets {
        return Err(invalid("BKTR: table does not match its header"));
    }
    let end = u64_at(&table, 8);
    let mut entries = Vec::with_capacity(count);
    for s in 0..sets {
        let set = &table[(1 + s) * NODE..(2 + s) * NODE];
        let n = u32_at(set, 4) as usize;
        if n > per_set || entries.len() + n > count {
            return Err(invalid("BKTR: bad entry set"));
        }
        for i in 0..n {
            entries.push(set[0x10 + i * entry_size..0x10 + (i + 1) * entry_size].to_vec());
        }
    }
    if entries.len() != count {
        return Err(invalid("BKTR: entry count mismatch"));
    }
    Ok((entries, end))
}

/// The update's data, decrypted range by range with the AES-CTR-EX counters; past the last
/// range (the BKTR tables) with the section's own counter.
pub struct AesCtrExStorage {
    ctr: Arc<CtrStorage>,
    upper: u64,
    /// (start offset, generation), sorted
    ranges: Vec<(u64, u32)>,
    end: u64,
}

impl AesCtrExStorage {
    /// `ctr` is the section with plain AES-CTR (for the table), `info` the FS header's
    /// AES-CTR-EX table (offset, size, header).
    pub fn new(ctr: Arc<CtrStorage>, upper: u64, info: (u64, u64, &[u8])) -> io::Result<Self> {
        let (entries, end) = read_tree(&*ctr, info.0, info.1, info.2, 0x10)?;
        let ranges: Vec<(u64, u32)> = entries
            .iter()
            .map(|e| (u64_at(e, 0), u32_at(e, 0xC)))
            .collect();
        if ranges.windows(2).any(|w| w[0].0 > w[1].0) || ranges.first().is_some_and(|r| r.0 != 0) {
            return Err(invalid("BKTR: AES-CTR-EX ranges out of order"));
        }
        Ok(Self {
            ctr,
            upper,
            ranges,
            end,
        })
    }
}

impl Storage for AesCtrExStorage {
    fn size(&self) -> u64 {
        self.ctr.size()
    }
    fn read_at(&self, mut offset: u64, mut buf: &mut [u8]) -> io::Result<()> {
        check_range(self.size(), offset, buf.len())?;
        while !buf.is_empty() {
            let (upper, until) = if offset >= self.end {
                (self.upper, self.size())
            } else {
                let i = self.ranges.partition_point(|r| r.0 <= offset) - 1;
                let next = self.ranges.get(i + 1).map_or(self.end, |r| r.0);
                (
                    (self.upper & !0xFFFF_FFFF) | u64::from(self.ranges[i].1),
                    next,
                )
            };
            let n = ((until - offset) as usize).min(buf.len());
            let (now, rest) = buf.split_at_mut(n);
            self.ctr.read_with(upper, offset, now)?;
            offset += n as u64;
            buf = rest;
        }
        Ok(())
    }
}

/// The patched section: each range from the base section or the update's data.
pub struct IndirectStorage {
    sources: [Shared; 2],
    /// (virtual offset, physical offset, storage index), sorted by virtual offset
    entries: Vec<(u64, u64, usize)>,
    size: u64,
}

impl IndirectStorage {
    /// `base` is the base game's decrypted RomFS section, `patch` the update's decrypted data;
    /// `info` the FS header's indirect table (offset, size, header), read from `patch`.
    pub fn new(base: Shared, patch: Shared, info: (u64, u64, &[u8])) -> io::Result<Self> {
        let (raw, size) = read_tree(&*patch, info.0, info.1, info.2, 0x14)?;
        let entries: Vec<(u64, u64, usize)> = raw
            .iter()
            .map(|e| (u64_at(e, 0), u64_at(e, 8), u32_at(e, 0x10) as usize))
            .collect();
        if entries.first().is_none_or(|e| e.0 != 0)
            || entries.windows(2).any(|w| w[0].0 >= w[1].0)
            || entries.iter().any(|e| e.2 > 1)
        {
            return Err(invalid("BKTR: bad indirect table"));
        }
        Ok(Self {
            sources: [base, patch],
            entries,
            size,
        })
    }
}

impl Storage for IndirectStorage {
    fn size(&self) -> u64 {
        self.size
    }
    fn read_at(&self, mut offset: u64, mut buf: &mut [u8]) -> io::Result<()> {
        check_range(self.size, offset, buf.len())?;
        while !buf.is_empty() {
            let i = self.entries.partition_point(|e| e.0 <= offset) - 1;
            let (virt, phys, source) = self.entries[i];
            let next = self.entries.get(i + 1).map_or(self.size, |e| e.0);
            let n = ((next - offset) as usize).min(buf.len());
            let (now, rest) = buf.split_at_mut(n);
            self.sources[source].read_at(phys + (offset - virt), now)?;
            offset += n as u64;
            buf = rest;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nsp::storage::MemStorage;

    /// A one-set bucket tree table and its FS header part.
    pub(crate) fn tree(entries: &[Vec<u8>], end: u64) -> (Vec<u8>, Vec<u8>) {
        let mut t = vec![0u8; 2 * NODE];
        t[4..8].copy_from_slice(&1u32.to_le_bytes());
        t[8..16].copy_from_slice(&end.to_le_bytes());
        let set = &mut t[NODE..];
        set[4..8].copy_from_slice(&(entries.len() as u32).to_le_bytes());
        set[8..16].copy_from_slice(&end.to_le_bytes());
        let mut o = 0x10;
        for e in entries {
            set[o..o + e.len()].copy_from_slice(e);
            o += e.len();
        }
        let mut h = b"BKTR".to_vec();
        h.extend_from_slice(&1u32.to_le_bytes());
        h.extend_from_slice(&(entries.len() as u32).to_le_bytes());
        h.extend_from_slice(&0u32.to_le_bytes());
        (t, h)
    }

    fn indirect(virt: u64, phys: u64, src: u32) -> Vec<u8> {
        [
            &virt.to_le_bytes()[..],
            &phys.to_le_bytes(),
            &src.to_le_bytes(),
        ]
        .concat()
    }

    #[test]
    fn indirect_mixes_base_and_patch() {
        let base: Shared = Arc::new(MemStorage(b"AAAAAAAAAA".to_vec()));
        let (table, header) = tree(
            &[indirect(0, 0, 0), indirect(4, 2, 1), indirect(7, 7, 0)],
            10,
        );
        let mut patch_bytes = b"xxPPPyy".to_vec();
        let at = patch_bytes.len() as u64;
        patch_bytes.extend(table);
        let len = patch_bytes.len() as u64 - at;
        let patch: Shared = Arc::new(MemStorage(patch_bytes));
        let s = IndirectStorage::new(base, patch, (at, len, &header)).unwrap();
        assert_eq!(s.size(), 10);
        assert_eq!(s.read_vec(0, 10).unwrap(), b"AAAAPPPAAA");
        assert_eq!(s.read_vec(3, 2).unwrap(), b"AP");
        assert!(s.read_vec(9, 2).is_err());
    }

    #[test]
    fn ctr_ex_switches_counters_by_range() {
        use crate::nsp::crypto::{Ctr, section_iv};
        let key = [5u8; 16];
        let (upper, base) = (0x1111_2222_0000_0007u64, 0x4000u64);
        // 0x40 bytes of data: 0x00..0x20 generation 1, 0x20..0x40 generation 9; then the table
        let mut plain: Vec<u8> = (0..0x40u8).collect();
        let ctr = Ctr::new(&key);
        let with_gen = |g: u64| (upper & !0xFFFF_FFFF) | g;
        ctr.apply(section_iv(with_gen(1), base), &mut plain[..0x20]);
        ctr.apply(section_iv(with_gen(9), base + 0x20), &mut plain[0x20..]);
        let ex = |off: u64, g: u32| {
            [
                &off.to_le_bytes()[..],
                &0u32.to_le_bytes(),
                &g.to_le_bytes(),
            ]
            .concat()
        };
        let (mut table, header) = tree(&[ex(0, 1), ex(0x20, 9)], 0x40);
        ctr.apply(section_iv(upper, base + 0x40), &mut table);
        let len = table.len() as u64;
        plain.extend(table);
        let raw: Shared = Arc::new(MemStorage(plain));
        let plain_ctr = Arc::new(CtrStorage::new(raw, &key, base, upper));
        let s = AesCtrExStorage::new(plain_ctr, upper, (0x40, len, &header)).unwrap();
        assert_eq!(
            s.read_vec(0, 0x40).unwrap(),
            (0..0x40u8).collect::<Vec<_>>()
        );
        assert_eq!(
            s.read_vec(0x1D, 6).unwrap(),
            [0x1D, 0x1E, 0x1F, 0x20, 0x21, 0x22]
        );
        // the table reads back through the section's own counter
        assert_eq!(&s.read_vec(0x40, 4).unwrap(), &[0, 0, 0, 0]);
    }
}
