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

#[cfg(test)]
mod tests {
    use super::*;

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
