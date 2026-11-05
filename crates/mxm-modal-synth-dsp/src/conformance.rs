//! The slots as the collection's modulation standard checks them (`mxm_modulation::conformance`;
//! the plan's §5.7): every pair offered as the standard says and reaching what it says, every
//! performance source published as it says, and no performance route holding a note open after its
//! release. [`Declared`] answers through the crate's own grid ([`Routing`]) and a real
//! [`Instrument`], never a copy of their tables.

use mxm_modulation::conformance::{self, Case, Declaration, Input, Kind};
use mxm_modulation::standard::{self, Law, Offer, Performance};
use std::collections::HashMap;

use crate::routing::{self, KEY_UNIT, Routing};
use crate::{Instrument, SLOTS, Settings, Slot, Source, Target};

/// What each target is, for the standard.
const fn kind(target: Target) -> Kind {
    match target {
        Target::Pitch => Kind::Pitch,
        Target::Amplitude => Kind::Amplitude,
        Target::Pan => Kind::Pan,
        Target::Lfo1Rate
        | Target::Lfo2Rate
        | Target::Lfo3Rate
        | Target::Envelope1Time
        | Target::Envelope2Time
        | Target::Envelope3Time => Kind::Rate,
        // Four models, latched at the note: the module's own selector, no standard reach.
        Target::Model => Kind::Machine(Law::Sum),
        // The module's Space CV, to 2: its own range, past the freeze, no standard reach.
        Target::Space => Kind::Machine(Law::Sum),
        Target::Envelope1Cycle
        | Target::Envelope1Trigger
        | Target::Envelope2Cycle
        | Target::Envelope2Trigger
        | Target::Envelope3Cycle
        | Target::Envelope3Trigger => Kind::Machine(Law::Edge),
        // The knobs, Strength, an envelope's Rise and Fall (a fraction of the time range) and
        // Input.
        _ => Kind::Control,
    }
}

/// One slot, as the plugin would fill it.
fn one_slot(source: Source, target: Target, amount: f32) -> [Slot; SLOTS] {
    let mut slots = [Slot::default(); SLOTS];
    slots[0] = Slot {
        source: Some(source),
        target: Some(target),
        amount,
    };
    slots
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Declared;

impl Declaration for Declared {
    fn sources(&self) -> usize {
        routing::SOURCES
    }

    fn targets(&self) -> usize {
        routing::TARGETS
    }

    fn performance(&self, source: usize) -> Option<Performance> {
        Source::ALL[source].performance()
    }

    fn kind(&self, target: usize) -> Kind {
        kind(Target::ALL[target])
    }

    fn machine(&self, _target: usize, _source: usize) -> bool {
        false
    }

    fn offered(&self, target: usize, source: usize) -> Offer {
        Target::ALL[target].offer(Source::ALL[source])
    }

    fn key_unit(&self) -> f32 {
        KEY_UNIT
    }

    /// One slot alone through the grid, its source publishing `raw`; Amplitude through the
    /// standard factor the voice applies.
    fn deliver(&self, target: usize, source: usize, amount: f32, raw: f32) -> f32 {
        let (target, source) = (Target::ALL[target], Source::ALL[source]);
        let mut routing = Routing::new();
        routing.update(&one_slot(source, target, amount));
        let mut frame = routing::Frame::new();
        frame.begin_sample();
        frame.write(source.index(), raw);
        let sum = routing.sum(target, &frame);
        if target == Target::Amplitude {
            standard::amplitude_factor(sum) - 1.0
        } else {
            sum
        }
    }

    fn name(&self, target: usize, source: usize) -> String {
        format!("{:?} from {:?}", Target::ALL[target], Source::ALL[source])
    }
}

fn report(result: Result<(), Vec<String>>) {
    if let Err(failures) = result {
        panic!("{} failure(s):\n{}", failures.len(), failures.join("\n"));
    }
}

/// **Every pair means what the standard says**: offered as it says, nothing at its source's
/// rest, a meaningful move at full, and the standard reach on every added pair.
///
/// Falsified before trusted: Key → Pitch at 12 semitones a unit (the Key unit forgotten) fails
/// every Key row; offering Wheel → a Trigger fails the edge rows.
#[test]
fn every_pair_means_what_the_standard_says() {
    report(conformance::check_declaration(&Declared));
}

/// **A voice publishes what the standard says**: Key the glided note on a sixty-semitone unit,
/// Velocity `v − 1`, the wheel, pressure and the lever as they arrive, each exactly zero at rest.
///
/// Falsified before trusted: publishing the raw velocity fails every Velocity input.
#[test]
fn a_voice_publishes_what_the_standard_says() {
    report(conformance::check_publishers(&Declared, |from, input| {
        let source = Source::ALL[from];
        let mut instrument = Instrument::new();
        let mut note = 60;
        let mut velocity = 1.0;
        match input {
            Input::Note(n) => note = n,
            Input::Normalised(v) if source == Source::Velocity => velocity = v,
            Input::Normalised(v) if source == Source::Wheel => instrument.set_wheel(v),
            Input::Normalised(v) => instrument.set_pressure(v),
            Input::Lever(v) => instrument.set_bend(v),
        }
        instrument.note_on(note, velocity, None);
        let (mut l, mut r) = ([0.0; 16], [0.0; 16]);
        instrument.render(&Settings::default(), &mut l, &mut r);
        instrument.voices.voices[0].published(source)
    }));
}

/// Random is drawn per note, `2u − 1`: inside −1 to 1, its mean near zero, and new each note.
#[test]
fn random_is_drawn_per_note_and_bipolar() {
    let mut instrument = Instrument::new();
    let (mut l, mut r) = ([0.0; 16], [0.0; 16]);
    let mut draws = Vec::new();
    for _ in 0..400 {
        instrument.note_on(60, 1.0, None);
        instrument.render(&Settings::default(), &mut l, &mut r);
        let voice = instrument
            .voices
            .voices
            .iter()
            .max_by_key(|v| v.started)
            .unwrap();
        draws.push(voice.published(Source::Random));
        instrument.note_off(60, None);
    }
    assert!(draws.iter().all(|d| (-1.0..=1.0).contains(d)));
    let mean = draws.iter().sum::<f32>() / draws.len() as f32;
    assert!(mean.abs() < 0.1, "{mean}");
    assert!(draws.windows(2).all(|w| w[0] != w[1]));
    assert!(draws.iter().any(|&d| d < -0.8) && draws.iter().any(|&d| d > 0.8));
}

/// **After a release, no performance route holds a note open**: every offered pair from a
/// performance source, each offered half, the softest and hardest notes and the lowest and
/// highest keys, the wheel and pressure at full through the note and let go after it. The voice
/// reaches idle, and idle is exact silence.
///
/// A short sound keeps the cases affordable (Contour percussive, Damping 0, Space dry), and a case
/// whose source ignores the key (or the velocity) is rendered once for both. A note the 30 s
/// guard had to end was held open. **Exempt, as the module is**: a note-long source (Key,
/// Velocity, Random) on Damping, which at its top rings for minutes (a string's crossfades to an
/// infinite decay), or on Space, which past 1.75 freezes the reverb, or on Model, which can latch
/// either; neither is a gate, and on the module a CV there does the same.
///
/// Falsified before trusted: the Damping exemption dropped, Key → Damping at the top key holds
/// the note open until the guard.
#[test]
fn after_a_release_no_performance_route_holds_a_note_open() {
    let mut ended: HashMap<(usize, usize, u32, u32, u8), bool> = HashMap::new();
    let held = [Source::Key, Source::Velocity, Source::Random];
    let exempt: Vec<(usize, usize)> = [Target::Damping, Target::Space, Target::Model]
        .iter()
        .flat_map(|t| held.iter().map(|s| (t.index(), s.index())))
        .collect();
    report(conformance::check_release_silence(
        &Declared,
        &exempt,
        |case: Case| {
            let source = Source::ALL[case.source];
            let velocity = if source == Source::Velocity {
                case.velocity
            } else {
                1.0
            };
            let key = if source == Source::Key { case.key } else { 60 };
            let id = (
                case.target,
                case.source,
                case.amount.to_bits(),
                velocity.to_bits(),
                key,
            );
            *ended.entry(id).or_insert_with(|| {
                let settings = Settings {
                    contour: 0.0,
                    damping: 0.0,
                    space: 0.0,
                    slots: one_slot(source, Target::ALL[case.target], case.amount),
                    ..Settings::default()
                };
                let mut instrument = Instrument::new();
                instrument.set_wheel(1.0);
                instrument.set_pressure(1.0);
                instrument.note_on(key, velocity, None);
                let (mut l, mut r) = ([0.0; 320], [0.0; 320]);
                for _ in 0..10 {
                    instrument.render(&settings, &mut l, &mut r);
                }
                instrument.note_off(key, None);
                instrument.set_wheel(0.0);
                instrument.set_pressure(0.0);
                for _ in 0..6_000 {
                    instrument.render(&settings, &mut l, &mut r);
                    if !instrument.is_sounding() {
                        instrument.render(&settings, &mut l, &mut r);
                        let guarded = instrument
                            .voices
                            .voices
                            .iter()
                            .map(|v| v.guarded)
                            .sum::<u32>();
                        return guarded == 0 && l.iter().chain(&r).all(|&x| x == 0.0);
                    }
                }
                false
            })
        },
    ));
}
