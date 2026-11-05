//! The three LFOs, run once per engine block (2 kHz). Output −1 to 1.
//!
//! With Retrigger on, each voice has its own, restarting at phase 0 with each note; with it off,
//! the instrument has one, free-running, for every voice. Either way the depth fades in after each
//! note ([`Fade`]).

use crate::LfoShape;

/// The fastest rate, after the Rate target: a fifth of the block rate, 400 Hz at 2 kHz.
pub(crate) const MAX_RATE: f32 = crate::ENGINE_RATE / crate::voice::BLOCK as f32 / 5.0;

/// xorshift32: the LFOs' and the notes' random draws. Never zero.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Rng(u32);

impl Rng {
    pub(crate) const fn new(seed: u32) -> Self {
        Self(if seed == 0 { 0x9e37_79b9 } else { seed })
    }

    /// Uniform in 0 to 1, 24 bits.
    #[inline]
    pub(crate) fn unit(&mut self) -> f32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        (x >> 8) as f32 / 16_777_216.0
    }

    /// Uniform in −1 to 1.
    #[inline]
    pub(crate) fn bipolar(&mut self) -> f32 {
        self.unit() * 2.0 - 1.0
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Lfo {
    /// 0 to 1.
    phase: f32,
    /// Sample-and-hold's value.
    held: f32,
    /// Smooth random's last point and next.
    from: f32,
    to: f32,
    rng: Rng,
    seed: u32,
}

impl Lfo {
    pub(crate) fn new(seed: u32) -> Self {
        let mut lfo = Self {
            phase: 0.0,
            held: 0.0,
            from: 0.0,
            to: 0.0,
            rng: Rng::new(seed),
            seed,
        };
        lfo.reset();
        lfo
    }

    /// As made: phase 0 and the random stream from its seed.
    pub(crate) fn reset(&mut self) {
        self.rng = Rng::new(self.seed);
        self.phase = 0.0;
        self.held = self.rng.bipolar();
        self.from = self.rng.bipolar();
        self.to = self.rng.bipolar();
    }

    /// Phase 0, as a note restarts it: a new held value, and the smooth random sets off from
    /// where it was toward a new point.
    pub(crate) fn restart(&mut self) {
        self.from = self.smooth();
        self.phase = 0.0;
        self.held = self.rng.bipolar();
        self.to = self.rng.bipolar();
    }

    /// Smooth random: a raised-cosine glide from one random point to the next, a point a cycle,
    /// so it has no corners and never leaves −1 to 1.
    #[inline]
    fn smooth(&self) -> f32 {
        let t = 0.5 - 0.5 * (std::f32::consts::PI * self.phase).cos();
        self.from + (self.to - self.from) * t
    }

    /// The output at the current phase.
    #[inline]
    pub(crate) fn value(&self, shape: LfoShape) -> f32 {
        let p = self.phase;
        match shape {
            LfoShape::Sine => (std::f32::consts::TAU * p).sin(),
            // From 0, rising, as the sine.
            LfoShape::Triangle => {
                if p < 0.25 {
                    4.0 * p
                } else if p < 0.75 {
                    2.0 - 4.0 * p
                } else {
                    4.0 * p - 4.0
                }
            }
            LfoShape::SawUp => 2.0 * p - 1.0,
            LfoShape::SawDown => 1.0 - 2.0 * p,
            LfoShape::Square => {
                if p < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            }
            LfoShape::SampleAndHold => self.held,
            LfoShape::SmoothRandom => self.smooth(),
        }
    }

    /// One block: the output at the current phase, then the phase moves on `rate_hz × dt`; each
    /// new cycle draws the next random value.
    #[inline]
    pub(crate) fn advance(&mut self, shape: LfoShape, rate_hz: f32, dt: f32) -> f32 {
        let value = self.value(shape);
        let rate = if rate_hz.is_finite() {
            rate_hz.clamp(0.0, MAX_RATE)
        } else {
            0.0
        };
        self.phase += rate * dt;
        if self.phase >= 1.0 {
            self.phase = self.phase.fract();
            self.held = self.rng.bipolar();
            self.from = self.to;
            self.to = self.rng.bipolar();
        }
        value
    }
}

/// A depth rising from 0 to 1 over the fade time after each note.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Fade(f32);

impl Default for Fade {
    fn default() -> Self {
        Self(1.0)
    }
}

impl Fade {
    pub(crate) fn restart(&mut self, fade_s: f32) {
        self.0 = if fade_s > 0.0 { 0.0 } else { 1.0 };
    }

    /// The depth for this block, then one block on.
    #[inline]
    pub(crate) fn advance(&mut self, fade_s: f32, dt: f32) -> f32 {
        let depth = self.0;
        self.0 = if fade_s > 0.0 {
            (self.0 + dt / fade_s).min(1.0)
        } else {
            1.0
        };
        depth
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = 12.0 / 48_000.0;

    /// Each shape's values at a quarter of its cycle, from phase 0.
    #[test]
    fn the_shapes() {
        let at = |shape: LfoShape, phase: f32| {
            let mut lfo = Lfo::new(7);
            lfo.phase = phase;
            lfo.value(shape)
        };
        for (shape, want) in [
            (LfoShape::Sine, [0.0, 1.0, 0.0, -1.0]),
            (LfoShape::Triangle, [0.0, 1.0, 0.0, -1.0]),
            (LfoShape::SawUp, [-1.0, -0.5, 0.0, 0.5]),
            (LfoShape::SawDown, [1.0, 0.5, 0.0, -0.5]),
            (LfoShape::Square, [1.0, 1.0, -1.0, -1.0]),
        ] {
            for (q, w) in want.iter().enumerate() {
                let v = at(shape, q as f32 * 0.25);
                assert!((v - w).abs() < 1e-6, "{shape:?} at {q}/4: {v}");
            }
        }
    }

    /// Sample-and-hold holds a value a cycle and draws a new one at each; smooth random moves
    /// without a step, inside −1 to 1, reaching each new point at the cycle's end.
    #[test]
    fn the_random_shapes() {
        let mut lfo = Lfo::new(11);
        let rate = 40.0;
        let per_cycle = (1.0 / (rate * DT)) as usize;
        let mut held = Vec::new();
        for _ in 0..per_cycle * 8 {
            held.push(lfo.advance(LfoShape::SampleAndHold, rate, DT));
        }
        let changes = held.windows(2).filter(|w| w[0] != w[1]).count();
        assert!((7..=8).contains(&changes), "{changes}");
        let mut lfo = Lfo::new(11);
        let mut last = lfo.value(LfoShape::SmoothRandom);
        let mut largest = 0.0f32;
        for _ in 0..per_cycle * 50 {
            let v = lfo.advance(LfoShape::SmoothRandom, rate, DT);
            assert!((-1.0..=1.0).contains(&v));
            largest = largest.max((v - last).abs());
            last = v;
        }
        // At 40 Hz a cycle is 100 blocks; the steepest a raised cosine across 2 gets is π/100.
        assert!(largest < 0.04, "{largest}");
    }

    /// The rate is cycles a second.
    #[test]
    fn the_rate_is_in_hertz() {
        let mut lfo = Lfo::new(3);
        // A second and two blocks: the fifth wrap lands on the second's last block, give or take
        // the rounding of the phase's sum.
        let blocks = (1.0 / DT) as usize + 2;
        let mut wraps = 0;
        let mut was = lfo.value(LfoShape::SawUp);
        for _ in 0..blocks {
            lfo.advance(LfoShape::SawUp, 5.0, DT);
            let v = lfo.value(LfoShape::SawUp);
            if v < was {
                wraps += 1;
            }
            was = v;
        }
        assert_eq!(wraps, 5);
    }

    /// A restart is phase 0.
    #[test]
    fn a_restart_is_phase_zero() {
        let mut lfo = Lfo::new(5);
        for _ in 0..37 {
            lfo.advance(LfoShape::Sine, 3.3, DT);
        }
        lfo.restart();
        assert_eq!(lfo.advance(LfoShape::SawUp, 3.3, DT), -1.0);
    }

    /// The fade rises from 0 to 1 over its time; with no time it is 1 at once.
    #[test]
    fn the_fade() {
        let mut fade = Fade::default();
        fade.restart(0.5);
        assert_eq!(fade.advance(0.5, DT), 0.0);
        let mut depth = 0.0;
        for _ in 0..999 {
            depth = fade.advance(0.5, DT);
        }
        assert!((depth - 0.4995).abs() < 1e-3, "{depth}");
        for _ in 0..2000 {
            depth = fade.advance(0.5, DT);
        }
        assert_eq!(depth, 1.0);
        fade.restart(0.0);
        assert_eq!(fade.advance(0.0, DT), 1.0);
    }
}
