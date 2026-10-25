//! The `OtoAsobi` bus (research/specs/sound-effect-parts.md §2b, private): the game sends its
//! music and effects (the BGM and SE buses) through it on their way to the final mix
//! (`Sound/Bus/BusSetting.byml`'s bus graph), and the placed sound effects Twisty Turny and
//! Woozy insert effects on it while they run [obs v3.0.3, the audio manager update
//! 0x71017b15d0]:
//! - Twisty Turny: `Otoasobi_Delay` (the system's delay) then `OtoAsobi_Filter`, a resonant
//!   band-pass whose centre the game sweeps every frame ([`band_pass`], 0x71004fe0a0), with the
//!   bus raised to 2.5 (0x71004d41f0);
//! - Woozy: `OtoAsobi_Vib`, a vibrato (a delay line swept by a sine, 0x7100521700), then
//!   `OtoAsobi_Mod`, a tremolo (0x7100520780).
//!
//! The theme's echo and reverb return beside the bus, unfiltered. Everything is f32 in a fixed
//! order with the sine from [`SINE`] (built from basic operations only), so a run renders the
//! same on every host. The delay is the console's own effect, which the game only configures:
//! a feedback delay line with dry and input gains and a cross-feed [inf: its exact network].

use std::sync::OnceLock;

/// The bus's effect settings, from the course's sound data (`Sound/Bus/BusSetting.byml`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OtoSettings {
    /// `Otoasobi_Delay`: delay (ms), input, feedback and dry gains, channel spread, low-pass.
    pub delay_ms: f32,
    pub in_gain: f32,
    pub feedback: f32,
    pub dry: f32,
    pub spread: f32,
    pub lpf: f32,
    /// `OtoAsobi_Vib`'s longest depth (s): the delay line's length.
    pub vib_depth_max: f32,
}

/// What the bus does now (set every game frame).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct OtoParams {
    /// Twisty Turny: the band-pass centre (Hz) and Q, with the delay before it and the bus at
    /// `gain`.
    pub filter: Option<(f32, f32)>,
    pub gain: f32,
    /// Woozy: the vibrato's depth (s) and rate (Hz), the tremolo's depth (0–1) and rate (Hz).
    pub woozy: Option<[f32; 4]>,
    /// Woozy's effects were put on the bus before Twisty Turny's (they run in that order).
    pub woozy_first: bool,
}

/// 256 steps of a sine and their slopes, as the game's tremolo and vibrato read one (value +
/// slope · fraction; the table at 0x71023385b8 holds the same values).
pub static SINE: OnceLock<[(f32, f32); 256]> = OnceLock::new();

/// sin(x) and cos(x) for |x| ≤ π/2 from their Taylor series in f64 (only + − × ÷, so every
/// host gets the same bits).
fn taylor(x: f64) -> (f64, f64) {
    let (mut ts, mut s) = (x, x);
    let (mut tc, mut c) = (1.0, 1.0);
    for n in 1..14 {
        let n = n as f64;
        ts = -ts * x * x / ((2.0 * n) * (2.0 * n + 1.0));
        tc = -tc * x * x / ((2.0 * n - 1.0) * (2.0 * n));
        s += ts;
        c += tc;
    }
    (s, c)
}

/// sin(2π·k/256), by quadrant.
fn sin_step(k: usize) -> f64 {
    let k = k % 256;
    let a = (k % 64) as f64 * std::f64::consts::FRAC_PI_2 / 64.0;
    let (s, c) = taylor(a);
    match k / 64 {
        0 => s,
        1 => c,
        2 => -s,
        _ => -c,
    }
}

fn sine() -> &'static [(f32, f32); 256] {
    SINE.get_or_init(|| {
        let v: Vec<f64> = (0..257).map(sin_step).collect();
        std::array::from_fn(|i| (v[i] as f32, (v[i + 1] - v[i]) as f32))
    })
}

/// The sine of a 32-bit phase (a full turn is 2³²): the table step plus its slope times the
/// low 24 bits.
#[inline]
fn sin_phase(phase: u32) -> f32 {
    let (v, d) = sine()[(phase >> 24) as usize];
    v + d * (phase & 0xff_ffff) as f32 * (1.0 / 16_777_216.0)
}

/// tan(x) for 0 ≤ x < π/2.
fn tan_det(x: f64) -> f64 {
    let (s, c) = taylor(x);
    s / c
}

/// The resonant band-pass's coefficients for `centre` Hz and `q` at `rate`, quantized to 1/16384
/// as the game hands them to the console (0x71004fe0a0) [obs v3.0.3]: K = tan(π·f/fs),
/// n = 1 / (K² + K/Q + 1); b0 = K/Q·n, b1 = 0, b2 = −b0, a1 = 2(1 − K²)·n, a2 = (K/Q − K² − 1)·n
/// (the feedback ones already negated). Out of range (f ≥ fs/2, Q or f ≤ 0) it passes the sound
/// through (b0 = 1).
pub fn band_pass(centre: f32, q: f32, rate: u32) -> [f32; 5] {
    let fs = rate as f32;
    if !(q > 0.0 && fs > 0.0 && centre > 0.0 && centre < fs * 0.5) {
        return [1.0, 0.0, 0.0, 0.0, 0.0];
    }
    let k = tan_det((centre * std::f32::consts::PI / fs) as f64) as f32;
    let k2 = k * k;
    let kq = (1.0 / q) * k;
    let n = 1.0 / (k2 + kq + 1.0);
    let b = kq * n;
    let q14 = |v: f32| (v * 16384.0) as i32 as i16 as f32 / 16384.0;
    [
        q14(b),
        0.0,
        ((b * -16384.0) as i32 as i16) as f32 / 16384.0,
        q14((k2 - 1.0) * -2.0 * n),
        q14(b - (k2 + 1.0) * n),
    ]
}

/// The bus's effects and their state.
pub struct OtoBus {
    rate: u32,
    settings: Option<OtoSettings>,
    params: OtoParams,
    /// The delay: an interleaved line and its write frame; the low-pass's state.
    line: Vec<f32>,
    line_w: usize,
    lp: [f32; 2],
    /// The band-pass's coefficients and its inputs / outputs (per channel: x1, x2, y1, y2).
    coef: [f32; 5],
    bq: [[f32; 4]; 2],
    /// The vibrato's line (interleaved), its write frame and phase.
    vib: Vec<f32>,
    vib_w: usize,
    vib_phase: u32,
    /// The tremolo's phase, its gain and the frames left on it (it changes every 8).
    trem_phase: u32,
    trem_gain: f32,
    trem_left: u32,
    filter_on: bool,
    woozy_on: bool,
}

/// The longest delay the line holds (s).
const LINE_SECONDS: f32 = 1.0;
/// The longest vibrato depth the line holds (s) and the block it reads ahead of.
const VIB_SECONDS: f32 = 0.1;
const VIB_BLOCK: usize = 512;

impl OtoBus {
    pub fn new(rate: u32) -> Self {
        let line = (LINE_SECONDS * rate as f32) as usize + 1;
        let vib = (2.0 * VIB_SECONDS * rate as f32) as usize + VIB_BLOCK + 2;
        Self {
            rate,
            settings: None,
            params: OtoParams::default(),
            line: vec![0.0; line * 2],
            line_w: 0,
            lp: [0.0; 2],
            coef: [1.0, 0.0, 0.0, 0.0, 0.0],
            bq: [[0.0; 4]; 2],
            vib: vec![0.0; vib * 2],
            vib_w: 0,
            vib_phase: 0,
            trem_phase: 0,
            trem_gain: 1.0,
            trem_left: 0,
            filter_on: false,
            woozy_on: false,
        }
    }

    pub fn active(&self) -> bool {
        self.filter_on || self.woozy_on
    }

    /// New settings and this frame's parameters. An effect put on starts from silence; one
    /// taken off forgets its state (the game removes it from the bus).
    pub fn set(&mut self, settings: OtoSettings, p: OtoParams) {
        self.settings = Some(settings);
        let filter = p.filter.is_some();
        if filter && !self.filter_on {
            self.line.fill(0.0);
            self.lp = [0.0; 2];
            self.bq = [[0.0; 4]; 2];
        }
        let woozy = p.woozy.is_some();
        if woozy && !self.woozy_on {
            self.vib.fill(0.0);
            self.vib_phase = 0;
            self.trem_phase = 0;
            self.trem_left = 0;
            self.trem_gain = 1.0;
        }
        if let Some((centre, q)) = p.filter {
            self.coef = band_pass(centre, q, self.rate);
        }
        self.filter_on = filter;
        self.woozy_on = woozy;
        self.params = p;
    }

    /// Run interleaved stereo `buf` (the music and effects of a block) through the bus.
    pub fn process(&mut self, buf: &mut [f32]) {
        let Some(s) = self.settings else { return };
        if !self.active() {
            return;
        }
        if self.params.woozy_first {
            self.woozy(buf, &s);
            self.filter(buf, &s);
        } else {
            self.filter(buf, &s);
            self.woozy(buf, &s);
        }
    }

    fn filter(&mut self, buf: &mut [f32], s: &OtoSettings) {
        if !self.filter_on {
            return;
        }
        // the delay
        let len = self.line.len() / 2;
        let n = ((s.delay_ms * 0.001 * self.rate as f32) as usize).clamp(1, len - 1);
        let base = (1.0 - s.spread) * s.feedback;
        let cross = s.spread * s.feedback;
        let lp_a = 1.0 - 0.95 * s.lpf.clamp(0.0, 1.0);
        for f in buf.as_chunks_mut::<2>().0.iter_mut() {
            let r = (self.line_w + len - n) % len;
            let (dl, dr) = (self.line[2 * r], self.line[2 * r + 1]);
            let il = s.in_gain * f[0] + base * dl + cross * dr;
            let ir = s.in_gain * f[1] + base * dr + cross * dl;
            self.lp[0] += lp_a * (il - self.lp[0]);
            self.lp[1] += lp_a * (ir - self.lp[1]);
            self.line[2 * self.line_w] = self.lp[0];
            self.line[2 * self.line_w + 1] = self.lp[1];
            self.line_w = (self.line_w + 1) % len;
            f[0] = s.dry * f[0] + dl;
            f[1] = s.dry * f[1] + dr;
        }
        // the band-pass, then the bus gain
        let [b0, b1, b2, a1, a2] = self.coef;
        let g = self.params.gain;
        for f in buf.as_chunks_mut::<2>().0.iter_mut() {
            for (c, x) in f.iter_mut().enumerate() {
                let st = &mut self.bq[c];
                let y = b0 * *x + b1 * st[0] + b2 * st[1] + a1 * st[2] + a2 * st[3];
                (st[1], st[0]) = (st[0], *x);
                (st[3], st[2]) = (st[2], y);
                *x = y * g;
            }
        }
    }

    fn woozy(&mut self, buf: &mut [f32], s: &OtoSettings) {
        let Some([depth, freq, t_depth, t_speed]) = self.params.woozy else {
            return;
        };
        let rate = self.rate as f32;
        // the vibrato: write the block, then read each frame `depth · rate · (1 + sin)` frames
        // back with the game's linear blend (its weight is the delay's own fraction,
        // 0x7100521700)
        let depth = depth.clamp(0.0, s.vib_depth_max.min(VIB_SECONDS));
        let len = self.vib.len() / 2;
        let inc = if freq > 0.0 {
            (4_294_967_296.0f32 / (rate / freq)) as i64 as u32
        } else {
            0
        };
        for chunk in buf.chunks_mut(VIB_BLOCK * 2) {
            let frames = chunk.len() / 2;
            let start = self.vib_w;
            for (i, f) in chunk.as_chunks::<2>().0.iter().enumerate() {
                let w = (start + i) % len;
                self.vib[2 * w] = f[0];
                self.vib[2 * w + 1] = f[1];
            }
            let mut phase = self.vib_phase;
            for i in 0..frames {
                phase = phase.wrapping_add(inc);
                let d = depth * rate * (sin_phase(phase) + 1.0);
                let back = (i as f32 - d).floor();
                let frac = d - d.floor();
                let k0 = (start as i64 + back as i64).rem_euclid(len as i64) as usize;
                let k1 = (k0 + 1) % len;
                for c in 0..2 {
                    chunk[2 * i + c] =
                        (1.0 - frac) * self.vib[2 * k0 + c] + frac * self.vib[2 * k1 + c];
                }
            }
            self.vib_phase = phase;
            self.vib_w = (start + frames) % len;
        }
        // the tremolo: a gain 1 − depth·(1 + sin)/2 in steps of 1/16384, changed every 8
        // frames
        let inc8 = ((t_speed * 4_294_967_296.0 / rate) * 8.0) as i64 as u32;
        let t_depth = t_depth.clamp(0.0, 1.0);
        for f in buf.as_chunks_mut::<2>().0.iter_mut() {
            if self.trem_left == 0 {
                self.trem_phase = self.trem_phase.wrapping_add(inc8);
                let g = (t_depth * (sin_phase(self.trem_phase) + 1.0) * -0.5 + 1.0) * 16384.0;
                self.trem_gain = (g as i32) as f32 / 16384.0;
                self.trem_left = 8;
            }
            self.trem_left -= 1;
            f[0] *= self.trem_gain;
            f[1] *= self.trem_gain;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> OtoSettings {
        OtoSettings {
            delay_ms: 150.0,
            in_gain: 0.7,
            feedback: 0.7,
            dry: 0.7,
            spread: 1.0,
            lpf: 0.0,
            vib_depth_max: 0.02,
        }
    }

    #[test]
    fn the_sine_table_is_a_sine() {
        for k in [0usize, 13, 32, 64, 100, 128, 192, 255] {
            let want = (k as f64 * std::f64::consts::TAU / 256.0).sin();
            assert!((sine()[k].0 as f64 - want).abs() < 1e-6, "{k}");
        }
        assert_eq!(sin_phase(0), 0.0);
        assert!((sin_phase(1 << 30) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn the_band_pass_matches_the_game_s_formula() {
        // 1 kHz, Q 1.8 at 48 kHz
        let c = band_pass(1000.0, 1.8, 48_000);
        let k = (std::f64::consts::PI * 1000.0 / 48_000.0).tan();
        let n = 1.0 / (k * k + k / 1.8 + 1.0);
        let q = |v: f64| (v * 16384.0) as i32 as f64 / 16384.0;
        assert_eq!(c[0] as f64, q(k / 1.8 * n));
        assert_eq!(c[1], 0.0);
        assert_eq!(c[2], -c[0]);
        assert!((c[3] as f64 - q(2.0 * (1.0 - k * k) * n)).abs() < 1e-4);
        // out of range: through
        assert_eq!(band_pass(30_000.0, 1.8, 48_000), [1.0, 0.0, 0.0, 0.0, 0.0]);
    }

    /// RMS of a sine of `hz` through the bus at a fixed centre.
    fn through(hz: f32, centre: f32) -> f32 {
        let mut b = OtoBus::new(48_000);
        b.set(
            OtoSettings {
                in_gain: 0.0,
                dry: 1.0,
                ..settings()
            },
            OtoParams {
                filter: Some((centre, 1.8)),
                gain: 1.0,
                ..Default::default()
            },
        );
        let mut buf: Vec<f32> = (0..48_000)
            .flat_map(|i| {
                let v = (i as f32 * hz * std::f32::consts::TAU / 48_000.0).sin() * 0.5;
                [v, v]
            })
            .collect();
        b.process(&mut buf);
        let tail = &buf[48_000..];
        (tail.iter().map(|v| v * v).sum::<f32>() / tail.len() as f32).sqrt()
    }

    #[test]
    fn the_filter_passes_its_centre_and_cuts_the_rest() {
        let at = through(1000.0, 1000.0);
        assert!((at - 0.5 / 2f32.sqrt()).abs() < 0.02, "{at}");
        assert!(through(100.0, 1000.0) < at * 0.2);
        assert!(through(8000.0, 1000.0) < at * 0.2);
    }

    #[test]
    fn the_delay_repeats_after_150_ms_crossed_over() {
        let mut b = OtoBus::new(48_000);
        b.set(
            settings(),
            OtoParams {
                filter: Some((20_000.0, 1.8)),
                gain: 1.0,
                ..Default::default()
            },
        );
        // an impulse on the left; the band-pass out of range passes it
        b.coef = [1.0, 0.0, 0.0, 0.0, 0.0];
        let mut buf = vec![0.0f32; 48_000 * 2];
        buf[0] = 1.0;
        b.process(&mut buf);
        let at = |f: usize, c: usize| buf[2 * f + c];
        assert_eq!(at(0, 0), 0.7, "dry");
        assert_eq!(at(7200, 0), 0.7, "the echo, 150 ms on");
        assert_eq!(at(7200, 1), 0.0);
        assert!(
            (at(14_400, 1) - 0.49).abs() < 1e-6,
            "fed back across: {}",
            at(14_400, 1)
        );
    }

    #[test]
    fn woozy_bends_the_pitch_and_pulses_the_level() {
        let mut b = OtoBus::new(48_000);
        b.set(
            settings(),
            OtoParams {
                woozy: Some([0.02, 0.5, 0.35, 800.0]),
                gain: 1.0,
                ..Default::default()
            },
        );
        let mut buf = vec![0.5f32; 4800 * 2];
        b.process(&mut buf);
        // a constant through the vibrato stays constant once the line is full; the tremolo
        // swings it between 1 and 0.65 every 8 frames
        let tail = &buf[4000 * 2..];
        let (lo, hi) = tail
            .iter()
            .fold((f32::MAX, f32::MIN), |(l, h), &v| (l.min(v), h.max(v)));
        assert!(hi <= 0.5 + 1e-6 && lo >= 0.5 * 0.649, "{lo} {hi}");
        assert!(hi - lo > 0.1, "{lo} {hi}");
        // the same input renders the same output
        let mut c = OtoBus::new(48_000);
        c.set(settings(), b.params);
        let mut again = vec![0.5f32; 4800 * 2];
        c.process(&mut again);
        assert_eq!(buf, again);
    }
}
