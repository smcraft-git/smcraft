//! Tickets (`.tik` in an NSP): the encrypted title key of content with a rights id. Only common
//! tickets can be used; a personalised one is encrypted for one console and needs that console's
//! own RSA key, which the importer does not take.

use std::io;

use super::storage::{invalid, u32_at};

pub struct Ticket {
    pub rights_id: [u8; 16],
    /// The title key, encrypted with `titlekek_XX`.
    pub(crate) title_key: [u8; 16],
    /// 0 common, 1 personalised.
    pub kind: u8,
}

impl Ticket {
    pub fn parse(t: &[u8]) -> io::Result<Self> {
        if t.len() < 4 {
            return Err(invalid("ticket too short"));
        }
        // signature block: type, signature, padding to 0x40
        let sig = match u32_at(t, 0) {
            0x10000 | 0x10003 => 0x200 + 0x3C,
            0x10001 | 0x10004 => 0x100 + 0x3C,
            0x10002 | 0x10005 => 0x3C + 0x40,
            _ => return Err(invalid("ticket: unknown signature type")),
        };
        let body = 4 + sig;
        if t.len() < body + 0x170 {
            return Err(invalid("ticket too short"));
        }
        Ok(Self {
            rights_id: t[body + 0x160..body + 0x170].try_into().unwrap(),
            title_key: t[body + 0x40..body + 0x50].try_into().unwrap(),
            kind: t[body + 0x141],
        })
    }
}

impl Drop for Ticket {
    fn drop(&mut self) {
        self.title_key = [0; 16];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_common_ticket() {
        let mut t = vec![0u8; 0x2C0];
        t[..4].copy_from_slice(&0x10004u32.to_le_bytes());
        t[0x180..0x190].fill(7);
        t[0x2A0..0x2B0].fill(9);
        let tik = Ticket::parse(&t).unwrap();
        assert_eq!(
            (tik.rights_id, tik.title_key, tik.kind),
            ([9; 16], [7; 16], 0)
        );
        assert!(Ticket::parse(&t[..0x200]).is_err());
    }
}
