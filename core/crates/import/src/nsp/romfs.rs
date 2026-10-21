//! RomFS: a 0x50-byte header (the offsets and sizes of the directory hash table, directory
//! entries, file hash table, file entries, and the file data), then the tables. A directory
//! entry links to its parent, next sibling, first child directory and first file; a file entry
//! to its parent and next sibling, with its data offset and size. Unused links are 0xFFFFFFFF.
//! The whole tree is indexed when opened; files are read on demand.

use std::collections::HashMap;
use std::io;

use super::storage::{Shared, invalid, u32_at, u64_at};

const NONE: u32 = 0xFFFF_FFFF;

pub struct RomFs {
    data: Shared,
    /// "dir/file" → (offset in `data`, size).
    files: HashMap<String, (u64, u64)>,
    /// "dir" ("" for the root) → its entries' names.
    dirs: HashMap<String, Vec<String>>,
}

fn name_at(table: &[u8], at: usize, len_field: usize, name: usize) -> io::Result<String> {
    let len = u32_at(table, at + len_field) as usize;
    table
        .get(at + name..at + name + len)
        .map(|n| String::from_utf8_lossy(n).into_owned())
        .ok_or_else(|| invalid("RomFS: name outside its table"))
}

impl RomFs {
    pub fn parse(data: Shared) -> io::Result<Self> {
        let h = data.read_vec(0, 0x50)?;
        if u64_at(&h, 0) != 0x50 {
            return Err(invalid("not a RomFS (header size is not 0x50)"));
        }
        let dir_table = data.read_vec(u64_at(&h, 0x18), u64_at(&h, 0x20))?;
        let file_table = data.read_vec(u64_at(&h, 0x38), u64_at(&h, 0x40))?;
        let data_offset = u64_at(&h, 0x48);
        let mut fs = Self {
            data,
            files: HashMap::new(),
            dirs: HashMap::new(),
        };
        let entry = |table: &[u8], at: u32, size: usize| {
            let at = at as usize;
            (at + size <= table.len())
                .then_some(at)
                .ok_or_else(|| invalid("RomFS: entry outside its table"))
        };
        // (directory entry, its path)
        let mut stack = vec![(0u32, String::new())];
        let mut visited = 0usize;
        while let Some((dir, path)) = stack.pop() {
            visited += 1;
            if visited > dir_table.len() / 0x18 + 1 {
                return Err(invalid("RomFS: directory loop"));
            }
            let d = entry(&dir_table, dir, 0x18)?;
            let mut names = Vec::new();
            let join = |name: &str| {
                if path.is_empty() {
                    name.to_string()
                } else {
                    format!("{path}/{name}")
                }
            };
            let mut child = u32_at(&dir_table, d + 8);
            while child != NONE {
                let c = entry(&dir_table, child, 0x18)?;
                let name = name_at(&dir_table, c, 0x14, 0x18)?;
                stack.push((child, join(&name)));
                names.push(name);
                child = u32_at(&dir_table, c + 4);
                if names.len() > dir_table.len() {
                    return Err(invalid("RomFS: sibling loop"));
                }
            }
            let mut file = u32_at(&dir_table, d + 0xC);
            while file != NONE {
                let f = entry(&file_table, file, 0x20)?;
                let name = name_at(&file_table, f, 0x1C, 0x20)?;
                let (offset, size) = (u64_at(&file_table, f + 8), u64_at(&file_table, f + 0x10));
                fs.files.insert(join(&name), (data_offset + offset, size));
                names.push(name);
                file = u32_at(&file_table, f + 4);
                if names.len() > file_table.len() + dir_table.len() {
                    return Err(invalid("RomFS: sibling loop"));
                }
            }
            fs.dirs.insert(path, names);
        }
        Ok(fs)
    }

    /// A file's bytes, or None when there is no such file.
    pub fn read(&self, path: &str) -> Option<io::Result<Vec<u8>>> {
        let &(offset, size) = self.files.get(path)?;
        Some(self.data.read_vec(offset, size))
    }

    pub fn is_file(&self, path: &str) -> bool {
        self.files.contains_key(path)
    }

    /// The names in a directory, or None when there is no such directory.
    pub fn list(&self, dir: &str) -> Option<&[String]> {
        self.dirs.get(dir).map(Vec::as_slice)
    }

    /// Every file path, in no particular order.
    pub fn files(&self) -> impl Iterator<Item = &str> {
        self.files.keys().map(String::as_str)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::nsp::storage::MemStorage;
    use std::sync::Arc;

    /// A RomFS with `files` ("dir/sub/name") in it (no hash tables: the reader does not use them).
    pub(crate) fn build(files: &[(&str, &[u8])]) -> Vec<u8> {
        // directories by path, in creation order: (path, parent index)
        let mut dirs: Vec<(String, usize)> = vec![(String::new(), 0)];
        let dir_of = |path: &str, dirs: &mut Vec<(String, usize)>| -> usize {
            let mut cur = 0;
            let mut acc = String::new();
            for part in path.split('/') {
                acc = if acc.is_empty() {
                    part.into()
                } else {
                    format!("{acc}/{part}")
                };
                cur = match dirs.iter().position(|(p, _)| *p == acc) {
                    Some(i) => i,
                    None => {
                        dirs.push((acc.clone(), cur));
                        dirs.len() - 1
                    }
                };
            }
            cur
        };
        let mut placed = Vec::new();
        for (path, data) in files {
            let (dir, name) = match path.rsplit_once('/') {
                Some((d, n)) => (dir_of(d, &mut dirs), n),
                None => (0, *path),
            };
            placed.push((dir, name, *data));
        }
        let pad4 = |n: usize| n.div_ceil(4) * 4;
        // offsets of the entries
        let mut dir_offs = Vec::new();
        let mut o = 0;
        for (p, _) in &dirs {
            dir_offs.push(o as u32);
            o += 0x18 + pad4(p.rsplit('/').next().unwrap().len());
        }
        let mut file_offs = Vec::new();
        o = 0;
        for (_, name, _) in &placed {
            file_offs.push(o as u32);
            o += 0x20 + pad4(name.len());
        }
        let first = |pred: &dyn Fn(usize) -> bool, n: usize, offs: &[u32]| {
            (0..n).find(|&i| pred(i)).map_or(NONE, |i| offs[i])
        };
        let mut dir_table = Vec::new();
        for (i, (p, parent)) in dirs.iter().enumerate() {
            let name = if i == 0 {
                ""
            } else {
                p.rsplit('/').next().unwrap()
            };
            let sibling = first(
                &|j| j > i && j != 0 && dirs[j].1 == *parent && i != 0,
                dirs.len(),
                &dir_offs,
            );
            let child = first(&|j| j != 0 && dirs[j].1 == i, dirs.len(), &dir_offs);
            let file = first(&|j| placed[j].0 == i, placed.len(), &file_offs);
            for v in [
                dir_offs[*parent],
                sibling,
                child,
                file,
                NONE,
                name.len() as u32,
            ] {
                dir_table.extend_from_slice(&v.to_le_bytes());
            }
            dir_table.extend_from_slice(name.as_bytes());
            dir_table.resize(pad4(dir_table.len()), 0);
        }
        let mut file_table = Vec::new();
        let mut blob = Vec::new();
        for (i, (dir, name, data)) in placed.iter().enumerate() {
            let sibling = first(&|j| j > i && placed[j].0 == *dir, placed.len(), &file_offs);
            file_table.extend_from_slice(&dir_offs[*dir].to_le_bytes());
            file_table.extend_from_slice(&sibling.to_le_bytes());
            file_table.extend_from_slice(&(blob.len() as u64).to_le_bytes());
            file_table.extend_from_slice(&(data.len() as u64).to_le_bytes());
            file_table.extend_from_slice(&NONE.to_le_bytes());
            file_table.extend_from_slice(&(name.len() as u32).to_le_bytes());
            file_table.extend_from_slice(name.as_bytes());
            file_table.resize(pad4(file_table.len()), 0);
            blob.extend_from_slice(data);
        }
        let dir_at = 0x50u64;
        let file_at = dir_at + dir_table.len() as u64;
        let data_at = file_at + file_table.len() as u64;
        let mut out = Vec::new();
        for v in [
            0x50,
            0,
            0,
            dir_at,
            dir_table.len() as u64,
            0,
            0,
            file_at,
            file_table.len() as u64,
            data_at,
        ] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out.extend(dir_table);
        out.extend(file_table);
        out.extend(blob);
        out
    }

    #[test]
    fn indexes_the_tree() {
        let fs = RomFs::parse(Arc::new(MemStorage(build(&[
            ("Pack/Static.pack", b"static"),
            ("Pack/M1_Model.pack", b"m1"),
            ("Layout/A/b.szs", b"layout"),
            ("top.bin", b"t"),
        ]))))
        .unwrap();
        assert_eq!(fs.read("Pack/M1_Model.pack").unwrap().unwrap(), b"m1");
        assert_eq!(fs.read("Layout/A/b.szs").unwrap().unwrap(), b"layout");
        assert!(fs.is_file("top.bin") && !fs.is_file("Pack"));
        let mut root = fs.list("").unwrap().to_vec();
        root.sort();
        assert_eq!(root, ["Layout", "Pack", "top.bin"]);
        assert_eq!(fs.list("Layout/A").unwrap(), ["b.szs"]);
        assert!(fs.read("nope").is_none());
    }
}
