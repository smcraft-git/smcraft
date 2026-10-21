//! Binary data as text for JSON values: hex, with runs of zero bytes written `z<count>;`.
//! Course data is mostly zeros, so a course takes a few KB instead of 750 (the Course Maker's
//! draft, the upload call between the front end and its host).

/// `b` as text.
pub fn hex_rle(b: &[u8]) -> String {
    let mut out = String::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == 0 {
            let mut n = 0;
            while i < b.len() && b[i] == 0 {
                n += 1;
                i += 1;
            }
            out.push_str(&format!("z{n};"));
        } else {
            out.push_str(&format!("{:02x}", b[i]));
            i += 1;
        }
    }
    out
}

/// The bytes of [`hex_rle`] text (None when it isn't).
pub fn un_hex_rle(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'z' {
            let end = s[i..].find(';')? + i;
            let n: usize = s[i + 1..end].parse().ok()?;
            out.extend(std::iter::repeat_n(0u8, n));
            i = end + 1;
        } else {
            out.push(u8::from_str_radix(s.get(i..i + 2)?, 16).ok()?);
            i += 2;
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let b = [0u8, 0, 0, 1, 0xff, 0, 7];
        let t = hex_rle(&b);
        assert_eq!(t, "z3;01ffz1;07");
        assert_eq!(un_hex_rle(&t).unwrap(), b);
        assert!(un_hex_rle("zx;").is_none());
    }
}
