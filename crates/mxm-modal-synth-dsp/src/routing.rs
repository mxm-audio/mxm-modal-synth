//! The sixteen route slots on the collection's routing (`mxm-modulation`): the sources' and
//! targets' indices, each target's law, reach and bound, which pairs the standard offers, and the
//! grid the slots fill.
//!
//! **The grid is mxm-modulation's**: a target has one route per source ([`Compacted`]) and an
//! amount per source, so the slots fill it: a slot in use sets its pair present and adds its
//! amount there, two slots on one pair adding. It is rebuilt when a slot changes, never per block.
//! A pair the standard refuses (a gesture into a gate) is not filled, and a one-sided pair keeps
//! only its live half, so such a slot does nothing.

use mxm_modulation::standard::{self, AMPLITUDE_SUM_BOUND, Law, Offer, Performance, reach};
use mxm_modulation::{Compacted, SourceFrame};

use crate::{SLOTS, Slot, Source, Target};

/// Sources, and targets: [`Source::ALL`] and [`Target::ALL`], in that order.
pub(crate) const SOURCES: usize = Source::ALL.len();
pub(crate) const TARGETS: usize = Target::ALL.len();

/// The Key unit: sixty semitones a published unit, the standard's usual one, so five octaves
/// either side of middle C fill the frame and the top seven keys track no further.
pub(crate) const KEY_UNIT: f32 = 60.0;

/// Model's reach: three models either way, so one slot reaches every model from any.
pub(crate) const MODEL_REACH: f32 = (crate::MODELS - 1) as f32;

/// Space's reach: the module's Space CV takes it to 2, past the freeze at 1.75, from a knob that
/// stops at 1.
pub(crate) const SPACE_REACH: f32 = 2.0;

/// One voice's source values, with the unit delay.
pub(crate) type Frame = SourceFrame<SOURCES>;

impl Source {
    /// Its place in [`Source::ALL`], the grid's index.
    #[inline]
    pub(crate) const fn index(self) -> usize {
        self as usize
    }

    /// The standard's performance source this is, or `None` for a generator.
    pub(crate) const fn performance(self) -> Option<Performance> {
        match self {
            Self::Key => Some(Performance::Key),
            Self::Velocity => Some(Performance::Velocity),
            Self::Wheel => Some(Performance::Wheel),
            Self::Pressure => Some(Performance::Pressure),
            Self::Bend => Some(Performance::Bend),
            Self::Random => Some(Performance::Random),
            _ => None,
        }
    }

    /// An envelope's level, its end of rise or its end of cycle.
    pub(crate) const fn envelope(e: usize) -> [Self; 3] {
        match e {
            0 => [Self::Envelope1, Self::Envelope1Rise, Self::Envelope1Cycle],
            1 => [Self::Envelope2, Self::Envelope2Rise, Self::Envelope2Cycle],
            _ => [Self::Envelope3, Self::Envelope3Rise, Self::Envelope3Cycle],
        }
    }

    pub(crate) const fn lfo(l: usize) -> Self {
        match l {
            0 => Self::Lfo1,
            1 => Self::Lfo2,
            _ => Self::Lfo3,
        }
    }
}

/// An envelope's six targets, in their order.
#[derive(Clone, Copy)]
pub(crate) struct EnvelopeTargets {
    pub rise: Target,
    pub fall: Target,
    pub time: Target,
    pub cycle: Target,
    pub trigger: Target,
    pub input: Target,
}

impl Target {
    /// Its place in [`Target::ALL`], the grid's index.
    #[inline]
    pub(crate) const fn index(self) -> usize {
        self as usize
    }

    pub(crate) const fn lfo_rate(l: usize) -> Self {
        match l {
            0 => Self::Lfo1Rate,
            1 => Self::Lfo2Rate,
            _ => Self::Lfo3Rate,
        }
    }

    pub(crate) const fn envelope(e: usize) -> EnvelopeTargets {
        match e {
            0 => EnvelopeTargets {
                rise: Self::Envelope1Rise,
                fall: Self::Envelope1Fall,
                time: Self::Envelope1Time,
                cycle: Self::Envelope1Cycle,
                trigger: Self::Envelope1Trigger,
                input: Self::Envelope1Input,
            },
            1 => EnvelopeTargets {
                rise: Self::Envelope2Rise,
                fall: Self::Envelope2Fall,
                time: Self::Envelope2Time,
                cycle: Self::Envelope2Cycle,
                trigger: Self::Envelope2Trigger,
                input: Self::Envelope2Input,
            },
            _ => EnvelopeTargets {
                rise: Self::Envelope3Rise,
                fall: Self::Envelope3Fall,
                time: Self::Envelope3Time,
                cycle: Self::Envelope3Cycle,
                trigger: Self::Envelope3Trigger,
                input: Self::Envelope3Input,
            },
        }
    }

    /// What the target does with its summed routes, for the standard's offer: Cycle and Trigger
    /// are gates; Amplitude is the standard factor; every other target adds (Strength too: an
    /// accent around the module's own, never a gate a held source keeps open).
    pub(crate) const fn law(self) -> Law {
        match self {
            Self::Amplitude => Law::Factor,
            Self::Envelope1Cycle
            | Self::Envelope1Trigger
            | Self::Envelope2Cycle
            | Self::Envelope2Trigger
            | Self::Envelope3Cycle
            | Self::Envelope3Trigger => Law::Edge,
            _ => Law::Sum,
        }
    }

    /// Whether, and on which half, a slot from `source` moves this target: the collection's
    /// standard ([`standard::offer`]). No pair here is the machine's own: the module's one
    /// modulation of its own, its resonator's slow sweep, is fixed in the patch, not a slot's.
    pub const fn offer(self, source: Source) -> Offer {
        standard::offer(self.law(), source.performance(), false)
    }

    /// What an amount of one with its source at one moves the target by, in the target's unit:
    /// semitones on Pitch, models on Model, Space's CV range, octaves on a rate and on an
    /// envelope's Time, the factor's swing on Amplitude, and every other target its whole range
    /// (a control, Strength, Pan, an envelope's Rise and Fall as a fraction of the time range, a
    /// gate, an Input).
    pub(crate) const fn reach(self) -> f32 {
        match self {
            Self::Pitch => reach::PITCH_SEMITONES,
            Self::Model => MODEL_REACH,
            Self::Space => SPACE_REACH,
            Self::Amplitude => reach::AMPLITUDE,
            Self::Lfo1Rate
            | Self::Lfo2Rate
            | Self::Lfo3Rate
            | Self::Envelope1Time
            | Self::Envelope2Time
            | Self::Envelope3Time => reach::OCTAVES,
            _ => reach::CONTROL,
        }
    }

    /// What a Key route at an amount of one moves the target by per octave of keyboard: the
    /// standard's (12 st on Pitch, an octave per octave on an exponential target, a fifth of the
    /// reach on a linear one).
    const fn key_reach_per_octave(self) -> f32 {
        match self {
            Self::Pitch => reach::KEY_PITCH_SEMITONES_PER_OCTAVE,
            Self::Lfo1Rate
            | Self::Lfo2Rate
            | Self::Lfo3Rate
            | Self::Envelope1Time
            | Self::Envelope2Time
            | Self::Envelope3Time => reach::KEY_OCTAVES_PER_OCTAVE,
            _ => self.reach() * reach::KEY_LINEAR_FRACTION_PER_OCTAVE,
        }
    }

    /// The bound on the target's sum, in its unit: wide enough for every slot at once, finite so a
    /// sum is.
    const fn bound(self) -> f32 {
        match self {
            Self::Pitch => 120.0,
            Self::Model => 2.0 * MODEL_REACH,
            Self::Amplitude => AMPLITUDE_SUM_BOUND,
            _ => 16.0,
        }
    }
}

/// Each pair's full scale: the target's reach, through the Key unit for a Key route.
const SCALES: [[f32; SOURCES]; TARGETS] = {
    let mut table = [[0.0; SOURCES]; TARGETS];
    let mut t = 0;
    while t < TARGETS {
        let target = Target::ALL[t];
        let mut s = 0;
        while s < SOURCES {
            table[t][s] = if s == Source::Key.index() {
                standard::key_scale(target.key_reach_per_octave(), KEY_UNIT)
            } else {
                target.reach()
            };
            s += 1;
        }
        t += 1;
    }
    table
};

/// The slots' grid.
#[derive(Clone, Debug)]
pub(crate) struct Routing {
    /// The slots it was built from.
    built: [Slot; SLOTS],
    live: [Compacted<SOURCES>; TARGETS],
    amounts: [[f32; SOURCES]; TARGETS],
    /// Whether any live route reads each source.
    reads: [bool; SOURCES],
}

impl Default for Routing {
    fn default() -> Self {
        Self::new()
    }
}

impl Routing {
    pub(crate) fn new() -> Self {
        Self {
            built: [Slot::default(); SLOTS],
            live: std::array::from_fn(|_| Compacted::new()),
            amounts: [[0.0; SOURCES]; TARGETS],
            reads: [false; SOURCES],
        }
    }

    /// Rebuilds the grid if a slot changed. Allocation-free.
    pub(crate) fn update(&mut self, slots: &[Slot; SLOTS]) {
        if *slots == self.built {
            return;
        }
        self.built = *slots;
        let mut present = [[false; SOURCES]; TARGETS];
        self.amounts = [[0.0; SOURCES]; TARGETS];
        for slot in slots {
            let (Some(source), Some(target)) = (slot.source, slot.target) else {
                continue;
            };
            let amount = if slot.amount.is_finite() {
                slot.amount
            } else {
                0.0
            };
            let amount = match target.offer(source) {
                Offer::Refused => continue,
                Offer::Both => amount,
                Offer::PositiveOnly => amount.max(0.0),
                Offer::NegativeOnly => amount.min(0.0),
            };
            present[target.index()][source.index()] = true;
            self.amounts[target.index()][source.index()] += amount;
        }
        self.reads = [false; SOURCES];
        for (t, live) in self.live.iter_mut().enumerate() {
            live.build(&present[t]);
            for &s in live.sources() {
                self.reads[s] = true;
            }
        }
    }

    /// The target's summed routes in its unit, bounded.
    #[inline]
    pub(crate) fn sum(&self, target: Target, frame: &Frame) -> f32 {
        let t = target.index();
        mxm_modulation::sum_scaled(
            frame,
            &self.live[t],
            &self.amounts[t],
            &SCALES[t],
            target.bound(),
        )
    }

    /// Whether any live route moves the target.
    #[cfg(test)]
    pub(crate) fn patched(&self, target: Target) -> bool {
        !self.live[target.index()].is_empty()
    }

    /// Whether any live route reads the source.
    #[inline]
    pub(crate) fn reads(&self, source: Source) -> bool {
        self.reads[source.index()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_indices_are_the_lists_order() {
        for (i, s) in Source::ALL.iter().enumerate() {
            assert_eq!(s.index(), i);
        }
        for (i, t) in Target::ALL.iter().enumerate() {
            assert_eq!(t.index(), i);
        }
    }

    fn one(source: Source, target: Target, amount: f32) -> Routing {
        let mut slots = [Slot::default(); SLOTS];
        slots[3] = Slot {
            source: Some(source),
            target: Some(target),
            amount,
        };
        let mut routing = Routing::new();
        routing.update(&slots);
        routing
    }

    fn frame_with(source: Source, value: f32) -> Frame {
        let mut frame = Frame::new();
        frame.begin_sample();
        frame.write(source.index(), value);
        frame
    }

    /// A slot is in use only with both a source and a target; two on one pair add.
    #[test]
    fn a_slot_needs_both_ends_and_two_on_one_pair_add() {
        let mut slots = [Slot::default(); SLOTS];
        slots[0] = Slot {
            source: Some(Source::Lfo1),
            target: None,
            amount: 1.0,
        };
        let mut routing = Routing::new();
        routing.update(&slots);
        assert!(!routing.reads(Source::Lfo1));
        slots[0].target = Some(Target::Pitch);
        slots[5] = Slot {
            source: Some(Source::Lfo1),
            target: Some(Target::Pitch),
            amount: 0.5,
        };
        routing.update(&slots);
        let frame = frame_with(Source::Lfo1, 1.0);
        assert_eq!(routing.sum(Target::Pitch, &frame), 18.0);
        assert!(routing.patched(Target::Pitch));
        assert!(!routing.patched(Target::Brightness));
    }

    /// A gesture into a gate is refused; Velocity into Strength is an accent, both ways.
    #[test]
    fn refused_pairs_do_nothing() {
        let frame = frame_with(Source::Wheel, 1.0);
        let routing = one(Source::Wheel, Target::Envelope1Trigger, 1.0);
        assert!(!routing.patched(Target::Envelope1Trigger));
        assert_eq!(routing.sum(Target::Envelope1Trigger, &frame), 0.0);
        let routing = one(Source::Velocity, Target::Strength, 0.5);
        let frame = frame_with(Source::Velocity, -1.0);
        assert!(routing.patched(Target::Strength));
        assert_eq!(routing.sum(Target::Strength, &frame), -0.5);
        // An LFO into a gate is a clock.
        let routing = one(Source::Lfo2, Target::Envelope3Cycle, 1.0);
        assert!(routing.patched(Target::Envelope3Cycle));
    }

    /// A Key route takes the standard per-octave reach through the Key unit.
    #[test]
    fn a_key_route_reaches_per_octave() {
        let octave = standard::key(72.0, KEY_UNIT);
        let routing = one(Source::Key, Target::Pitch, 1.0);
        let pitch = routing.sum(Target::Pitch, &frame_with(Source::Key, octave));
        assert!((pitch - 12.0).abs() < 1e-5, "{pitch}");
        let routing = one(Source::Key, Target::Lfo1Rate, 1.0);
        let rate = routing.sum(Target::Lfo1Rate, &frame_with(Source::Key, octave));
        assert!((rate - 1.0).abs() < 1e-6, "{rate}");
        let routing = one(Source::Key, Target::Brightness, 1.0);
        let timbre = routing.sum(Target::Brightness, &frame_with(Source::Key, octave));
        assert!((timbre - 0.2).abs() < 1e-6, "{timbre}");
    }

    /// A non-finite amount fills nothing.
    #[test]
    fn a_non_finite_amount_is_zero() {
        let routing = one(Source::Lfo1, Target::Geometry, f32::NAN);
        assert_eq!(
            routing.sum(Target::Geometry, &frame_with(Source::Lfo1, 1.0)),
            0.0
        );
    }
}
