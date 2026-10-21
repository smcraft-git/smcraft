//! The player's own Switch dump, read in place: NSP (PFS0) → program NCA (header AES-XTS,
//! section key from the key area or the ticket's title key, sections AES-CTR) → ExeFS (PFS0) and
//! RomFS; with an update NSP, its ExeFS and its RomFS patched over the base one (BKTR: indirect
//! storage + AES-CTR-EX). Nothing is extracted to disk.
//!
//! [`Dump::open`] takes the keys by reference and keeps only the section keys it derived; the
//! [`DumpVfs`] then serves the RomFS and ExeFS to the import stages under virtual roots inside
//! the NSP's path, and passes everything else (the cache it writes) to the file system.
//!
//! This module (and the importer) are the only code that opens NSP, NCA or key files: the
//! runtime reads the asset pack alone (`core/crates/import/tests/rom_free.rs` checks that).

pub mod bktr;
pub mod crypto;
pub mod keys;
pub mod nca;
pub mod pfs0;
pub mod romfs;
pub mod storage;
pub mod ticket;

use std::io;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use bktr::{AesCtrExStorage, IndirectStorage};
use keys::Keys;
use nca::{CONTENT_PROGRAM, ENC_CTR_EX, FS_PFS0, FS_ROMFS, Nca};
use pfs0::Pfs0;
use romfs::RomFs;
use storage::{FileStorage, Shared, SubStorage, invalid};
use ticket::Ticket;

use crate::vfs::{StdFs, Vfs};

/// `e` with the file it came from in front.
fn in_file(path: &Path) -> impl Fn(io::Error) -> io::Error + '_ {
    move |e| io::Error::new(e.kind(), format!("{}: {e}", path.display()))
}

/// The program NCA of an NSP, opened.
fn program_nca(nsp: &Path, keys: &Keys) -> io::Result<Nca> {
    let file: Shared = Arc::new(FileStorage::open(nsp)?);
    let pfs = Pfs0::parse(file)?;
    let mut tickets = Vec::new();
    for e in pfs.entries.iter().filter(|e| e.name.ends_with(".tik")) {
        let s = pfs.open(e)?;
        tickets.push(Ticket::parse(&s.read_vec(0, s.size())?)?);
    }
    // the largest program NCA (an NSP has one; a few have extra program parts for manuals)
    let mut best: Option<(u64, Shared)> = None;
    for e in pfs.entries.iter().filter(|e| e.name.ends_with(".nca")) {
        let s = pfs.open(e)?;
        if Nca::header(&s, keys)?[0x205] == CONTENT_PROGRAM
            && best.as_ref().is_none_or(|b| e.size > b.0)
        {
            best = Some((e.size, s));
        }
    }
    let (_, raw) = best.ok_or_else(|| invalid("no program NCA in the NSP"))?;
    Nca::open(raw, keys, &tickets)
}

/// The ExeFS of a program NCA: its first PFS0 section that has `main`.
fn exefs(nca: &Nca) -> io::Result<Pfs0> {
    for i in 0..4 {
        let Some((_, _, fs)) = nca.section(i) else {
            continue;
        };
        if fs.fs_type() != FS_PFS0 {
            continue;
        }
        let (offset, size) = fs.pfs0_region()?;
        let pfs = Pfs0::parse(SubStorage::shared(nca.section_storage(i)?, offset, size)?)?;
        if pfs.find("main").is_some() {
            return Ok(pfs);
        }
    }
    Err(invalid("no ExeFS (with main) in the program NCA"))
}

/// A game dump opened for reading: its ExeFS and (patched) RomFS.
pub struct Dump {
    pub exefs: Pfs0,
    pub romfs: RomFs,
    pub program_id: u64,
}

impl Dump {
    /// Open the base game NSP and, optionally, an update NSP patched over it.
    pub fn open(base: &Path, update: Option<&Path>, keys: &Keys) -> io::Result<Self> {
        let (base_nca, base_section, base_fs) = Self::base(base, keys).map_err(in_file(base))?;
        let program_id = base_nca.program_id();
        let (exefs, romfs) = match update {
            None => {
                let (offset, size) = base_fs.romfs_region().map_err(in_file(base))?;
                (
                    exefs(&base_nca).map_err(in_file(base))?,
                    SubStorage::shared(base_section, offset, size)?,
                )
            }
            Some(update) => {
                Self::patch(update, keys, program_id, base_section).map_err(in_file(update))?
            }
        };
        Ok(Self {
            exefs,
            romfs: RomFs::parse(romfs)?,
            program_id,
        })
    }

    /// The base game's program NCA, its decrypted RomFS section and that section's header.
    fn base(base: &Path, keys: &Keys) -> io::Result<(Nca, Shared, nca::FsHeader)> {
        let nca = program_nca(base, keys)?;
        let (index, fs) = nca
            .find_section(FS_ROMFS)
            .ok_or_else(|| invalid("no RomFS in the program NCA"))?;
        let section = nca.section_storage(index)?;
        Ok((nca, section, fs))
    }

    /// The update's ExeFS and its RomFS, patched over the base RomFS section.
    fn patch(
        update: &Path,
        keys: &Keys,
        program_id: u64,
        base_section: Shared,
    ) -> io::Result<(Pfs0, Shared)> {
        let nca = program_nca(update, keys)?;
        // an update's program id is the game's, or the game's patch id (+0x800)
        if nca.program_id() & !0x800 != program_id & !0x800 {
            return Err(invalid(
                "the update is for a different game than the base NSP",
            ));
        }
        let (index, fs) = nca
            .find_section(FS_ROMFS)
            .ok_or_else(|| invalid("no RomFS in the program NCA"))?;
        if fs.encryption() != ENC_CTR_EX {
            return Err(invalid("not an update (its RomFS is not a BKTR patch)"));
        }
        let [indirect, ctr_ex] = fs.patch_info();
        let plain = nca.ctr_section(index)?;
        let data: Shared = Arc::new(AesCtrExStorage::new(plain, fs.ctr_upper(), ctr_ex)?);
        let patched: Shared = Arc::new(IndirectStorage::new(base_section, data, indirect)?);
        let (offset, size) = fs.romfs_region()?;
        Ok((exefs(&nca)?, SubStorage::shared(patched, offset, size)?))
    }
}

/// The import stages' file access over an opened dump: `<root>/romfs/...` and
/// `<root>/exefs/...` come from the dump, everything else is the file system. `root` is the base
/// NSP's own path, so no real folder can be shadowed.
pub struct DumpVfs {
    dump: Dump,
    romfs_root: PathBuf,
    exefs_root: PathBuf,
}

enum Place {
    RomFs(String),
    ExeFs(String),
    Outside,
}

/// `path` below `root`, as "a/b/c" (None when not below it).
fn below(path: &Path, root: &Path) -> Option<String> {
    let rest = path.strip_prefix(root).ok()?;
    let mut parts = Vec::new();
    for c in rest.components() {
        match c {
            Component::Normal(s) => parts.push(s.to_string_lossy().into_owned()),
            Component::CurDir => {}
            _ => return Some("\0".into()), // ".." etc.: no such file
        }
    }
    Some(parts.join("/"))
}

fn not_found() -> io::Error {
    io::ErrorKind::NotFound.into()
}

impl DumpVfs {
    pub fn new(dump: Dump, root: &Path) -> Self {
        Self {
            dump,
            romfs_root: root.join("romfs"),
            exefs_root: root.join("exefs"),
        }
    }
    /// The RomFS as the import stages name it.
    pub fn romfs_root(&self) -> &Path {
        &self.romfs_root
    }
    pub fn exefs_root(&self) -> &Path {
        &self.exefs_root
    }

    fn place(&self, path: &Path) -> Place {
        if let Some(p) = below(path, &self.romfs_root) {
            Place::RomFs(p)
        } else if let Some(p) = below(path, &self.exefs_root) {
            Place::ExeFs(p)
        } else {
            Place::Outside
        }
    }
}

fn read_only() -> io::Error {
    io::Error::new(
        io::ErrorKind::PermissionDenied,
        "the game dump is read-only",
    )
}

impl Vfs for DumpVfs {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        match self.place(path) {
            Place::RomFs(p) => self.dump.romfs.read(&p).unwrap_or_else(|| Err(not_found())),
            Place::ExeFs(p) => {
                let e = self.dump.exefs.find(&p).ok_or_else(not_found)?;
                let s = self.dump.exefs.open(e)?;
                s.read_vec(0, s.size())
            }
            Place::Outside => StdFs.read(path),
        }
    }
    fn write(&self, path: &Path, data: &[u8]) -> io::Result<()> {
        match self.place(path) {
            Place::Outside => StdFs.write(path, data),
            _ => Err(read_only()),
        }
    }
    fn read_dir(&self, dir: &Path) -> io::Result<Vec<PathBuf>> {
        match self.place(dir) {
            Place::RomFs(p) => Ok(self
                .dump
                .romfs
                .list(&p)
                .ok_or_else(not_found)?
                .iter()
                .map(|n| dir.join(n))
                .collect()),
            Place::ExeFs(p) if p.is_empty() => Ok(self
                .dump
                .exefs
                .entries
                .iter()
                .map(|e| dir.join(&e.name))
                .collect()),
            Place::ExeFs(_) => Err(not_found()),
            Place::Outside => StdFs.read_dir(dir),
        }
    }
    fn is_file(&self, path: &Path) -> bool {
        match self.place(path) {
            Place::RomFs(p) => self.dump.romfs.is_file(&p),
            Place::ExeFs(p) => self.dump.exefs.find(&p).is_some(),
            Place::Outside => StdFs.is_file(path),
        }
    }
    fn remove_dir_all(&self, dir: &Path) -> io::Result<()> {
        match self.place(dir) {
            Place::Outside => StdFs.remove_dir_all(dir),
            _ => Err(read_only()),
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::path::PathBuf;

    /// The owner's dump and keys (`MMK_NSP`, `MMK_UPDATE_NSP`, `MMK_PROD_KEYS`, else the usual
    /// places), or None (CI has no game data).
    pub(crate) fn owner_dump() -> Option<(PathBuf, PathBuf, PathBuf)> {
        let home = PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
        let env = |k: &str| std::env::var_os(k).map(PathBuf::from);
        let dev = home.join("Documents/Dev/Mario Maker");
        let base = env("MMK_NSP").unwrap_or_else(|| dev.join("MarioMaker2.NSP"));
        let update =
            env("MMK_UPDATE_NSP").unwrap_or_else(|| dev.join("Mario Maker2 Updatev3.0.3.nsp"));
        let keys = env("MMK_PROD_KEYS")
            .unwrap_or_else(|| home.join("Library/Application Support/Ryujinx/system/prod.keys"));
        (base.is_file() && update.is_file() && keys.is_file()).then_some((base, update, keys))
    }

    /// A program NCA (key generation 3, key area) with an ExeFS and a RomFS, encrypted with
    /// made-up keys, and those keys as a `prod.keys` text.
    fn synthetic_nca(exefs: &[u8], romfs: &[u8]) -> (Vec<u8>, String) {
        use crypto::{Ctr, ecb_encrypt, section_iv, xts_encrypt_nintendo};
        let (header_key, kak, key) = ([0x33u8; 32], [0x11u8; 16], [0x22u8; 16]);
        let pad = |mut v: Vec<u8>| {
            v.resize(v.len().div_ceil(0x200) * 0x200, 0);
            v
        };
        // each section: 0x200 bytes of (unused) hash data, then the payload
        let sections = [
            pad([&[0u8; 0x200][..], exefs].concat()),
            pad([&[0u8; 0x200][..], romfs].concat()),
        ];
        let mut h = vec![0u8; 0xC00];
        h[0x200..0x204].copy_from_slice(b"NCA3");
        h[0x220] = 3;
        h[0x210..0x218].copy_from_slice(&0x0100_0000_0000_1000u64.to_le_bytes());
        h[0x320..0x330].copy_from_slice(&ecb_encrypt(&kak, &key));
        let mut body = Vec::new();
        let mut at = 0xC00u64;
        for (i, (sec, len)) in sections.iter().zip([exefs.len(), romfs.len()]).enumerate() {
            let e = 0x240 + i * 0x10;
            h[e..e + 4].copy_from_slice(&((at / 0x200) as u32).to_le_bytes());
            h[e + 4..e + 8]
                .copy_from_slice(&(((at + sec.len() as u64) / 0x200) as u32).to_le_bytes());
            let fs = &mut h[0x400 + i * 0x200..0x600 + i * 0x200];
            fs[4] = nca::ENC_CTR;
            let upper = 0x0000_0007_0000_0000u64 | i as u64;
            fs[0x140..0x148].copy_from_slice(&upper.to_le_bytes());
            if i == 0 {
                fs[2] = FS_PFS0;
                fs[0x2C..0x30].copy_from_slice(&2u32.to_le_bytes());
                fs[0x40..0x48].copy_from_slice(&0x200u64.to_le_bytes());
                fs[0x48..0x50].copy_from_slice(&(len as u64).to_le_bytes());
            } else {
                fs[2] = FS_ROMFS;
                fs[8..12].copy_from_slice(b"IVFC");
                fs[0x14..0x18].copy_from_slice(&7u32.to_le_bytes());
                fs[0x90..0x98].copy_from_slice(&0x200u64.to_le_bytes());
                fs[0x98..0xA0].copy_from_slice(&(len as u64).to_le_bytes());
            }
            let mut enc = sec.clone();
            Ctr::new(&key).apply(section_iv(upper, at), &mut enc);
            body.extend(enc);
            at += sec.len() as u64;
        }
        xts_encrypt_nintendo(&header_key, 0x200, &mut h);
        let hex = |b: &[u8]| b.iter().map(|x| format!("{x:02x}")).collect::<String>();
        let keys = format!(
            "header_key = {}\nkey_area_key_application_02 = {}\n",
            hex(&header_key),
            hex(&kak)
        );
        ([h, body].concat(), keys)
    }

    #[test]
    fn synthetic_nsp_through_the_vfs() {
        let exefs = pfs0::tests::build(&[("main", b"NSO-ish"), ("main.npdm", b"meta")]);
        let romfs = romfs::tests::build(&[("Pack/Static.pack", b"static"), ("Course/a.szs", b"a")]);
        let (nca, keys) = synthetic_nca(&exefs, &romfs);
        let nsp = pfs0::tests::build(&[("0123.nca", &nca)]);
        let tmp = std::env::temp_dir().join(format!("mmk-nsp-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let path = tmp.join("game.nsp");
        std::fs::write(&path, &nsp).unwrap();

        let keys = Keys::parse(&keys).unwrap();
        let dump = Dump::open(&path, None, &keys).unwrap();
        assert_eq!(dump.program_id, 0x0100_0000_0000_1000);
        let fs = DumpVfs::new(dump, &path);
        let (r, x) = (fs.romfs_root().to_path_buf(), fs.exefs_root().to_path_buf());
        assert_eq!(fs.read(&r.join("Pack/Static.pack")).unwrap(), b"static");
        assert_eq!(fs.read(&x.join("main")).unwrap(), b"NSO-ish");
        assert!(fs.is_file(&r.join("Course/a.szs")) && !fs.is_file(&r.join("Course")));
        let mut root = fs.read_dir(&r).unwrap();
        root.sort();
        assert_eq!(root, [r.join("Course"), r.join("Pack")]);
        assert!(fs.read(&r.join("Pack/../Pack/Static.pack")).is_err());
        assert!(
            fs.write(&r.join("x"), b"x").is_err(),
            "the dump is read-only"
        );
        // everything else is the file system (where the cache goes)
        fs.write(&tmp.join("out/a.json"), b"{}").unwrap();
        assert_eq!(std::fs::read(tmp.join("out/a.json")).unwrap(), b"{}");

        // the wrong header key is caught, without showing any key
        let wrong = Keys::parse(&format!("header_key = {}\n", "44".repeat(32))).unwrap();
        let err = Dump::open(&path, None, &wrong).err().unwrap().to_string();
        assert!(err.contains("header_key") && !err.contains("4444"), "{err}");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// A reference extraction to compare with (hactool / Ryujinx output), if there.
    fn reference(name: &str) -> Option<PathBuf> {
        let home = PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
        Some(home.join("smm-work/extracted").join(name)).filter(|p| p.is_dir())
    }

    fn compare(fs: &RomFs, reference: &Path, files: &[&str]) {
        for f in files {
            let ours = fs.read(f).unwrap_or_else(|| panic!("{f} missing")).unwrap();
            let theirs = std::fs::read(reference.join(f)).unwrap();
            assert!(ours == theirs, "{f} differs from the reference extraction");
        }
    }

    fn count_files(dir: &Path) -> usize {
        std::fs::read_dir(dir)
            .unwrap()
            .flatten()
            .filter(|e| e.file_name() != ".DS_Store")
            .map(|e| {
                let p = e.path();
                if p.is_dir() { count_files(&p) } else { 1 }
            })
            .sum()
    }

    const SAMPLE: &[&str] = &[
        "Pack/Static.pack",
        "Pack/M1_Model.pack",
        "Course/quest_012.szs",
        "SLink2/SLink2DB.szs",
    ];

    #[test]
    fn owner_dump_reads_like_the_reference() {
        let Some((base, update, keys)) = owner_dump() else {
            eprintln!("skipped: no NSP / prod.keys");
            return;
        };
        let keys = Keys::load(&keys).unwrap();
        let plain = Dump::open(&base, None, &keys).unwrap();
        let patched = Dump::open(&base, Some(&update), &keys).unwrap();
        drop(keys);
        assert!(plain.romfs.files().count() > 1000);
        // MMK_COMPARE_ALL=1: every file, and no file missing on either side (minutes in release)
        let all = std::env::var_os("MMK_COMPARE_ALL").is_some();
        for (dump, name) in [(&plain, "smm2-romfs"), (&patched, "smm2-v303-romfs")] {
            let Some(r) = reference(name) else { continue };
            if !all {
                compare(&dump.romfs, &r, SAMPLE);
                continue;
            }
            let mut ours: Vec<&str> = dump.romfs.files().collect();
            ours.sort();
            compare(&dump.romfs, &r, &ours);
            assert_eq!(ours.len(), count_files(&r), "{name}: file count");
            eprintln!("{name}: {} files identical", ours.len());
        }
        for (dump, name) in [(&plain, "smm2-nsp/exefs"), (&patched, "smm2-v303-exefs")] {
            let Some(r) = reference(name) else { continue };
            for e in &dump.exefs.entries {
                let s = dump.exefs.open(e).unwrap();
                let ours = s.read_vec(0, s.size()).unwrap();
                assert!(
                    ours == std::fs::read(r.join(&e.name)).unwrap(),
                    "{name}/{}",
                    e.name
                );
            }
        }
    }
}
