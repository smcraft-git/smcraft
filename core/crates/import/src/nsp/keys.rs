//! The player's `prod.keys`, read once for an import. Only the keys the dump readers need are
//! kept (the NCA header key, the application key area keys, the title key encryption keys); the
//! rest of the file is dropped unread. Key values are never written, logged, formatted or put in
//! an error message, and the memory holding them is cleared when the [`Keys`] are dropped.

use std::fmt;
use std::io;

use super::storage::invalid;

/// Key generations the arrays hold (`_00` … `_1f`).
const GENERATIONS: usize = 0x20;

pub struct Keys {
    header: Option<[u8; 32]>,
    key_area_application: [Option<[u8; 16]>; GENERATIONS],
    titlekek: [Option<[u8; 16]>; GENERATIONS],
}

/// Parse `len` bytes of hex, or None (without saying what the text was).
fn parse_hex<const N: usize>(s: &str) -> Option<[u8; N]> {
    let s = s.as_bytes();
    if s.len() != N * 2 {
        return None;
    }
    let nibble = |c: u8| (c as char).to_digit(16).map(|d| d as u8);
    let mut out = [0u8; N];
    for (i, o) in out.iter_mut().enumerate() {
        *o = (nibble(s[2 * i])? << 4) | nibble(s[2 * i + 1])?;
    }
    Some(out)
}

/// `<prefix>XX` → the generation XX.
fn generation(name: &str, prefix: &str) -> Option<usize> {
    let g = name.strip_prefix(prefix)?;
    (g.len() == 2)
        .then(|| usize::from_str_radix(g, 16).ok())
        .flatten()
        .filter(|&g| g < GENERATIONS)
}

fn wipe(bytes: &mut [u8]) {
    for b in bytes {
        // volatile so the clearing is not optimised away as a dead store
        unsafe { std::ptr::write_volatile(b, 0) };
    }
}

impl Keys {
    /// Read a `prod.keys` file (`name = hex` lines). The file's text is cleared from memory once
    /// parsed.
    pub fn load(path: &std::path::Path) -> io::Result<Self> {
        let mut raw = std::fs::read(path)?;
        let keys = std::str::from_utf8(&raw)
            .map_err(|_| invalid("prod.keys is not a text file"))
            .and_then(Self::parse);
        wipe(&mut raw);
        keys
    }

    pub fn parse(text: &str) -> io::Result<Self> {
        let mut keys = Keys {
            header: None,
            key_area_application: [None; GENERATIONS],
            titlekek: [None; GENERATIONS],
        };
        for (n, line) in text.lines().enumerate() {
            let Some((name, value)) = line.split_once('=') else {
                continue;
            };
            let (name, value) = (name.trim().to_ascii_lowercase(), value.trim());
            let bad = || invalid(format!("prod.keys line {}: {name} is not valid hex", n + 1));
            if name == "header_key" {
                keys.header = Some(parse_hex(value).ok_or_else(bad)?);
            } else if let Some(g) = generation(&name, "key_area_key_application_") {
                keys.key_area_application[g] = Some(parse_hex(value).ok_or_else(bad)?);
            } else if let Some(g) = generation(&name, "titlekek_") {
                keys.titlekek[g] = Some(parse_hex(value).ok_or_else(bad)?);
            }
        }
        if keys.header.is_none() {
            return Err(invalid("prod.keys has no header_key"));
        }
        Ok(keys)
    }

    pub(crate) fn header_key(&self) -> io::Result<&[u8; 32]> {
        self.header
            .as_ref()
            .ok_or_else(|| invalid("prod.keys has no header_key"))
    }

    pub(crate) fn key_area_application(&self, generation: u8) -> io::Result<&[u8; 16]> {
        Self::pick(
            &self.key_area_application,
            generation,
            "key_area_key_application",
        )
    }

    pub(crate) fn titlekek(&self, generation: u8) -> io::Result<&[u8; 16]> {
        Self::pick(&self.titlekek, generation, "titlekek")
    }

    fn pick<'a>(
        set: &'a [Option<[u8; 16]>; GENERATIONS],
        generation: u8,
        name: &str,
    ) -> io::Result<&'a [u8; 16]> {
        set.get(generation as usize)
            .and_then(|k| k.as_ref())
            .ok_or_else(|| {
                invalid(format!(
                    "prod.keys has no {name}_{generation:02x} (this dump needs a newer prod.keys)"
                ))
            })
    }
}

impl Drop for Keys {
    fn drop(&mut self) {
        if let Some(k) = &mut self.header {
            wipe(k);
        }
        for k in self
            .key_area_application
            .iter_mut()
            .chain(self.titlekek.iter_mut())
            .flatten()
        {
            wipe(k);
        }
    }
}

/// Which keys are there, never their values.
impl fmt::Debug for Keys {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let count = |s: &[Option<[u8; 16]>]| s.iter().filter(|k| k.is_some()).count();
        f.debug_struct("Keys")
            .field("header_key", &self.header.is_some())
            .field(
                "key_area_key_application",
                &count(&self.key_area_application),
            )
            .field("titlekek", &count(&self.titlekek))
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(name: &str, byte: u8, len: usize) -> String {
        format!("{name} = {}\n", format!("{byte:02x}").repeat(len))
    }

    #[test]
    fn keeps_only_the_needed_keys_and_never_shows_them() {
        let text = [
            line("header_key", 0xA1, 32),
            line("key_area_key_application_0b", 0xB2, 16),
            line("TITLEKEK_00", 0xC3, 16),
            line("master_key_00", 0xD4, 16),
            "; a comment\n".into(),
        ]
        .concat();
        let keys = Keys::parse(&text).unwrap();
        assert_eq!(keys.header_key().unwrap(), &[0xA1; 32]);
        assert_eq!(keys.key_area_application(0x0b).unwrap(), &[0xB2; 16]);
        assert_eq!(keys.titlekek(0).unwrap(), &[0xC3; 16]);
        let missing = keys.titlekek(1).unwrap_err().to_string();
        assert!(missing.contains("titlekek_01"), "{missing}");
        let shown = format!("{keys:?}");
        assert!(!shown.to_ascii_lowercase().contains("a1a1"), "{shown}");

        let bad = Keys::parse(&format!("header_key = {}zz\n", "a1".repeat(31))).unwrap_err();
        assert!(!bad.to_string().contains("a1a1"), "{bad}");
        assert!(
            Keys::parse(&line("titlekek_00", 1, 16)).is_err(),
            "no header_key"
        );
    }
}
