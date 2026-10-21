//! PFS0 ("partition file system"): the container of an NSP (NCAs, tickets) and of an ExeFS
//! (`main`, `main.npdm`, `rtld`, `sdk`). A 0x10-byte header (magic, file count, string table
//! size), 0x18-byte entries (data offset, size, name offset), the name strings, then the data.

use std::io;

use super::storage::{Shared, SubStorage, invalid, u32_at, u64_at};

#[derive(Clone, Debug)]
pub struct Entry {
    pub name: String,
    /// Offset in the PFS0's storage.
    pub offset: u64,
    pub size: u64,
}

pub struct Pfs0 {
    storage: Shared,
    pub entries: Vec<Entry>,
}

impl Pfs0 {
    pub fn parse(storage: Shared) -> io::Result<Self> {
        let head = storage.read_vec(0, 0x10)?;
        if &head[..4] != b"PFS0" {
            return Err(invalid("not a PFS0 / NSP file"));
        }
        let (count, strings) = (u32_at(&head, 4) as u64, u32_at(&head, 8) as u64);
        if count > 0x10000 {
            return Err(invalid("PFS0: implausible file count"));
        }
        let table = storage.read_vec(0x10, count * 0x18 + strings)?;
        let names = &table[count as usize * 0x18..];
        let data_start = 0x10 + count * 0x18 + strings;
        let mut entries = Vec::new();
        for i in 0..count as usize {
            let e = &table[i * 0x18..];
            let name_off = u32_at(e, 0x10) as usize;
            let name = names
                .get(name_off..)
                .and_then(|n| n.split(|&b| b == 0).next())
                .ok_or_else(|| invalid("PFS0: name outside the string table"))?;
            entries.push(Entry {
                name: String::from_utf8_lossy(name).into_owned(),
                offset: data_start + u64_at(e, 0),
                size: u64_at(e, 8),
            });
        }
        Ok(Self { storage, entries })
    }

    pub fn find(&self, name: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.name == name)
    }

    /// A file's bytes as a storage.
    pub fn open(&self, entry: &Entry) -> io::Result<Shared> {
        SubStorage::shared(self.storage.clone(), entry.offset, entry.size)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::nsp::storage::MemStorage;
    use std::sync::Arc;

    /// A PFS0 holding `files`.
    pub(crate) fn build(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut names = Vec::new();
        let mut entries = Vec::new();
        let mut data = Vec::new();
        for (name, bytes) in files {
            entries.extend_from_slice(&(data.len() as u64).to_le_bytes());
            entries.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
            entries.extend_from_slice(&(names.len() as u32).to_le_bytes());
            entries.extend_from_slice(&0u32.to_le_bytes());
            names.extend_from_slice(name.as_bytes());
            names.push(0);
            data.extend_from_slice(bytes);
        }
        while names.len() % 0x10 != 0 {
            names.push(0);
        }
        let mut out = b"PFS0".to_vec();
        out.extend_from_slice(&(files.len() as u32).to_le_bytes());
        out.extend_from_slice(&(names.len() as u32).to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend(entries);
        out.extend(names);
        out.extend(data);
        out
    }

    #[test]
    fn lists_and_opens_files() {
        let p = Pfs0::parse(Arc::new(MemStorage(build(&[
            ("main", b"abc"),
            ("main.npdm", b"defg"),
        ]))))
        .unwrap();
        assert_eq!(p.entries.len(), 2);
        let e = p.find("main.npdm").unwrap();
        assert_eq!(p.open(e).unwrap().read_vec(0, 4).unwrap(), b"defg");
        assert!(Pfs0::parse(Arc::new(MemStorage(vec![0; 0x20]))).is_err());
    }
}
