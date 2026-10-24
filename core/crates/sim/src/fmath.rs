//! Deterministic float math. The sim must compute the same bits on every platform: online peers
//! on macOS, Windows, Linux and the web step the same world, and a Linux server re-simulates
//! replays recorded anywhere. `f32::sin` and friends call the platform's math library, whose last
//! bits differ between systems; these go through `libm` (a pure-Rust port of musl's), which
//! gives the same result everywhere. Basic arithmetic and `sqrt` are exact IEEE operations and
//! need nothing. `clippy.toml` in this crate forbids the platform versions.

pub trait DetMath: Sized {
    fn dsin(self) -> Self;
    fn dcos(self) -> Self;
    fn dsin_cos(self) -> (Self, Self);
    fn dtan(self) -> Self;
    fn dasin(self) -> Self;
    fn datan2(self, x: Self) -> Self;
    fn dhypot(self, other: Self) -> Self;
}

impl DetMath for f32 {
    fn dsin(self) -> f32 {
        libm::sinf(self)
    }
    fn dcos(self) -> f32 {
        libm::cosf(self)
    }
    fn dsin_cos(self) -> (f32, f32) {
        (libm::sinf(self), libm::cosf(self))
    }
    fn dtan(self) -> f32 {
        libm::tanf(self)
    }
    fn dasin(self) -> f32 {
        libm::asinf(self)
    }
    fn datan2(self, x: f32) -> f32 {
        libm::atan2f(self, x)
    }
    fn dhypot(self, other: f32) -> f32 {
        libm::hypotf(self, other)
    }
}

impl DetMath for f64 {
    fn dsin(self) -> f64 {
        libm::sin(self)
    }
    fn dcos(self) -> f64 {
        libm::cos(self)
    }
    fn dsin_cos(self) -> (f64, f64) {
        (libm::sin(self), libm::cos(self))
    }
    fn dtan(self) -> f64 {
        libm::tan(self)
    }
    fn dasin(self) -> f64 {
        libm::asin(self)
    }
    fn datan2(self, x: f64) -> f64 {
        libm::atan2(self, x)
    }
    fn dhypot(self, other: f64) -> f64 {
        libm::hypot(self, other)
    }
}

/// The game's 256-entry sine table: sin, its step to the next entry, cos, its step
/// [obs 0x7101d34bf8; v3.0.3 0x71023385b8: the same values as sin/cos of 2π·i/256 rounded to
/// f32, exact 0 on the axes].
pub fn table_wave(i: u32) -> (f32, f32, f32, f32) {
    let at = |k: u32| (std::f64::consts::TAU * k as f64 / 256.0).dsin_cos();
    let (s0, c0) = at(i);
    let (s1, c1) = at(i + 1);
    let axis = |v: f64, zero: bool| if zero { 0.0 } else { v as f32 };
    (
        axis(s0, i.is_multiple_of(128)),
        (s1 - s0) as f32,
        axis(c0, i % 128 == 64),
        (c1 - c0) as f32,
    )
}

/// sin and cos of a 32-bit binary angle (0x4000_0000 = 90°) the game's way: the top byte picks
/// the table entry, the low 24 bits interpolate (× 2⁻²⁴).
pub fn table_sin_cos(angle: u32) -> (f32, f32) {
    let (s, ds, c, dc) = table_wave(angle >> 24);
    let f = (angle & 0xff_ffff) as f32 * f32::from_bits(0x3380_0000);
    (s + ds * f, c + f * dc)
}

/// The game's arctangent table entry i (0 ≤ i ≤ 129): atan(i / 128) as a binary angle,
/// rounded down [obs v3.0.3 table 0x71023395c8: all 129 stored entries are
/// ⌊atan(i/128)·2³¹/π⌋, each with its step to the next as f32].
fn atan_entry(i: u32) -> i32 {
    libm::floor(libm::atan(i as f64 / 128.0) * 2_147_483_648.0 / std::f64::consts::PI) as i32
}

/// atan(t) for 0 ≤ t ≤ 1 as a binary angle, the game's way: table entry ⌊128·t⌋ plus its
/// step times the fraction, truncated [obs v3.0.3 0x710027eed0].
pub(crate) fn table_atan(t: f32) -> i32 {
    let i = (t * 128.0) as i32;
    let (a, b) = (atan_entry(i as u32), atan_entry(i as u32 + 1));
    a.wrapping_add(((t * 128.0 - i as f32) * (b - a) as f32) as i32)
}

/// The angle of (x, y) as a 32-bit binary angle (0 along +x, 0x4000_0000 along +y), from the
/// arctangent table by octant; (0, 0) gives 0 [obs v3.0.3 0x7100d5db50].
pub fn table_atan2(y: f32, x: f32) -> u32 {
    if y == 0.0 && x == 0.0 {
        return 0;
    }
    let a = if x >= 0.0 {
        if y < 0.0 {
            let y = -y;
            if x < y {
                table_atan(x / y).wrapping_add(0xc000_0000u32 as i32)
            } else {
                table_atan(y / x).wrapping_neg()
            }
        } else if y <= x {
            table_atan(y / x)
        } else {
            0x4000_0000 - table_atan(x / y)
        }
    } else if y < 0.0 {
        if x <= y {
            table_atan(y / x) ^ i32::MIN
        } else {
            (-0x4000_0000i32).wrapping_sub(table_atan(x / y))
        }
    } else {
        let x = -x;
        if x < y {
            table_atan(x / y).wrapping_add(0x4000_0000)
        } else {
            i32::MIN.wrapping_sub(table_atan(y / x))
        }
    };
    a as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_arctangent_table_is_the_games() {
        // the 129 entries the game stores, summed, and a few of them
        let sum: i64 = (0..=128).map(|i| atan_entry(i) as i64).sum();
        assert_eq!(sum, 38_663_763_629);
        assert_eq!(atan_entry(3), 16_018_128);
        assert_eq!(atan_entry(128), 0x2000_0000);
        assert_eq!(atan_entry(129) - atan_entry(128), 2_659_773);
        assert_eq!(table_atan2(0.0, 1.0), 0);
        assert_eq!(table_atan2(1.0, 0.0), 0x4000_0000);
        assert_eq!(table_atan2(-1.0, 0.0), 0xc000_0000);
        assert_eq!(table_atan2(0.0, -1.0), 0x8000_0000);
        assert_eq!(table_atan2(1.0, 1.0), 0x2000_0000);
        assert_eq!(table_atan2(-1.0, -1.0), 0xa000_0000);
    }

    #[test]
    fn close_to_the_platform_and_fixed_bits() {
        for i in -50..50 {
            let a = i as f32 * 0.137;
            assert!((a.dsin() - libm::sinf(a)).abs() == 0.0);
            #[allow(clippy::disallowed_methods)]
            let platform = a.sin();
            assert!((a.dsin() - platform).abs() < 1e-6);
        }
        // pinned values: these bits must never change between platforms or releases
        assert_eq!(1.0f32.dsin().to_bits(), 0x3f57_6aa4);
        assert_eq!(0.5f32.datan2(-0.25).to_bits(), 0x4002_3454);
    }
}
