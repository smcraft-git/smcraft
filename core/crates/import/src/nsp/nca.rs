//! NCA content archives (version 3): the 0xC00-byte header, encrypted with AES-128-XTS
//! (`header_key`, 0x200-byte sectors, Nintendo's tweak), names the content type, the key
//! generation, an optional rights id and up to four sections. A section's key comes from the
//! key area (decrypted with `key_area_key_application_XX`) or, with a rights id, is the title key
//! from the ticket (decrypted with `titlekek_XX`). Sections are AES-128-CTR; an update's RomFS
//! section is AES-CTR-EX, read by [`super::bktr`].
//!
//! Each section has a 0x200-byte FS header after the NCA header: file system type, hash type,
//! encryption type, the hash data that locates the payload (a PFS0 after a SHA-256 hash table,
//! or a RomFS as the last level of an IVFC tree), the patch info of a BKTR section, and the upper
//! half of the CTR counter.

use std::io;
use std::sync::Arc;

use super::crypto::{Ctr, ecb_decrypt, section_iv, xts_decrypt_nintendo};
use super::keys::Keys;
use super::storage::{Shared, Storage, SubStorage, check_range, invalid, u32_at, u64_at};
use super::ticket::Ticket;

pub const CONTENT_PROGRAM: u8 = 0;
pub const FS_ROMFS: u8 = 0;
pub const FS_PFS0: u8 = 1;
pub const ENC_NONE: u8 = 1;
pub const ENC_CTR: u8 = 3;
pub const ENC_CTR_EX: u8 = 4;

const HEADER_SIZE: usize = 0xC00;

/// One section's FS header (decrypted).
#[derive(Clone)]
pub struct FsHeader(pub Vec<u8>);

impl FsHeader {
    pub fn fs_type(&self) -> u8 {
        self.0[2]
    }
    pub fn encryption(&self) -> u8 {
        self.0[4]
    }
    /// The upper 64 bits of the section's CTR counter (generation, secure value).
    pub fn ctr_upper(&self) -> u64 {
        u64_at(&self.0, 0x140)
    }

    /// The PFS0 of a PFS0 section: the last layer of its SHA-256 hash data (offset, size).
    pub fn pfs0_region(&self) -> io::Result<(u64, u64)> {
        let layers = u32_at(&self.0, 0x2C) as usize;
        if !(1..=5).contains(&layers) {
            return Err(invalid("NCA: bad PFS0 hash data"));
        }
        let o = 0x30 + (layers - 1) * 0x10;
        Ok((u64_at(&self.0, o), u64_at(&self.0, o + 8)))
    }

    /// The RomFS of a RomFS section: the last level of its IVFC tree (offset, size).
    pub fn romfs_region(&self) -> io::Result<(u64, u64)> {
        let levels = u32_at(&self.0, 0x14) as usize;
        if &self.0[8..12] != b"IVFC" || !(2..=7).contains(&levels) {
            return Err(invalid("NCA: RomFS section without IVFC hash data"));
        }
        let o = 0x18 + (levels - 2) * 0x18;
        Ok((u64_at(&self.0, o), u64_at(&self.0, o + 8)))
    }

    /// A BKTR section's tables: (offset, size, bucket tree header) of the indirect table and of
    /// the AES-CTR-EX table.
    pub fn patch_info(&self) -> [(u64, u64, &[u8]); 2] {
        let at = |o: usize| {
            (
                u64_at(&self.0, o),
                u64_at(&self.0, o + 8),
                &self.0[o + 0x10..o + 0x20],
            )
        };
        [at(0x100), at(0x120)]
    }
}

/// A section's AES-CTR decryption: `raw` is the encrypted section, starting at absolute NCA
/// offset `base`.
pub struct CtrStorage {
    raw: Shared,
    ctr: Ctr,
    base: u64,
    upper: u64,
}

impl CtrStorage {
    pub fn new(raw: Shared, key: &[u8; 16], base: u64, upper: u64) -> Self {
        Self {
            raw,
            ctr: Ctr::new(key),
            base,
            upper,
        }
    }

    /// Decrypt `buf` (read from `offset`) with the counter's upper half `upper`.
    pub(crate) fn read_with(&self, upper: u64, offset: u64, buf: &mut [u8]) -> io::Result<()> {
        check_range(self.raw.size(), offset, buf.len())?;
        let head = (offset % 16) as usize;
        let start = offset - head as u64;
        let iv = section_iv(upper, self.base + start);
        if head == 0 {
            self.raw.read_at(start, buf)?;
            self.ctr.apply(iv, buf);
        } else {
            let mut tmp = vec![0; head + buf.len()];
            self.raw.read_at(start, &mut tmp)?;
            self.ctr.apply(iv, &mut tmp);
            buf.copy_from_slice(&tmp[head..]);
        }
        Ok(())
    }
}

impl Storage for CtrStorage {
    fn size(&self) -> u64 {
        self.raw.size()
    }
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> io::Result<()> {
        self.read_with(self.upper, offset, buf)
    }
}

pub struct Nca {
    raw: Shared,
    header: Vec<u8>,
    /// The sections' AES-CTR key (title key or key area key 2).
    key: Option<[u8; 16]>,
}

impl Nca {
    /// The decrypted header only (enough to see the content type and program id).
    pub fn header(raw: &Shared, keys: &Keys) -> io::Result<Vec<u8>> {
        let mut h = raw.read_vec(0, HEADER_SIZE as u64)?;
        xts_decrypt_nintendo(keys.header_key()?, 0x200, 0, &mut h);
        if &h[0x200..0x204] != b"NCA3" {
            return Err(invalid(
                "NCA header did not decrypt (wrong header_key, or not an NCA3)",
            ));
        }
        Ok(h)
    }

    /// Open an NCA, decrypting its section key from the key area or a matching ticket.
    pub fn open(raw: Shared, keys: &Keys, tickets: &[Ticket]) -> io::Result<Self> {
        let header = Self::header(&raw, keys)?;
        let mut nca = Self {
            raw,
            header,
            key: None,
        };
        let generation = nca.key_generation();
        let rights_id: [u8; 16] = nca.header[0x230..0x240].try_into().unwrap();
        if rights_id != [0; 16] {
            let tik = tickets
                .iter()
                .find(|t| t.rights_id == rights_id)
                .ok_or_else(|| {
                    invalid("no ticket for this content in the NSP (it needs its .tik file)")
                })?;
            if tik.kind != 0 {
                return Err(invalid(
                    "the ticket is personalised to one console; dump the game with a common ticket",
                ));
            }
            nca.key = Some(ecb_decrypt(keys.titlekek(generation)?, &tik.title_key));
        } else {
            if nca.header[0x207] != 0 {
                return Err(invalid("NCA uses a system key area (not a game's content)"));
            }
            let kak = keys.key_area_application(generation)?;
            let enc: [u8; 16] = nca.header[0x320..0x330].try_into().unwrap();
            nca.key = Some(ecb_decrypt(kak, &enc));
        }
        Ok(nca)
    }

    pub fn content_type(&self) -> u8 {
        self.header[0x205]
    }
    pub fn program_id(&self) -> u64 {
        u64_at(&self.header, 0x210)
    }
    /// The master key revision the keys are picked by (the larger of the two header fields,
    /// counted from 1 when non-zero).
    fn key_generation(&self) -> u8 {
        self.header[0x206].max(self.header[0x220]).saturating_sub(1)
    }

    /// Section `i`: (absolute offset, size, FS header), or None when unused.
    pub fn section(&self, i: usize) -> Option<(u64, u64, FsHeader)> {
        let e = 0x240 + i * 0x10;
        let (start, end) = (
            u32_at(&self.header, e) as u64 * 0x200,
            u32_at(&self.header, e + 4) as u64 * 0x200,
        );
        (i < 4 && end > start).then(|| {
            let fs = FsHeader(self.header[0x400 + i * 0x200..0x600 + i * 0x200].to_vec());
            (start, end - start, fs)
        })
    }

    /// The first section of file system type `fs_type` (index, FS header).
    pub fn find_section(&self, fs_type: u8) -> Option<(usize, FsHeader)> {
        (0..4).find_map(|i| {
            self.section(i)
                .filter(|(_, _, fs)| fs.fs_type() == fs_type)
                .map(|(_, _, fs)| (i, fs))
        })
    }

    /// Section `i` as stored (still encrypted).
    pub fn raw_section(&self, i: usize) -> io::Result<(Shared, u64)> {
        let (offset, size, _) = self
            .section(i)
            .ok_or_else(|| invalid("NCA: missing section"))?;
        Ok((SubStorage::shared(self.raw.clone(), offset, size)?, offset))
    }

    /// Section `i` decrypted with plain AES-CTR (for an AES-CTR-EX section that is right only
    /// for its BKTR tables).
    pub fn ctr_section(&self, i: usize) -> io::Result<Arc<CtrStorage>> {
        let (_, _, fs) = self
            .section(i)
            .ok_or_else(|| invalid("NCA: missing section"))?;
        let (raw, offset) = self.raw_section(i)?;
        let key = self
            .key
            .as_ref()
            .ok_or_else(|| invalid("NCA: no section key"))?;
        Ok(Arc::new(CtrStorage::new(raw, key, offset, fs.ctr_upper())))
    }

    /// Section `i` decrypted (sections that are not AES-CTR-EX).
    pub fn section_storage(&self, i: usize) -> io::Result<Shared> {
        let (_, _, fs) = self
            .section(i)
            .ok_or_else(|| invalid("NCA: missing section"))?;
        match fs.encryption() {
            ENC_NONE => Ok(self.raw_section(i)?.0),
            ENC_CTR => Ok(self.ctr_section(i)?),
            ENC_CTR_EX => Err(invalid(
                "NCA: this is an update's patch section; open it with the base game",
            )),
            _ => Err(invalid("NCA: unsupported section encryption")),
        }
    }
}

impl Drop for Nca {
    fn drop(&mut self) {
        if let Some(k) = &mut self.key {
            *k = [0; 16];
        }
    }
}
