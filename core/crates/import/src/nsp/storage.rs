//! Random-access byte storages the dump readers are layered on: a file, a window into another
//! storage, bytes in memory. The decrypting and patching layers (`nca`, `bktr`) are storages too,
//! so a RomFS file is read straight out of the NSP without extracting anything to disk.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::{Arc, Mutex};

/// Bytes that can be read at any offset.
pub trait Storage: Send + Sync {
    /// Size in bytes.
    fn size(&self) -> u64;
    /// Fill `buf` from `offset`; reading past the end is an error.
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> io::Result<()>;

    fn read_vec(&self, offset: u64, len: u64) -> io::Result<Vec<u8>> {
        let len = usize::try_from(len).map_err(|_| invalid("read too large"))?;
        let mut buf = vec![0; len];
        self.read_at(offset, &mut buf)?;
        Ok(buf)
    }
}

pub type Shared = Arc<dyn Storage>;

/// A malformed dump (the message names the structure, never key material).
pub(crate) fn invalid(msg: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg.into())
}

/// `offset..offset + len` lies inside a storage of `size` bytes.
pub(crate) fn check_range(size: u64, offset: u64, len: usize) -> io::Result<()> {
    match offset.checked_add(len as u64) {
        Some(end) if end <= size => Ok(()),
        _ => Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "read past the end of a storage",
        )),
    }
}

/// A file on disk.
pub struct FileStorage {
    file: Mutex<File>,
    size: u64,
}

impl FileStorage {
    pub fn open(path: &Path) -> io::Result<Self> {
        let file = File::open(path)?;
        let size = file.metadata()?.len();
        Ok(Self {
            file: Mutex::new(file),
            size,
        })
    }
}

impl Storage for FileStorage {
    fn size(&self) -> u64 {
        self.size
    }
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> io::Result<()> {
        check_range(self.size, offset, buf.len())?;
        let mut f = self.file.lock().unwrap_or_else(|e| e.into_inner());
        f.seek(SeekFrom::Start(offset))?;
        f.read_exact(buf)
    }
}

/// Bytes in memory.
pub struct MemStorage(pub Vec<u8>);

impl Storage for MemStorage {
    fn size(&self) -> u64 {
        self.0.len() as u64
    }
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> io::Result<()> {
        check_range(self.size(), offset, buf.len())?;
        let o = offset as usize;
        buf.copy_from_slice(&self.0[o..o + buf.len()]);
        Ok(())
    }
}

/// `len` bytes of another storage, from `offset`.
pub struct SubStorage {
    inner: Shared,
    offset: u64,
    size: u64,
}

impl SubStorage {
    pub fn new(inner: Shared, offset: u64, size: u64) -> io::Result<Self> {
        match offset.checked_add(size) {
            Some(end) if end <= inner.size() => Ok(Self {
                inner,
                offset,
                size,
            }),
            _ => Err(invalid("region outside its container")),
        }
    }
    pub fn shared(inner: Shared, offset: u64, size: u64) -> io::Result<Shared> {
        Ok(Arc::new(Self::new(inner, offset, size)?))
    }
}

impl Storage for SubStorage {
    fn size(&self) -> u64 {
        self.size
    }
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> io::Result<()> {
        check_range(self.size, offset, buf.len())?;
        self.inner.read_at(self.offset + offset, buf)
    }
}

/// Little-endian fields of a header in memory.
pub(crate) fn u32_at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}
pub(crate) fn u64_at(b: &[u8], o: usize) -> u64 {
    u64::from_le_bytes(b[o..o + 8].try_into().unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_and_bounds() {
        let mem: Shared = Arc::new(MemStorage((0..100u8).collect()));
        let sub = SubStorage::new(mem.clone(), 10, 20).unwrap();
        assert_eq!(sub.read_vec(5, 3).unwrap(), [15, 16, 17]);
        assert!(sub.read_vec(18, 3).is_err());
        assert!(SubStorage::new(mem, 90, 11).is_err());
    }
}
