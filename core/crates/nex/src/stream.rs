//! NEX's serialisation (little-endian), in the Switch (NEX 4) profile: 8-byte PIDs and a
//! structure header ([u8 version][u32 length]) in front of every class level of a structure.
//!
//! Types: integers, bool, float/double, String (u16 length counting a NUL terminator),
//! Buffer (u32 length), qBuffer (u16 length), List (u32 count), Map (u32 count of pairs),
//! PID, DateTime, Variant, ResultRange, structures and the any-data holder.

use crate::Error;
use crate::station::StationUrl;

/// A packed NEX date and time: seconds | minutes << 6 | hours << 12 | day << 17 | month << 22
/// | year << 26.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DateTime(pub u64);

impl DateTime {
    pub fn new(year: u64, month: u64, day: u64, hour: u64, minute: u64, second: u64) -> Self {
        Self(second | minute << 6 | hour << 12 | day << 17 | month << 22 | year << 26)
    }

    /// The current UTC time.
    pub fn now() -> Self {
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let (days, rem) = (secs / 86_400, secs % 86_400);
        // days since 1970-01-01 to a civil date (Howard Hinnant's algorithm)
        let z = days as i64 + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let day = doy - (153 * mp + 2) / 5 + 1;
        let month = if mp < 10 { mp + 3 } else { mp - 9 };
        let year = yoe + era * 400 + i64::from(month <= 2);
        Self::new(
            year as u64,
            month as u64,
            day as u64,
            rem / 3600,
            rem / 60 % 60,
            rem % 60,
        )
    }

    pub fn year(self) -> u64 {
        self.0 >> 26
    }
}

/// A tagged value (matchmake parameters).
#[derive(Clone, Debug, PartialEq)]
pub enum Variant {
    None,
    I64(i64),
    F64(f64),
    Bool(bool),
    Str(String),
    DateTime(DateTime),
    U64(u64),
}

/// A NEX structure: one or more class levels (base first), each serialised on its own and,
/// with structure headers, framed with its version and length.
pub trait Structure: Default {
    /// The version of each class level, base first.
    const VERSIONS: &'static [u8] = &[0];
    fn save(&self, level: usize, w: &mut Writer);
    fn load(&mut self, level: usize, r: &mut Reader) -> Result<(), Error>;
}

/// An offset / size window for list queries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResultRange {
    pub offset: u32,
    pub size: u32,
}

impl Default for ResultRange {
    fn default() -> Self {
        Self {
            offset: 0,
            size: 10,
        }
    }
}

impl Structure for ResultRange {
    fn save(&self, _: usize, w: &mut Writer) {
        w.u32(self.offset);
        w.u32(self.size);
    }
    fn load(&mut self, _: usize, r: &mut Reader) -> Result<(), Error> {
        self.offset = r.u32()?;
        self.size = r.u32()?;
        Ok(())
    }
}

/// Builds a NEX byte stream.
#[derive(Clone, Debug)]
pub struct Writer {
    buf: Vec<u8>,
    /// Frame each structure level with [version][length] (true on the Switch).
    pub struct_header: bool,
}

impl Default for Writer {
    fn default() -> Self {
        Self::new()
    }
}

impl Writer {
    pub fn new() -> Self {
        Self {
            buf: Vec::new(),
            struct_header: true,
        }
    }

    fn sub(&self) -> Self {
        Self {
            buf: Vec::new(),
            struct_header: self.struct_header,
        }
    }

    pub fn bytes(&self) -> &[u8] {
        &self.buf
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.buf
    }

    pub fn raw(&mut self, b: &[u8]) -> &mut Self {
        self.buf.extend_from_slice(b);
        self
    }
    pub fn u8(&mut self, v: u8) -> &mut Self {
        self.raw(&[v])
    }
    pub fn u16(&mut self, v: u16) -> &mut Self {
        self.raw(&v.to_le_bytes())
    }
    pub fn u32(&mut self, v: u32) -> &mut Self {
        self.raw(&v.to_le_bytes())
    }
    pub fn u64(&mut self, v: u64) -> &mut Self {
        self.raw(&v.to_le_bytes())
    }
    pub fn i32(&mut self, v: i32) -> &mut Self {
        self.raw(&v.to_le_bytes())
    }
    pub fn i64(&mut self, v: i64) -> &mut Self {
        self.raw(&v.to_le_bytes())
    }
    pub fn f64(&mut self, v: f64) -> &mut Self {
        self.raw(&v.to_le_bytes())
    }
    pub fn bool(&mut self, v: bool) -> &mut Self {
        self.u8(v as u8)
    }
    /// A principal ID (8 bytes on the Switch).
    pub fn pid(&mut self, v: u64) -> &mut Self {
        self.u64(v)
    }
    /// A String: u16 length including the NUL terminator, the UTF-8 bytes, the NUL.
    pub fn string(&mut self, s: &str) -> &mut Self {
        self.u16(s.len() as u16 + 1).raw(s.as_bytes()).u8(0)
    }
    pub fn buffer(&mut self, b: &[u8]) -> &mut Self {
        self.u32(b.len() as u32).raw(b)
    }
    pub fn qbuffer(&mut self, b: &[u8]) -> &mut Self {
        self.u16(b.len() as u16).raw(b)
    }
    pub fn datetime(&mut self, d: DateTime) -> &mut Self {
        self.u64(d.0)
    }
    pub fn station(&mut self, u: &StationUrl) -> &mut Self {
        self.string(&u.to_string())
    }
    pub fn list<T>(&mut self, items: &[T], mut f: impl FnMut(&mut Self, &T)) -> &mut Self {
        self.u32(items.len() as u32);
        for it in items {
            f(self, it);
        }
        self
    }
    /// A Map: written in the given order (the pairs of a map are not sorted on the wire).
    pub fn map<K, V>(
        &mut self,
        pairs: &[(K, V)],
        mut fk: impl FnMut(&mut Self, &K),
        mut fv: impl FnMut(&mut Self, &V),
    ) -> &mut Self {
        self.u32(pairs.len() as u32);
        for (k, v) in pairs {
            fk(self, k);
            fv(self, v);
        }
        self
    }
    pub fn variant(&mut self, v: &Variant) -> &mut Self {
        match v {
            Variant::None => self.u8(0),
            Variant::I64(x) => self.u8(1).i64(*x),
            Variant::F64(x) => self.u8(2).f64(*x),
            Variant::Bool(x) => self.u8(3).bool(*x),
            Variant::Str(x) => self.u8(4).string(x),
            Variant::DateTime(x) => self.u8(5).datetime(*x),
            Variant::U64(x) => self.u8(6).u64(*x),
        }
    }
    pub fn structure<S: Structure>(&mut self, s: &S) -> &mut Self {
        for (level, &version) in S::VERSIONS.iter().enumerate() {
            if self.struct_header {
                let mut sub = self.sub();
                s.save(level, &mut sub);
                self.u8(version).buffer(&sub.buf);
            } else {
                s.save(level, self);
            }
        }
        self
    }
    /// An any-data holder: the class name, the holder length (the buffer's + 4), then the
    /// structure in a Buffer.
    pub fn any_data<S: Structure>(&mut self, name: &str, s: &S) -> &mut Self {
        let mut sub = self.sub();
        sub.structure(s);
        self.string(name)
            .u32(sub.buf.len() as u32 + 4)
            .buffer(&sub.buf)
    }
}

/// Reads a NEX byte stream.
#[derive(Clone, Debug)]
pub struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
    pub struct_header: bool,
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            pos: 0,
            struct_header: true,
        }
    }

    pub fn remaining(&self) -> usize {
        self.data.len() - self.pos
    }

    pub fn is_empty(&self) -> bool {
        self.remaining() == 0
    }

    pub fn take(&mut self, n: usize) -> Result<&'a [u8], Error> {
        if self.remaining() < n {
            return Err(Error::Truncated);
        }
        let b = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(b)
    }

    pub fn rest(&mut self) -> &'a [u8] {
        let b = &self.data[self.pos..];
        self.pos = self.data.len();
        b
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], Error> {
        Ok(self.take(N)?.try_into().unwrap())
    }
    pub fn u8(&mut self) -> Result<u8, Error> {
        Ok(self.take(1)?[0])
    }
    pub fn u16(&mut self) -> Result<u16, Error> {
        self.array().map(u16::from_le_bytes)
    }
    pub fn u32(&mut self) -> Result<u32, Error> {
        self.array().map(u32::from_le_bytes)
    }
    pub fn u64(&mut self) -> Result<u64, Error> {
        self.array().map(u64::from_le_bytes)
    }
    pub fn i32(&mut self) -> Result<i32, Error> {
        self.array().map(i32::from_le_bytes)
    }
    pub fn i64(&mut self) -> Result<i64, Error> {
        self.array().map(i64::from_le_bytes)
    }
    pub fn f64(&mut self) -> Result<f64, Error> {
        self.array().map(f64::from_le_bytes)
    }
    pub fn bool(&mut self) -> Result<bool, Error> {
        Ok(self.u8()? != 0)
    }
    pub fn pid(&mut self) -> Result<u64, Error> {
        self.u64()
    }
    pub fn string(&mut self) -> Result<String, Error> {
        let n = self.u16()? as usize;
        let b = self.take(n)?;
        let b = b.strip_suffix(&[0]).unwrap_or(b);
        Ok(String::from_utf8_lossy(b).into_owned())
    }
    pub fn buffer(&mut self) -> Result<&'a [u8], Error> {
        let n = self.u32()? as usize;
        self.take(n)
    }
    pub fn qbuffer(&mut self) -> Result<&'a [u8], Error> {
        let n = self.u16()? as usize;
        self.take(n)
    }
    pub fn datetime(&mut self) -> Result<DateTime, Error> {
        self.u64().map(DateTime)
    }
    pub fn station(&mut self) -> Result<StationUrl, Error> {
        Ok(StationUrl::parse(&self.string()?))
    }
    /// A List; a count larger than the bytes left is corrupt (every element takes a byte).
    pub fn list<T>(
        &mut self,
        mut f: impl FnMut(&mut Self) -> Result<T, Error>,
    ) -> Result<Vec<T>, Error> {
        let n = self.u32()? as usize;
        if n > self.remaining() {
            return Err(Error::Truncated);
        }
        (0..n).map(|_| f(self)).collect()
    }
    pub fn map<K, V>(
        &mut self,
        mut fk: impl FnMut(&mut Self) -> Result<K, Error>,
        mut fv: impl FnMut(&mut Self) -> Result<V, Error>,
    ) -> Result<Vec<(K, V)>, Error> {
        let n = self.u32()? as usize;
        if n > self.remaining() {
            return Err(Error::Truncated);
        }
        (0..n).map(|_| Ok((fk(self)?, fv(self)?))).collect()
    }
    pub fn variant(&mut self) -> Result<Variant, Error> {
        Ok(match self.u8()? {
            0 => Variant::None,
            1 => Variant::I64(self.i64()?),
            2 => Variant::F64(self.f64()?),
            3 => Variant::Bool(self.bool()?),
            4 => Variant::Str(self.string()?),
            5 => Variant::DateTime(self.datetime()?),
            6 => Variant::U64(self.u64()?),
            t => return Err(Error::Protocol(format!("unknown variant type {t}"))),
        })
    }
    /// A structure; a level longer than this client knows (a newer version) is fine.
    pub fn structure<S: Structure>(&mut self) -> Result<S, Error> {
        let mut s = S::default();
        for level in 0..S::VERSIONS.len() {
            if self.struct_header {
                let _version = self.u8()?;
                let mut sub = Reader::new(self.buffer()?);
                s.load(level, &mut sub)?;
            } else {
                s.load(level, self)?;
            }
        }
        Ok(s)
    }
    /// An any-data holder: its class name and the structure's bytes.
    pub fn any_data(&mut self) -> Result<(String, &'a [u8]), Error> {
        let name = self.string()?;
        let _len = self.u32()?;
        Ok((name, self.buffer()?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default, Debug, PartialEq)]
    struct Two {
        a: u32,
        b: String,
    }

    impl Structure for Two {
        const VERSIONS: &'static [u8] = &[0, 1];
        fn save(&self, level: usize, w: &mut Writer) {
            match level {
                0 => w.u32(self.a),
                _ => w.string(&self.b),
            };
        }
        fn load(&mut self, level: usize, r: &mut Reader) -> Result<(), Error> {
            match level {
                0 => self.a = r.u32()?,
                _ => self.b = r.string()?,
            }
            Ok(())
        }
    }

    #[test]
    fn strings_buffers_and_lists() {
        let mut w = Writer::new();
        w.string("ab")
            .buffer(&[1, 2])
            .qbuffer(&[3])
            .list(&[7u32, 8], |w, v| {
                w.u32(*v);
            });
        assert_eq!(
            w.bytes(),
            [
                3, 0, b'a', b'b', 0, 2, 0, 0, 0, 1, 2, 1, 0, 3, 2, 0, 0, 0, 7, 0, 0, 0, 8, 0, 0, 0
            ]
        );
        let mut r = Reader::new(w.bytes());
        assert_eq!(r.string().unwrap(), "ab");
        assert_eq!(r.buffer().unwrap(), [1, 2]);
        assert_eq!(r.qbuffer().unwrap(), [3]);
        assert_eq!(r.list(|r| r.u32()).unwrap(), [7, 8]);
        assert!(r.is_empty());
        assert_eq!(r.u8(), Err(Error::Truncated));
    }

    #[test]
    fn structure_levels_have_headers() {
        let s = Two {
            a: 5,
            b: "x".into(),
        };
        let mut w = Writer::new();
        w.structure(&s);
        assert_eq!(
            w.bytes(),
            [0, 4, 0, 0, 0, 5, 0, 0, 0, 1, 4, 0, 0, 0, 2, 0, b'x', 0]
        );
        assert_eq!(Reader::new(w.bytes()).structure::<Two>().unwrap(), s);
        let mut plain = Writer::new();
        plain.struct_header = false;
        plain.structure(&s);
        assert_eq!(plain.bytes(), [5, 0, 0, 0, 2, 0, b'x', 0]);
    }

    #[test]
    fn variants_and_datetime() {
        let vs = [
            Variant::None,
            Variant::I64(-3),
            Variant::Bool(true),
            Variant::Str("s".into()),
            Variant::U64(9),
            Variant::F64(0.5),
            Variant::DateTime(DateTime::new(2026, 10, 21, 1, 2, 3)),
        ];
        let mut w = Writer::new();
        for v in &vs {
            w.variant(v);
        }
        let mut r = Reader::new(w.bytes());
        for v in &vs {
            assert_eq!(&r.variant().unwrap(), v);
        }
        let d = DateTime::new(2026, 10, 21, 13, 14, 15);
        assert_eq!(d.year(), 2026);
        assert_eq!(d.0 >> 22 & 15, 10);
        assert!(DateTime::now().year() >= 2026);
    }

    #[test]
    fn any_data_holder_length_counts_the_buffer_prefix() {
        let mut w = Writer::new();
        w.any_data("Two", &Two::default());
        let mut r = Reader::new(w.bytes());
        let (name, body) = r.any_data().unwrap();
        assert_eq!(name, "Two");
        // two framed levels: 5 + 4 and 5 + 3 bytes
        assert_eq!(body.len(), 17);
        assert_eq!(&w.bytes()[6..10], &21u32.to_le_bytes());
    }
}
