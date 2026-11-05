//! The three envelopes: ADSRs with MATHS' functions (`research:modulation/make-noise-maths.md`;
//! the plan's §5.4). Modelled from the manual, not ported, and run once per engine block (2 kHz).
//!
//! **One slope core.** The envelope is a ramp `x` from 0 to 1 that rises at the attack's rate and
//! falls at the decay's or the release's, and its output is `curve(x)`. The curve is MATHS'
//! Vari-Response, which the manual defines against the voltage, not the time: *"With the LOG
//! response, the rate of change decreases as the voltage increases. With EXPO response, the rate of
//! change increases as the voltage increases."* A curve of `x` does exactly that for the rise and
//! the fall alike, and a retrigger from part way continues from the level it is at. A time is the
//! whole 0-to-1 travel, so a segment over part of it (a decay to the sustain, a release from it)
//! takes its share; the curve does not change the time (MATHS' does, which the manual leaves
//! unmeasured).
//!
//! **Cycling** rises to 0.8 (the manual's 8 V) and falls to 0, the attack and the decay repeating;
//! a triggered or gated function rises to 1 (10 V).

use crate::{EnvelopeMode, EnvelopeSettings};

/// The shortest and longest time a parameter sets.
pub(crate) const MIN_TIME: f32 = 0.0005;
pub(crate) const MAX_TIME: f32 = 30.0;
/// The time range Rise and Fall reach at an amount of one.
pub(crate) const TIME_RANGE: f32 = MAX_TIME - MIN_TIME;
/// Where a cycling function turns: the manual's *"0-8V when Cycling"*.
pub(crate) const CYCLE_PEAK: f32 = 0.8;
/// The curve's steepness at Response ±1: `e^6 − 1` from the flat end to the steep one, so an
/// exponential fall is down 26 dB half way and a logarithmic rise is at 95 %.
const CURVE: f32 = 6.0;

/// One curve for every segment: −1 logarithmic, 0 linear, +1 exponential.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Curve {
    k: f32,
    /// `e^|k| − 1`.
    span: f32,
}

impl Curve {
    pub(crate) fn new(response: f32) -> Self {
        let k = response.clamp(-1.0, 1.0) * CURVE;
        if k.abs() < 1e-3 {
            Self { k: 0.0, span: 0.0 }
        } else {
            Self {
                k,
                span: k.abs().exp_m1(),
            }
        }
    }

    /// The output at ramp position `x`.
    #[inline]
    pub(crate) fn shape(self, x: f32) -> f32 {
        let x = x.clamp(0.0, 1.0);
        if self.k == 0.0 {
            x
        } else if self.k > 0.0 {
            (self.k * x).exp_m1() / self.span
        } else {
            1.0 - (-self.k * (1.0 - x)).exp_m1() / self.span
        }
    }

    /// The ramp position whose output is `v`.
    #[inline]
    pub(crate) fn inverse(self, v: f32) -> f32 {
        let v = v.clamp(0.0, 1.0);
        if self.k == 0.0 {
            v
        } else if self.k > 0.0 {
            (v * self.span).ln_1p() / self.k
        } else {
            1.0 - ((1.0 - v) * self.span).ln_1p() / -self.k
        }
    }
}

/// Where the envelope is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Stage {
    Rest,
    Attack,
    /// Toward the sustain level, and holding there: the decay and the sustain are one stage.
    Decay,
    Release,
    /// A cycling function's fall.
    Fall,
}

/// What reaches an envelope this block, besides its settings.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Inputs {
    /// The note's gate (Note mode).
    pub note_gate: bool,
    /// A new note this block, not a tied one (Note mode).
    pub note_on: bool,
    /// The Rise and Fall targets, as fractions of the time range; positive slows.
    pub rise: f32,
    pub fall: f32,
    /// The Time target, in octaves; positive shortens.
    pub time: f32,
    /// The Cycle and Trigger targets' sums: high above one half.
    pub cycle: f32,
    pub trigger: f32,
    /// The Input target (Follow).
    pub input: f32,
}

/// The times a block runs at, after the Rise, Fall and Time targets.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Times {
    pub attack: f32,
    pub decay: f32,
    pub release: f32,
}

impl Times {
    /// Rise and Fall add their share of the range, linearly, as MATHS' Rise and Fall CV, within
    /// the range; Time then divides every time by `2^time`, as MATHS' Both.
    pub(crate) fn new(settings: &EnvelopeSettings, inputs: &Inputs) -> Self {
        let speed = inputs.time.clamp(-16.0, 16.0).exp2();
        let at = |seconds: f32, add: f32| {
            let base = seconds.clamp(MIN_TIME, MAX_TIME);
            (base + add * TIME_RANGE).clamp(MIN_TIME, MAX_TIME) / speed
        };
        Self {
            attack: at(settings.attack_s, inputs.rise),
            decay: at(settings.decay_s, inputs.fall),
            release: at(settings.release_s, inputs.fall),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Envelope {
    /// The ramp, 0 to 1.
    x: f32,
    stage: Stage,
    /// End of rise.
    eor: bool,
    /// The Trigger target was high last block.
    trigger_high: bool,
    /// The attack was started by the Trigger target in Note mode: it completes whatever the
    /// note's gate does, as MATHS' rise ignores the gate's length.
    latched: bool,
    /// Follow mode, at its input and not moving: as still as rest, for the voice's idling.
    settled: bool,
    value: f32,
}

impl Default for Envelope {
    fn default() -> Self {
        Self::new()
    }
}

impl Envelope {
    pub(crate) const fn new() -> Self {
        Self {
            x: 0.0,
            stage: Stage::Rest,
            eor: false,
            trigger_high: false,
            latched: false,
            settled: false,
            value: 0.0,
        }
    }

    pub(crate) fn reset(&mut self) {
        *self = Self::new();
    }

    /// The output, 0 to 1.
    #[inline]
    pub(crate) fn value(&self) -> f32 {
        self.value
    }

    /// End of rise: high from the end of the attack until rest or the next attack. End of cycle
    /// is its complement.
    #[inline]
    pub(crate) fn end_of_rise(&self) -> bool {
        self.eor
    }

    #[inline]
    pub(crate) fn at_rest(&self) -> bool {
        self.stage == Stage::Rest
    }

    /// At rest, or in Follow mode holding its input's level: nothing in it will move until its
    /// input does. A voice may go idle on it; whether it still sounds is the voice's to hear.
    #[inline]
    pub(crate) fn still(&self) -> bool {
        self.at_rest() || self.settled
    }

    #[cfg(test)]
    pub(crate) fn stage(&self) -> Stage {
        self.stage
    }

    /// One block of `dt` seconds.
    pub(crate) fn process(&mut self, settings: &EnvelopeSettings, inputs: &Inputs, dt: f32) {
        let curve = Curve::new(settings.response);
        let times = Times::new(settings, inputs);
        let trigger = inputs.trigger > 0.5;
        let edge = trigger && !self.trigger_high;
        self.trigger_high = trigger;
        if settings.mode == EnvelopeMode::Follow {
            self.follow(curve, &times, inputs.input, dt);
        } else {
            self.settled = false;
            self.function(settings, curve, &times, inputs, trigger, edge, dt);
        }
        self.value = curve.shape(self.x);
    }

    /// Note and Gate modes.
    #[allow(clippy::too_many_arguments)]
    fn function(
        &mut self,
        settings: &EnvelopeSettings,
        curve: Curve,
        times: &Times,
        inputs: &Inputs,
        trigger: bool,
        edge: bool,
        dt: f32,
    ) {
        let note = settings.mode == EnvelopeMode::Note;
        let gate = if note { inputs.note_gate } else { trigger };
        let cycle = settings.cycle || inputs.cycle > 0.5;
        // In Note mode it cycles while the note is held; in Gate mode with or without a trigger.
        let cycling = cycle && (!note || inputs.note_gate);

        // A rising Trigger during the attack is ignored; otherwise it restarts the attack from
        // where the envelope is, as does a new note.
        let retrigger = edge && self.stage != Stage::Attack;
        if note && inputs.note_on {
            self.attack(false);
        } else if retrigger {
            self.attack(note);
        } else if cycling && self.stage == Stage::Rest {
            self.attack(false);
        }

        match self.stage {
            Stage::Attack if !gate && !cycling && !self.latched => self.stage = Stage::Release,
            Stage::Decay if cycling => self.stage = Stage::Fall,
            Stage::Decay if !gate => self.stage = Stage::Release,
            Stage::Fall if !cycling => {
                self.stage = if gate { Stage::Decay } else { Stage::Release }
            }
            Stage::Release if cycling => self.stage = Stage::Fall,
            _ => {}
        }

        match self.stage {
            Stage::Rest => {}
            Stage::Attack => {
                let top = if cycling {
                    curve.inverse(CYCLE_PEAK)
                } else {
                    1.0
                };
                self.x += dt / times.attack;
                if self.x >= top {
                    self.x = top;
                    self.eor = true;
                    self.latched = false;
                    self.stage = if cycling {
                        Stage::Fall
                    } else if gate {
                        Stage::Decay
                    } else {
                        Stage::Release
                    };
                }
            }
            Stage::Decay => {
                let sustain = curve.inverse(settings.sustain.clamp(0.0, 1.0));
                if self.x > sustain {
                    self.x = (self.x - dt / times.decay).max(sustain);
                } else if self.x < sustain {
                    self.x = (self.x + dt / times.attack).min(sustain);
                }
            }
            Stage::Fall => {
                self.x -= dt / times.decay;
                if self.x <= 0.0 {
                    self.x = 0.0;
                    self.eor = false;
                    if cycling {
                        self.attack(false);
                    } else {
                        self.stage = Stage::Rest;
                    }
                }
            }
            Stage::Release => {
                self.x -= dt / times.release;
                if self.x <= 0.0 {
                    self.x = 0.0;
                    self.eor = false;
                    self.stage = Stage::Rest;
                }
            }
        }
    }

    fn attack(&mut self, latched: bool) {
        self.stage = Stage::Attack;
        self.eor = false;
        self.latched = latched;
    }

    /// Follow: toward the Input at the attack's rate rising and the release's falling. A level
    /// reached and held counts as the end of a rise; rest is 0 with nothing to follow.
    fn follow(&mut self, curve: Curve, times: &Times, input: f32, dt: f32) {
        let target = curve.inverse(input.clamp(0.0, 1.0));
        self.latched = false;
        if self.x < target {
            self.x = (self.x + dt / times.attack).min(target);
            self.eor = self.x >= target;
            self.stage = Stage::Attack;
        } else if self.x > target {
            self.x = (self.x - dt / times.release).max(target);
            self.eor = self.x > 0.0;
            self.stage = Stage::Release;
        } else {
            self.eor = self.x > 0.0;
            self.stage = Stage::Decay;
        }
        if self.x <= 0.0 && target <= 0.0 {
            self.x = 0.0;
            self.eor = false;
            self.stage = Stage::Rest;
        }
        self.settled = self.x == target;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = 12.0 / 48_000.0;
    /// A segment ends on the block it reaches its level in, a block either way of its time as
    /// the ramp's sum rounds.
    const ROUNDING: f32 = 1.5 * DT;

    fn settings(mode: EnvelopeMode) -> EnvelopeSettings {
        EnvelopeSettings {
            attack_s: 0.1,
            decay_s: 0.2,
            sustain: 0.5,
            release_s: 0.4,
            response: 0.0,
            cycle: false,
            mode,
        }
    }

    /// Blocks until `done` holds, at most `limit`.
    fn blocks_until(
        env: &mut Envelope,
        s: &EnvelopeSettings,
        inputs: &Inputs,
        limit: usize,
        done: impl Fn(&Envelope) -> bool,
    ) -> Option<usize> {
        for n in 1..=limit {
            env.process(s, inputs, DT);
            if done(env) {
                return Some(n);
            }
        }
        None
    }

    fn seconds(blocks: usize) -> f32 {
        blocks as f32 * DT
    }

    #[test]
    fn the_curve_is_its_own_inverse_and_bends_the_right_way() {
        for r in [-1.0, -0.4, 0.0, 0.3, 1.0] {
            let c = Curve::new(r);
            for v in [0.0, 0.1, 0.5, 0.8, 1.0] {
                assert!((c.shape(c.inverse(v)) - v).abs() < 1e-5, "{r} {v}");
            }
            assert_eq!(c.shape(0.0), 0.0);
            assert!((c.shape(1.0) - 1.0).abs() < 1e-6);
        }
        // Exponential: slow off zero (a rise) and fast off the top (a fall); logarithmic the
        // other way; linear straight.
        assert!(Curve::new(1.0).shape(0.5) < 0.1);
        assert!(Curve::new(-1.0).shape(0.5) > 0.9);
        assert_eq!(Curve::new(0.0).shape(0.25), 0.25);
    }

    /// Note mode: an ADSR. The attack takes its time to the top, the decay its share of the range
    /// to the sustain, and the release from the sustain its share.
    #[test]
    fn note_mode_is_an_adsr() {
        let s = settings(EnvelopeMode::Note);
        let mut env = Envelope::new();
        let on = Inputs {
            note_gate: true,
            note_on: true,
            ..Inputs::default()
        };
        env.process(&s, &on, DT);
        let held = Inputs {
            note_gate: true,
            ..Inputs::default()
        };
        let attack = blocks_until(&mut env, &s, &held, 100_000, |e| e.end_of_rise()).unwrap() + 1;
        assert!(
            (seconds(attack) - 0.1).abs() <= ROUNDING,
            "{}",
            seconds(attack)
        );
        assert_eq!(env.value(), 1.0);
        let decay = blocks_until(&mut env, &s, &held, 100_000, |e| e.value() <= 0.5).unwrap();
        assert!(
            (seconds(decay) - 0.1).abs() <= ROUNDING,
            "{}",
            seconds(decay)
        );
        for _ in 0..1000 {
            env.process(&s, &held, DT);
        }
        assert_eq!(env.value(), 0.5, "it sustains");
        assert!(env.end_of_rise());
        let release =
            blocks_until(&mut env, &s, &Inputs::default(), 100_000, Envelope::at_rest).unwrap();
        assert!(
            (seconds(release) - 0.2).abs() <= ROUNDING,
            "{}",
            seconds(release)
        );
        assert!(!env.end_of_rise(), "end of rise drops at rest");
    }

    /// A note off during the attack releases from where it is.
    #[test]
    fn a_release_during_the_attack_starts_where_it_is() {
        let s = settings(EnvelopeMode::Note);
        let mut env = Envelope::new();
        let on = Inputs {
            note_gate: true,
            note_on: true,
            ..Inputs::default()
        };
        env.process(&s, &on, DT);
        for _ in 0..99 {
            env.process(
                &s,
                &Inputs {
                    note_gate: true,
                    ..Inputs::default()
                },
                DT,
            );
        }
        let reached = env.value();
        assert!((reached - 0.25).abs() < 0.01, "{reached}");
        env.process(&s, &Inputs::default(), DT);
        assert_eq!(env.stage(), Stage::Release);
        assert!(env.value() < reached);
        assert!(!env.end_of_rise());
    }

    /// MATHS' rule: a rising Trigger during the attack is ignored; one during the decay or the
    /// release restarts the attack from where the envelope is, and that attack completes.
    #[test]
    fn the_retrigger_rule() {
        let s = settings(EnvelopeMode::Note);
        let mut env = Envelope::new();
        let trig = |high: bool| Inputs {
            trigger: if high { 1.0 } else { 0.0 },
            ..Inputs::default()
        };
        // A trigger with no note: a function that rises fully, then releases.
        env.process(&s, &trig(true), DT);
        assert_eq!(env.stage(), Stage::Attack);
        env.process(&s, &trig(false), DT);
        assert_eq!(
            env.stage(),
            Stage::Attack,
            "the rise ignores the gate's length"
        );
        let before = env.value();
        env.process(&s, &trig(true), DT);
        assert_eq!(env.stage(), Stage::Attack);
        assert!(
            (env.value() - before - DT / 0.1).abs() < 1e-5,
            "a trigger during the rise changes nothing"
        );
        let top = blocks_until(&mut env, &s, &trig(false), 10_000, |e| e.end_of_rise());
        assert!(top.is_some());
        assert_eq!(env.stage(), Stage::Release);
        for _ in 0..200 {
            env.process(&s, &trig(false), DT);
        }
        let falling = env.value();
        assert!(falling > 0.1 && falling < 1.0);
        env.process(&s, &trig(true), DT);
        assert_eq!(
            env.stage(),
            Stage::Attack,
            "a trigger during the fall restarts"
        );
        assert!(
            (env.value() - falling - DT / 0.1).abs() < 1e-5,
            "from where it is"
        );
        assert!(!env.end_of_rise());
    }

    /// Gate mode: the Trigger target is the gate: rising attacks, high sustains, falling releases.
    #[test]
    fn gate_mode_follows_its_trigger() {
        let s = settings(EnvelopeMode::Gate);
        let mut env = Envelope::new();
        let gate = |high: bool| Inputs {
            trigger: if high { 0.6 } else { 0.4 },
            // A note means nothing in Gate mode.
            note_gate: !high,
            note_on: !high,
            ..Inputs::default()
        };
        env.process(&s, &gate(false), DT);
        assert!(env.at_rest(), "a note does not start it");
        let attack = blocks_until(&mut env, &s, &gate(true), 10_000, |e| e.end_of_rise()).unwrap();
        assert!((seconds(attack) - 0.1).abs() <= ROUNDING);
        for _ in 0..2000 {
            env.process(&s, &gate(true), DT);
        }
        assert_eq!(env.value(), 0.5);
        let release = blocks_until(&mut env, &s, &gate(false), 10_000, Envelope::at_rest).unwrap();
        assert!((seconds(release) - 0.2).abs() <= ROUNDING);
    }

    /// Cycling: attack and decay repeat, the rise turning at 0.8, Sustain unused; the end of rise
    /// is a pulse as wide as the fall. In Note mode it releases at note off from where it is.
    #[test]
    fn cycling_repeats_to_eight_tenths() {
        let mut s = settings(EnvelopeMode::Note);
        s.cycle = true;
        let mut env = Envelope::new();
        env.process(
            &s,
            &Inputs {
                note_gate: true,
                note_on: true,
                ..Inputs::default()
            },
            DT,
        );
        let held = Inputs {
            note_gate: true,
            ..Inputs::default()
        };
        let mut peak = 0.0f32;
        let mut rises = 0;
        let mut was = env.end_of_rise();
        let mut high_blocks = 0;
        // Linear: 0.08 s up and 0.16 s down, so 0.24 s a cycle; four cycles.
        for _ in 0..(4.0 * 0.24 / DT) as usize {
            env.process(&s, &held, DT);
            peak = peak.max(env.value());
            if env.end_of_rise() {
                high_blocks += 1;
            }
            if env.end_of_rise() && !was {
                rises += 1;
            }
            was = env.end_of_rise();
        }
        assert!((peak - CYCLE_PEAK).abs() < 1e-6, "{peak}");
        assert_eq!(rises, 4);
        let width = seconds(high_blocks) / 4.0;
        assert!((width - 0.16).abs() < 2.0 * DT, "{width}");
        assert_ne!(env.stage(), Stage::Decay, "Sustain is unused");
        env.process(&s, &Inputs::default(), DT);
        assert_eq!(env.stage(), Stage::Release);
    }

    /// Gate mode cycles while its Cycle target is high, with no trigger.
    #[test]
    fn gate_mode_cycles_on_its_cycle_target() {
        let s = settings(EnvelopeMode::Gate);
        let mut env = Envelope::new();
        let cycle = Inputs {
            cycle: 0.75,
            ..Inputs::default()
        };
        env.process(&s, &cycle, DT);
        assert_eq!(env.stage(), Stage::Attack);
        let top = blocks_until(&mut env, &s, &cycle, 10_000, |e| e.end_of_rise());
        assert!(top.is_some());
        assert!((env.value() - CYCLE_PEAK).abs() < 1e-6);
        let back = blocks_until(&mut env, &s, &cycle, 10_000, |e| !e.end_of_rise());
        assert!(back.is_some());
        assert_eq!(env.stage(), Stage::Attack, "it cycles again");
        // Cycle low, and no gate: it releases from where it is, to rest.
        let rest = blocks_until(&mut env, &s, &Inputs::default(), 10_000, Envelope::at_rest);
        assert!(rest.is_some());
    }

    /// Follow: toward the Input at the attack's rate up and the release's down; the end of rise
    /// goes high when the level is reached and stays high until rest. Cycle does nothing.
    #[test]
    fn follow_mode_lags_toward_its_input() {
        let mut s = settings(EnvelopeMode::Follow);
        s.cycle = true;
        let mut env = Envelope::new();
        let to = |input: f32| Inputs {
            input,
            note_on: true,
            note_gate: true,
            ..Inputs::default()
        };
        let up = blocks_until(&mut env, &s, &to(0.6), 10_000, |e| e.end_of_rise()).unwrap();
        assert!((seconds(up) - 0.06).abs() <= ROUNDING, "{}", seconds(up));
        assert!((env.value() - 0.6).abs() < 1e-6);
        let down = blocks_until(&mut env, &s, &to(0.2), 10_000, |e| e.value() <= 0.2).unwrap();
        assert!(
            (seconds(down) - 0.16).abs() <= ROUNDING,
            "{}",
            seconds(down)
        );
        assert!(env.end_of_rise());
        let rest = blocks_until(&mut env, &s, &to(-1.0), 10_000, Envelope::at_rest).unwrap();
        assert!((seconds(rest) - 0.08).abs() <= ROUNDING);
        assert!(!env.end_of_rise());
        assert_eq!(env.value(), 0.0);
    }

    /// **A Follow envelope holding a level is still, though not at rest**: following a constant
    /// above zero (Key above middle C, after the note is released), it never reaches rest, and a
    /// voice waiting for rest was never freed. Still is what the voice's idling asks.
    #[test]
    fn a_follow_envelope_holding_a_level_is_still() {
        let s = settings(EnvelopeMode::Follow);
        let mut env = Envelope::new();
        let held = Inputs {
            input: 0.6,
            ..Inputs::default()
        };
        env.process(&s, &held, DT);
        assert!(!env.still(), "moving toward its input");
        blocks_until(&mut env, &s, &held, 10_000, |e| e.value() >= 0.6).unwrap();
        env.process(&s, &held, DT);
        assert!(env.still() && !env.at_rest());
        let moved = Inputs {
            input: 0.3,
            ..Inputs::default()
        };
        env.process(&s, &moved, DT);
        assert!(!env.still(), "its input moved, so it does");
    }

    /// The Time target divides every time by 2^octaves (four octaves at full), positive
    /// shortening; Rise and Fall add their share of the whole range, positive slowing.
    #[test]
    fn the_time_targets_reach() {
        let s = settings(EnvelopeMode::Note);
        let t = Times::new(
            &s,
            &Inputs {
                time: 4.0,
                ..Inputs::default()
            },
        );
        assert!((t.attack - 0.1 / 16.0).abs() < 1e-7);
        assert!((t.release - 0.4 / 16.0).abs() < 1e-7);
        let t = Times::new(
            &s,
            &Inputs {
                time: -4.0,
                ..Inputs::default()
            },
        );
        assert!((t.decay - 0.2 * 16.0).abs() < 1e-5);
        let t = Times::new(
            &s,
            &Inputs {
                rise: 0.5,
                fall: -1.0,
                ..Inputs::default()
            },
        );
        assert!((t.attack - (0.1 + 0.5 * TIME_RANGE)).abs() < 1e-5);
        assert_eq!(t.decay, MIN_TIME);
        assert_eq!(t.release, MIN_TIME);
        let t = Times::new(
            &s,
            &Inputs {
                rise: 1.0,
                ..Inputs::default()
            },
        );
        assert_eq!(t.attack, MAX_TIME, "the whole range, no further");
    }
}
