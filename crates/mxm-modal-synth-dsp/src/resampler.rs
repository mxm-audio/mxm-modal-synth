//! The resampler from the engine's 32 kHz to the host's rate: a fixed polyphase windowed sinc,
//! MXM Thanks EG's with the engine's rate as the source (ops' `plans/plan-elements.md` §3).
//! Passband to 14.4 kHz (0.45 of the engine's rate), stopband at least 100 dB down from the lower
//! of the two Nyquists (designed for 110), a fixed latency in host samples, history cleared on
//! reset. At 32 kHz the instrument does not make one: nothing is resampled.
//!
//! **One table, made in `new`.** A Kaiser-windowed sinc, `2·half` taps at the input's rate, for
//! `phases` positions between two input samples. When the host's rate over 32 kHz reduces to at
//! most 1 024 (`L/M`: 441 at 44.1 kHz, 3 at 48, 3 at 96, 6 at 192), every output falls on one of `L`
//! phases exactly; any other rate takes 512 phases and interpolates between the two nearest,
//! which keeps the error near −110 dB at the passband's top. Positions are counted in whole input samples and
//! a numerator over the host's rate, so nothing drifts.
//!
//! **Causal by a fixed latency**: output `k` is the input's band-limited value at
//! `(k − latency) / rate` seconds, and `latency = ⌈half · rate / 32 000⌉` host samples is the least
//! for which its newest tap is never later than output `k`'s own time. The instrument reports it.

use std::f64::consts::PI;

const SOURCE_RATE: u64 = 32_000;
/// The design's stopband attenuation, with 10 dB of margin over the contract's 100.
const ATTENUATION_DB: f64 = 110.0;
/// 0.45 of the engine's rate: the module's own sound stops at its Nyquist, 16 kHz.
const PASSBAND_HZ: f64 = 14_400.0;
/// Exact phases up to this many; past it, interpolated ones.
const MAX_EXACT_PHASES: u64 = 1_024;
const INTERPOLATED_PHASES: u64 = 512;

#[derive(Clone, Debug)]
pub(crate) struct Resampler {
    rate: u64,
    half: usize,
    taps: usize,
    phases: u64,
    /// `phases + 1` rows of `taps`, each summing to one.
    table: Vec<f32>,
    /// The last `taps` inputs, written twice so the window is one slice.
    left: Vec<f32>,
    right: Vec<f32>,
    /// Where the newest input is, in `0..taps`.
    write: usize,
    /// Inputs pushed.
    pushed: i64,
    /// The next output's position in input samples: `whole + frac / rate`.
    whole: i64,
    frac: u64,
    latency: u32,
    /// Inputs pushed since the last one that was not zero.
    quiet: usize,
    scratch: Vec<f32>,
}

impl Resampler {
    /// For a host rate in Hz, rounded to a whole hertz; allocates its table and history.
    pub(crate) fn new(rate: u32) -> Self {
        let rate = u64::from(rate.clamp(8_000, 1_536_000));
        let source = SOURCE_RATE as f64;
        let stop = (rate as f64 / 2.0).min(source / 2.0);
        let pass = PASSBAND_HZ.min(stop - 1_000.0);
        // In cycles per input sample.
        let cutoff = 0.5 * (pass + stop) / source;
        let width = (stop - pass) / source;
        let beta = 0.1102 * (ATTENUATION_DB - 8.7);
        let length = (ATTENUATION_DB - 7.95) / (2.285 * 2.0 * PI * width) + 1.0;
        let half = (length / 2.0).ceil() as usize;
        let taps = 2 * half;
        let reduced = rate / gcd(rate, SOURCE_RATE);
        let phases = if reduced <= MAX_EXACT_PHASES {
            reduced
        } else {
            INTERPOLATED_PHASES
        };

        let i0_beta = bessel_i0(beta);
        let mut table = vec![0.0f32; (phases as usize + 1) * taps];
        let mut row = vec![0.0f64; taps];
        for (p, out) in table.chunks_exact_mut(taps).enumerate() {
            let phase = p as f64 / phases as f64;
            for (m, value) in row.iter_mut().enumerate() {
                let t = phase + (half as f64 - 1.0 - m as f64);
                *value = kernel(t, cutoff, half as f64, beta, i0_beta);
            }
            let sum: f64 = row.iter().sum();
            for (o, v) in out.iter_mut().zip(&row) {
                *o = (v / sum) as f32;
            }
        }

        let latency = ((half as u64 * rate).div_ceil(SOURCE_RATE)) as u32;
        let mut resampler = Self {
            rate,
            half,
            taps,
            phases,
            table,
            left: vec![0.0; 2 * taps],
            right: vec![0.0; 2 * taps],
            write: 0,
            pushed: 0,
            whole: 0,
            frac: 0,
            latency,
            quiet: 0,
            scratch: vec![0.0; taps],
        };
        resampler.reset();
        resampler
    }

    /// The fixed latency, in host samples.
    pub(crate) fn latency(&self) -> u32 {
        self.latency
    }

    /// History cleared, the position back to the start; allocates nothing.
    pub(crate) fn reset(&mut self) {
        self.left.fill(0.0);
        self.right.fill(0.0);
        self.write = self.taps - 1;
        self.pushed = 0;
        // −latency · 32 000 / rate, as a whole part and a numerator.
        let behind = u64::from(self.latency) * SOURCE_RATE;
        let (q, r) = (behind / self.rate, behind % self.rate);
        if r == 0 {
            self.whole = -(q as i64);
            self.frac = 0;
        } else {
            self.whole = -(q as i64) - 1;
            self.frac = self.rate - r;
        }
        self.quiet = self.taps;
    }

    /// Whether the next output needs another input first.
    #[inline]
    pub(crate) fn needs_input(&self) -> bool {
        self.pushed <= self.whole + self.half as i64
    }

    #[inline]
    pub(crate) fn push(&mut self, left: f32, right: f32) {
        self.write += 1;
        if self.write == self.taps {
            self.write = 0;
        }
        self.left[self.write] = left;
        self.left[self.write + self.taps] = left;
        self.right[self.write] = right;
        self.right[self.write + self.taps] = right;
        self.pushed += 1;
        if left != 0.0 || right != 0.0 {
            self.quiet = 0;
        } else {
            self.quiet = self.quiet.saturating_add(1);
        }
    }

    /// Whether anything but zero is still in the history: the output's tail.
    #[inline]
    pub(crate) fn ringing(&self) -> bool {
        self.quiet < self.taps
    }

    /// The next output, once [`Self::needs_input`] is false.
    #[inline]
    pub(crate) fn output(&mut self) -> (f32, f32) {
        let taps = self.taps;
        let start = self.write + 1;
        let window_left = &self.left[start..start + taps];
        let window_right = &self.right[start..start + taps];
        let position = self.frac * self.phases;
        let p0 = (position / self.rate) as usize;
        let rem = position % self.rate;
        let row0 = &self.table[p0 * taps..(p0 + 1) * taps];
        let out = if rem == 0 {
            (dot(row0, window_left), dot(row0, window_right))
        } else {
            let w = rem as f32 / self.rate as f32;
            let row1 = &self.table[(p0 + 1) * taps..(p0 + 2) * taps];
            for ((c, a), b) in self.scratch.iter_mut().zip(row0).zip(row1) {
                *c = a + (b - a) * w;
            }
            (
                dot(&self.scratch, window_left),
                dot(&self.scratch, window_right),
            )
        };
        self.frac += SOURCE_RATE;
        while self.frac >= self.rate {
            self.frac -= self.rate;
            self.whole += 1;
        }
        out
    }
}

#[inline]
fn dot(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

/// The windowed sinc at `t` input samples from its centre.
fn kernel(t: f64, cutoff: f64, half: f64, beta: f64, i0_beta: f64) -> f64 {
    let x = t / half;
    if x.abs() > 1.0 {
        return 0.0;
    }
    let window = bessel_i0(beta * (1.0 - x * x).max(0.0).sqrt()) / i0_beta;
    let arg = 2.0 * cutoff * t;
    let sinc = if arg.abs() < 1e-12 {
        1.0
    } else {
        (PI * arg).sin() / (PI * arg)
    };
    2.0 * cutoff * sinc * window
}

/// The modified Bessel function of the first kind, order zero, by its series.
fn bessel_i0(x: f64) -> f64 {
    let half = x / 2.0;
    let mut term = 1.0;
    let mut sum = 1.0;
    let mut k = 1.0;
    while term > sum * 1e-17 {
        term *= (half / k) * (half / k);
        sum += term;
        k += 1.0;
    }
    sum
}

fn gcd(mut a: u64, mut b: u64) -> u64 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The resampler's output for `input`, `count` host samples.
    fn run(rate: u32, input: impl Fn(usize) -> f32, count: usize) -> Vec<f32> {
        let mut r = Resampler::new(rate);
        let mut n = 0;
        let mut out = Vec::with_capacity(count);
        for _ in 0..count {
            while r.needs_input() {
                let x = input(n);
                r.push(x, -x);
                n += 1;
            }
            let (left, right) = r.output();
            assert_eq!(left, -right, "the channels are one filter");
            out.push(left);
        }
        out
    }

    /// The least-squares sinusoid at `hz` and the residual's RMS around it.
    fn fit(x: &[f32], hz: f64, rate: f64) -> (f64, f64) {
        let (mut cc, mut ss, mut cs, mut xc, mut xs) = (0.0, 0.0, 0.0, 0.0, 0.0);
        for (k, &v) in x.iter().enumerate() {
            let w = 2.0 * PI * hz * k as f64 / rate;
            let (c, s) = (w.cos(), w.sin());
            cc += c * c;
            ss += s * s;
            cs += c * s;
            xc += f64::from(v) * c;
            xs += f64::from(v) * s;
        }
        let det = cc * ss - cs * cs;
        let a = (xc * ss - xs * cs) / det;
        let b = (xs * cc - xc * cs) / det;
        let mut residual = 0.0;
        for (k, &v) in x.iter().enumerate() {
            let w = 2.0 * PI * hz * k as f64 / rate;
            let e = f64::from(v) - a * w.cos() - b * w.sin();
            residual += e * e;
        }
        (a.hypot(b), (residual / x.len() as f64).sqrt())
    }

    const RATES: [u32; 5] = [44_100, 48_000, 96_000, 192_000, 31_999];

    /// The passband: tones to 14.4 kHz come out at their level, within ±0.001 dB, and at the
    /// time the latency says, within −90 dB of the tone; and the stopband, for a rate above 32 kHz
    /// where it holds the images: whatever is not the tone is at least 100 dB down.
    #[test]
    fn the_passband_is_flat_and_the_images_are_100_db_down() {
        for rate in RATES {
            let r = Resampler::new(rate);
            let latency = r.latency() as usize;
            // Past the latency and the start's transient: the window's taps, in host samples.
            let skip = latency + r.taps * 2 * (rate as usize).div_ceil(32_000);
            let n = 8192;
            for hz in [100.0, 1_000.0, 7_000.0, 12_000.0, 14_400.0] {
                let amplitude = 0.5;
                let out = run(
                    rate,
                    |i| (amplitude * (2.0 * PI * hz * i as f64 / 32_000.0).sin()) as f32,
                    skip + n,
                );
                let window = &out[skip..];
                let (level, residual) = fit(window, hz, f64::from(rate));
                let gain_db = 20.0 * (level / amplitude).log10();
                assert!(gain_db.abs() < 0.001, "{rate} Hz, {hz} Hz: {gain_db} dB");
                let alias_db = 20.0 * (residual / (amplitude / 2f64.sqrt())).log10();
                assert!(alias_db < -100.0, "{rate} Hz, {hz} Hz: {alias_db} dB");
                // On time: against the input delayed by exactly the latency.
                let mut err = 0.0f64;
                for (j, &v) in window.iter().enumerate() {
                    let k = (skip + j) as f64 - latency as f64;
                    let want = amplitude * (2.0 * PI * hz * k / f64::from(rate)).sin();
                    err += (f64::from(v) - want).powi(2);
                }
                let late_db = 20.0 * ((err / n as f64).sqrt() / (amplitude / 2f64.sqrt())).log10();
                assert!(late_db < -90.0, "{rate} Hz, {hz} Hz: {late_db} dB off");
            }
        }
    }

    /// The stopband below 32 kHz: what lies above the host's Nyquist does not alias back, at
    /// least 100 dB down.
    #[test]
    fn above_the_host_nyquist_is_100_db_down() {
        for rate in [22_050, 31_999] {
            let skip = 600;
            let n = 8192;
            for hz in [11_100.0, 13_000.0, 15_900.0] {
                if hz < f64::from(rate) / 2.0 + 50.0 {
                    continue;
                }
                let out = run(
                    rate,
                    |i| (0.5 * (2.0 * PI * hz * i as f64 / 32_000.0).sin()) as f32,
                    skip + n,
                );
                let rms = (out[skip..]
                    .iter()
                    .map(|&v| f64::from(v).powi(2))
                    .sum::<f64>()
                    / n as f64)
                    .sqrt();
                let db = 20.0 * (rms / (0.5 / 2f64.sqrt())).log10();
                assert!(db < -100.0, "{rate} Hz, {hz} Hz: {db} dB");
            }
        }
    }

    /// The latency is fixed, in host samples: an impulse comes out at it, its response symmetric
    /// about it.
    #[test]
    fn an_impulse_comes_out_at_the_latency() {
        for (rate, want) in [(44_100, 100), (48_000, 108), (96_000, 216), (192_000, 432)] {
            let r = Resampler::new(rate);
            assert_eq!(r.latency(), want, "{rate} Hz");
            let out = run(rate, |i| if i == 0 { 1.0 } else { 0.0 }, 800);
            let peak = out
                .iter()
                .enumerate()
                .max_by(|a, b| a.1.abs().total_cmp(&b.1.abs()))
                .unwrap()
                .0;
            assert_eq!(peak, want as usize, "{rate} Hz");
            for d in 1..20 {
                assert!(
                    (out[peak - d] - out[peak + d]).abs() < 1e-6,
                    "{rate} Hz: not symmetric at {d}"
                );
            }
        }
    }

    /// Silence in is exactly zero out once the history has passed, and a reset clears it.
    #[test]
    fn silence_is_exact_zero_and_reset_clears() {
        let mut r = Resampler::new(44_100);
        for _ in 0..50 {
            while r.needs_input() {
                r.push(0.7, 0.2);
            }
            r.output();
        }
        assert!(r.ringing());
        r.reset();
        assert!(!r.ringing());
        for _ in 0..500 {
            while r.needs_input() {
                r.push(0.0, 0.0);
            }
            assert_eq!(r.output(), (0.0, 0.0));
        }
    }

    /// Over a second, the outputs take exactly a second of inputs: nothing drifts.
    #[test]
    fn a_second_in_is_a_second_out() {
        for rate in RATES {
            let mut r = Resampler::new(rate);
            for _ in 0..rate {
                while r.needs_input() {
                    r.push(0.0, 0.0);
                }
                r.output();
            }
            // Output `rate` reads the input a latency late: 32 000 − ⌈latency · 32 000 / rate⌉,
            // exactly.
            let behind = (u64::from(r.latency()) * 32_000).div_ceil(u64::from(rate));
            assert_eq!(r.whole, 32_000 - behind as i64, "{rate} Hz");
        }
    }
}
