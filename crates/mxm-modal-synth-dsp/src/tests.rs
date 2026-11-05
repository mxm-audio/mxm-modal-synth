//! The instrument's tests, through its interface and, where a test needs to see inside a block,
//! the voices' and the part's own state.

use mxm_modal_voice::stmlib::dsp::dsp::soft_convert;
use mxm_modal_voice::{Part as ModulePart, PerformanceState};

use crate::voice::{
    BLOCK, MODULATION_FREQUENCY, MODULATION_OFFSET, Played, REVERB_DIFFUSION, REVERB_LP,
};
use crate::{
    EnvelopeMode, Instrument, LfoShape, MODELS, Mode, SLOTS, Settings, Slot, Source, Target, VOICES,
};

fn slots(list: &[(Source, Target, f32)]) -> [Slot; SLOTS] {
    let mut slots = [Slot::default(); SLOTS];
    for (slot, &(source, target, amount)) in slots.iter_mut().zip(list) {
        *slot = Slot {
            source: Some(source),
            target: Some(target),
            amount,
        };
    }
    slots
}

/// `n` host samples of both channels.
fn render(instrument: &mut Instrument, settings: &Settings, n: usize) -> (Vec<f32>, Vec<f32>) {
    let (mut l, mut r) = (vec![0.0; n], vec![0.0; n]);
    instrument.render(settings, &mut l, &mut r);
    (l, r)
}

/// One engine block at 32 kHz, so a test reads the voices between blocks.
fn block(instrument: &mut Instrument, settings: &Settings) -> (Vec<f32>, Vec<f32>) {
    render(instrument, settings, BLOCK)
}

fn played(instrument: &Instrument, voice: usize) -> Played {
    instrument.voices.voices[voice].played
}

fn published(instrument: &Instrument, voice: usize, source: Source) -> f32 {
    instrument.voices.voices[voice].published(source)
}

/// The voice that took the last note.
fn newest(instrument: &Instrument) -> usize {
    (0..VOICES)
        .max_by_key(|&v| instrument.voices.voices[v].started)
        .unwrap()
}

fn sounding(instrument: &Instrument) -> usize {
    instrument.voices.voices.iter().filter(|v| v.active).count()
}

/// **The link to the port's reference**: at 32 kHz, one voice and no slots, the instrument's part
/// is Elements' `Part` exactly, for every model and the hidden voice: the floats it hands its
/// reverb's output stage, every bit, and so the frames, read as `frame / 32768` times Volume. The
/// module's part is driven with what the voice handed its engine each block (its patch, the gate
/// a block after the note, its pitch and strength), and the patch's fields no knob sets are
/// `Part::Init`'s. Compared while the voice sounds; once it is idle the module's part would run on.
#[test]
fn one_voice_is_the_part_exactly() {
    let mut reference = ModulePart::new();
    reference.init();
    let init = *reference.patch();
    assert_eq!(init.resonator_modulation_frequency, MODULATION_FREQUENCY);
    assert_eq!(init.resonator_modulation_offset, MODULATION_OFFSET);
    assert_eq!(init.reverb_diffusion, REVERB_DIFFUSION);
    assert_eq!(init.reverb_lp, REVERB_LP);
    assert_eq!(init.exciter_signature, 0.0);
    for model in 0..MODELS {
        let settings = Settings {
            model,
            bow: 0.4,
            space: 0.8,
            ..Settings::default()
        };
        let mut instrument = Instrument::new();
        let mut part = ModulePart::new();
        part.init();
        if model < 3 {
            part.set_resonator_model(model as u8);
        } else {
            part.set_easter_egg(true);
        }
        let silence = [0.0f32; BLOCK];
        let (mut main, mut aux) = ([0.0f32; BLOCK], [0.0f32; BLOCK]);
        instrument.note_on(60, 1.0, None);
        let mut compared = 0;
        for b in 0..6_000 {
            if b == 2_000 {
                instrument.note_off(60, None);
            }
            let (l, r) = block(&mut instrument, &settings);
            if sounding(&instrument) == 0 {
                break;
            }
            let p = played(&instrument, 0);
            assert_eq!(p.model, model);
            *part.mutable_patch() = p.patch;
            let state = PerformanceState {
                gate: p.gate,
                note: p.note,
                modulation: 0.0,
                strength: p.strength,
            };
            part.process(&state, &silence, &silence, &mut main, &mut aux, BLOCK);
            let (ours_main, ours_aux) = instrument.part.last;
            for i in 0..BLOCK {
                assert_eq!(
                    (ours_main[i].to_bits(), ours_aux[i].to_bits()),
                    (main[i].to_bits(), aux[i].to_bits()),
                    "model {model}, block {b}, sample {i}"
                );
                let want = (
                    f32::from(soft_convert(aux[i])) / 32768.0 * settings.volume,
                    f32::from(soft_convert(main[i])) / 32768.0 * settings.volume,
                );
                assert_eq!((l[i], r[i]), want, "model {model}, block {b}, sample {i}");
            }
            compared += 1;
        }
        assert!(compared > 2_000, "model {model}: {compared} blocks");
        assert!(sounding(&instrument) == 0, "model {model} never went idle");
    }
}

/// **The gate reaches the voice a block after its controls**: a note on in block N gives the
/// voice its pitch and strength in N and the rising gate in N+1; a note off in M the falling gate
/// in M+1.
#[test]
fn the_gate_is_a_block_after_the_controls() {
    let settings = Settings::default();
    let mut instrument = Instrument::new();
    block(&mut instrument, &settings);
    instrument.note_on(67, 1.0, None);
    block(&mut instrument, &settings);
    let p = played(&instrument, 0);
    assert_eq!(
        (p.note, p.gate),
        (67.0, false),
        "the controls, not yet the gate"
    );
    block(&mut instrument, &settings);
    assert!(played(&instrument, 0).gate, "the gate, a block later");
    instrument.note_off(67, None);
    block(&mut instrument, &settings);
    assert!(
        played(&instrument, 0).gate,
        "still high in the note off's block"
    );
    block(&mut instrument, &settings);
    assert!(!played(&instrument, 0).gate, "low a block later");
}

/// **Silence in is exactly zero out once the voices are idle and the reverb has died**, at 32 kHz
/// and through the resampler: every model, four voices, released, then nothing but zeros, and
/// nothing sounding.
#[test]
fn silence_is_exact_zero_once_everything_has_ended() {
    for rate in [32_000.0, 44_100.0] {
        for model in 0..MODELS {
            let settings = Settings {
                model,
                damping: 0.4,
                ..Settings::default()
            };
            let mut instrument = Instrument::new();
            instrument.set_sample_rate(rate);
            for k in 0..4 {
                instrument.note_on(48 + 5 * k, 0.8, None);
            }
            render(&mut instrument, &settings, 3_200);
            assert_eq!(instrument.telemetry().voices_sounding, 4);
            instrument.release_all();
            let mut seconds = 0;
            while instrument.is_sounding() {
                render(&mut instrument, &settings, rate as usize);
                seconds += 1;
                assert!(seconds < 40, "model {model} at {rate}: still sounding");
            }
            assert_eq!(instrument.tail_samples(), 0);
            let (l, r) = render(&mut instrument, &settings, 6_400);
            assert!(
                l.iter().chain(&r).all(|&x| x == 0.0),
                "model {model} at {rate}"
            );
            assert_eq!(instrument.telemetry().peak, 0.0);
        }
    }
}

/// **A released note always ends**: the hidden voice with its strength past the middle stays
/// open, as on the module (a pressure CV holds it), so 30 s after its key went up the guard fades
/// it over 10 ms and stops it.
#[test]
fn a_sound_that_never_ends_is_ended_thirty_seconds_after_its_release() {
    let settings = Settings {
        model: 3,
        blow: 0.7,
        strike: 0.7,
        space: 0.5,
        slots: slots(&[(Source::Wheel, Target::Strength, 0.5)]),
        ..Settings::default()
    };
    let mut instrument = Instrument::new();
    instrument.set_wheel(1.0);
    instrument.note_on(45, 1.0, None);
    render(&mut instrument, &settings, 3_200);
    instrument.note_off(45, None);
    let mut seconds = 0;
    while sounding(&instrument) > 0 {
        render(&mut instrument, &settings, 32_000);
        seconds += 1;
        assert!(seconds <= 31, "still sounding");
    }
    assert_eq!(seconds, 31, "it sounds until the guard");
    assert_eq!(instrument.voices.voices[0].guarded, 1);
}

/// **The hidden voice's dry drone ends with its note**: at a low Space its dry path is its
/// oscillators, ungated (on the module a drone); once its gated outputs are silent after the
/// release, it fades over 10 ms, and the voice is idle without the guard.
#[test]
fn the_hidden_voices_dry_drone_ends_with_its_note() {
    let settings = Settings {
        model: 3,
        contour: 0.0,
        blow: 0.7,
        strike: 0.7,
        space: 0.0,
        ..Settings::default()
    };
    let mut instrument = Instrument::new();
    instrument.note_on(45, 1.0, None);
    let (l, _) = render(&mut instrument, &settings, 3_200);
    assert!(l.iter().any(|&x| x.abs() > 0.01), "the drone is heard");
    instrument.note_off(45, None);
    let mut seconds = 0;
    while sounding(&instrument) > 0 {
        render(&mut instrument, &settings, 32_000);
        seconds += 1;
        assert!(seconds <= 10, "still sounding");
    }
    assert_eq!(instrument.voices.voices[0].guarded, 0);
}

/// Every slot of a set, alternating ±1.
fn every_slot(set: usize, sign: f32) -> [Slot; SLOTS] {
    let mut slots = [Slot::default(); SLOTS];
    for (i, slot) in slots.iter_mut().enumerate() {
        *slot = Slot {
            source: Some(Source::ALL[(i + 7 * set) % Source::ALL.len()]),
            target: Some(Target::ALL[(i + SLOTS * set) % Target::ALL.len()]),
            amount: if i % 2 == 0 { sign } else { -sign },
        };
    }
    slots
}

/// **No NaN or infinity**, on every model, at extreme settings and with every slot at ±1: every
/// source against every target over the sets, cycling envelopes in each mode, the LFOs at their
/// fastest, every gesture at its end; then settings that are NaN and infinite, which are ignored.
#[test]
fn no_nan_or_infinity_anywhere() {
    for model in 0..MODELS {
        for (n, extreme) in [0.0f32, 1.0].into_iter().enumerate() {
            let mut settings = Settings {
                model,
                coarse_st: if n == 0 { -48.0 } else { 48.0 },
                fine_st: 1.0,
                contour: extreme,
                bow: 1.0 - extreme,
                bow_timbre: extreme,
                blow: extreme,
                flow: 1.0 - extreme,
                blow_timbre: extreme,
                strike: 1.0,
                mallet: extreme,
                strike_timbre: 1.0 - extreme,
                geometry: extreme,
                brightness: 1.0 - extreme,
                damping: extreme,
                position: extreme,
                space: 1.0,
                mode: [Mode::Poly, Mode::Legato][n],
                glide_s: 0.05,
                bend_range_st: 48.0,
                volume: 1.0,
                ..Settings::default()
            };
            for (e, env) in settings.envelopes.iter_mut().enumerate() {
                env.attack_s = if n == 0 { 0.0005 } else { 30.0 };
                env.decay_s = 0.0005;
                env.release_s = if n == 0 { 30.0 } else { 0.0005 };
                env.response = 2.0 * extreme - 1.0;
                env.cycle = true;
                env.mode = [EnvelopeMode::Note, EnvelopeMode::Gate, EnvelopeMode::Follow][e];
            }
            for (l, lfo) in settings.lfos.iter_mut().enumerate() {
                lfo.rate_hz = 50.0;
                lfo.shape = [
                    LfoShape::Square,
                    LfoShape::SampleAndHold,
                    LfoShape::SmoothRandom,
                ][l];
                lfo.retrigger = l != 1;
                lfo.fade_s = 0.01;
            }
            for set in 0..3 {
                for sign in [1.0, -1.0] {
                    settings.slots = every_slot(set, sign);
                    let mut instrument = Instrument::new();
                    instrument.set_sample_rate(44_100.0);
                    instrument.set_bend(sign);
                    instrument.set_wheel(1.0);
                    instrument.set_pressure(1.0);
                    for k in [0u8, 40, 80, 127] {
                        instrument.note_on(k, 1.0 - extreme, Some(i32::from(k)));
                        instrument.set_tuning(k, Some(i32::from(k)), 24.0 * sign);
                    }
                    let (l, r) = render(&mut instrument, &settings, 4_410);
                    assert!(
                        l.iter().chain(&r).all(|x| x.is_finite()),
                        "model {model}, extreme {extreme}, set {set}, sign {sign}"
                    );
                    instrument.release_all();
                    let mut bad = settings;
                    bad.brightness = f32::NAN;
                    bad.volume = f32::INFINITY;
                    bad.envelopes[0].attack_s = f32::NAN;
                    bad.lfos[2].rate_hz = f32::NEG_INFINITY;
                    bad.slots[3].amount = f32::NAN;
                    instrument.set_bend(f32::NAN);
                    instrument.set_tuning(127, None, f32::INFINITY);
                    instrument.note_on(64, f32::NAN, None);
                    let (l, r) = render(&mut instrument, &bad, 2_205);
                    assert!(
                        l.iter().chain(&r).all(|x| x.is_finite()),
                        "model {model}, NaN settings"
                    );
                    assert!(instrument.telemetry().peak.is_finite());
                }
            }
        }
    }
}

/// **Poly**: a note takes an idle voice; with all eight held, a ninth takes the one sounding
/// longest; a released voice is taken before a held one. A stolen voice keeps ringing into the
/// new note, its engine's gate dropped for a block and raised the next.
#[test]
fn poly_takes_free_voices_then_the_longest_sounding() {
    let settings = Settings::default();
    let mut instrument = Instrument::new();
    for k in 0..VOICES as u8 {
        instrument.note_on(60 + k, 1.0, None);
        block(&mut instrument, &settings);
    }
    block(&mut instrument, &settings);
    let notes: Vec<u8> = instrument.voices.voices.iter().map(|v| v.note).collect();
    assert_eq!(notes, (60..68).collect::<Vec<u8>>(), "each its own voice");
    assert_eq!(instrument.telemetry().voices_sounding, VOICES);
    // All held: the ninth steals the first.
    instrument.note_on(80, 1.0, None);
    instrument.apply();
    assert_eq!(instrument.voices.voices[0].note, 80);
    // A released one goes before a held one, though younger.
    instrument.note_off(65, None);
    instrument.note_on(81, 1.0, None);
    instrument.apply();
    assert_eq!(instrument.voices.voices[5].note, 81);
    // The stolen voice drops its gate for a block, then raises it: a new edge.
    block(&mut instrument, &settings);
    assert!(!played(&instrument, 0).gate);
    block(&mut instrument, &settings);
    assert!(played(&instrument, 0).gate);
}

/// An idle voice is not rendered: one note, one voice sounding; after its release, its ring and
/// the reverb's tail, nothing.
#[test]
fn idle_voices_are_not_rendered() {
    let settings = Settings {
        contour: 0.0,
        damping: 0.0,
        ..Settings::default()
    };
    let mut instrument = Instrument::new();
    render(&mut instrument, &settings, 320);
    assert_eq!(instrument.telemetry().voices_sounding, 0);
    assert!(!instrument.is_sounding());
    assert_eq!(instrument.tail_samples(), 0);
    instrument.note_on(60, 1.0, None);
    render(&mut instrument, &settings, 3_200);
    assert_eq!(instrument.telemetry().voices_sounding, 1);
    assert_eq!(instrument.tail_samples(), u32::MAX, "held");
    instrument.note_off(60, None);
    let mut taken = 0;
    while instrument.is_sounding() {
        assert_eq!(instrument.tail_samples(), u32::MAX);
        render(&mut instrument, &settings, 320);
        taken += 320;
        assert!(taken < 32_000 * 10, "ten seconds");
    }
    assert_eq!(sounding(&instrument), 0);
}

/// **Mono**: one voice; every new key retriggers it (its gate low for a block); a released key
/// returns to the one held before it without a new gate.
#[test]
fn mono_retriggers_and_returns_to_the_held_key() {
    let settings = Settings {
        mode: Mode::Mono,
        ..Settings::default()
    };
    let mut instrument = Instrument::new();
    block(&mut instrument, &settings);
    instrument.note_on(60, 1.0, None);
    block(&mut instrument, &settings);
    block(&mut instrument, &settings);
    assert!(played(&instrument, 0).gate);
    instrument.note_on(64, 1.0, None);
    block(&mut instrument, &settings);
    assert!(!played(&instrument, 0).gate, "retriggered");
    assert_eq!(played(&instrument, 0).note, 64.0);
    block(&mut instrument, &settings);
    assert!(played(&instrument, 0).gate);
    assert_eq!(sounding(&instrument), 1);
    instrument.note_off(64, None);
    block(&mut instrument, &settings);
    let p = played(&instrument, 0);
    assert_eq!((p.note, p.gate), (60.0, true), "back to 60, no new gate");
    assert_eq!(instrument.voices.held(), vec![60]);
    instrument.note_off(60, None);
    block(&mut instrument, &settings);
    block(&mut instrument, &settings);
    assert!(!played(&instrument, 0).gate, "released");
}

/// **Legato**: a key over a held one glides to it without a new gate; a key after every key is
/// up starts where it is played.
#[test]
fn legato_glides_without_a_new_gate() {
    let settings = Settings {
        mode: Mode::Legato,
        glide_s: 0.1,
        ..Settings::default()
    };
    let mut instrument = Instrument::new();
    instrument.note_on(60, 1.0, None);
    for _ in 0..20 {
        block(&mut instrument, &settings);
    }
    instrument.note_on(72, 0.3, None);
    let mut pitches = Vec::new();
    for _ in 0..550 {
        block(&mut instrument, &settings);
        let p = played(&instrument, 0);
        assert!(p.gate, "tied: no new gate");
        pitches.push(p.note);
    }
    assert!(
        pitches[0] < 61.0,
        "it starts from the held key: {}",
        pitches[0]
    );
    assert!(pitches.windows(2).all(|w| w[1] >= w[0]));
    // Within 1 % of the interval at the glide time (200 blocks), there by the end.
    assert!((pitches[199] - 72.0).abs() < 0.13, "{}", pitches[199]);
    assert_eq!(*pitches.last().unwrap(), 72.0);
    // The tie keeps the first press's velocity.
    assert_eq!(published(&instrument, 0, Source::Velocity), 0.0);
    instrument.note_off(72, None);
    instrument.note_off(60, None);
    block(&mut instrument, &settings);
    block(&mut instrument, &settings);
    instrument.note_on(48, 1.0, None);
    block(&mut instrument, &settings);
    assert_eq!(played(&instrument, 0).note, 48.0, "untied: no glide");
}

/// A note id picks its voice when both have one; with none, the key does. The per-id forms act
/// on the voice playing the id, or on nothing.
#[test]
fn note_ids_pick_their_voice() {
    let settings = Settings::default();
    let mut instrument = Instrument::new();
    instrument.note_on(60, 1.0, Some(1));
    instrument.note_on(60, 1.0, Some(2));
    instrument.note_on(62, 1.0, None);
    instrument.note_off(60, Some(2));
    instrument.apply();
    let gates: Vec<bool> = instrument.voices.voices[..3]
        .iter()
        .map(|v| v.gate)
        .collect();
    assert_eq!(gates, [true, false, true]);
    instrument.set_tuning(60, Some(1), 0.5);
    instrument.set_tuning_id(2, -0.25);
    instrument.set_tuning_id(99, 3.0);
    block(&mut instrument, &settings);
    assert_eq!(played(&instrument, 0).note, 60.5);
    assert_eq!(played(&instrument, 1).note, 59.75);
    assert_eq!(played(&instrument, 2).note, 62.0);
    instrument.note_off_id(99);
    instrument.apply();
    assert!(instrument.voices.voices[0].gate);
    instrument.note_off_id(1);
    instrument.apply();
    assert!(!instrument.voices.voices[0].gate);
    instrument.choke_id(2);
    instrument.apply();
    assert!(!instrument.voices.voices[1].active);
    instrument.note_off(62, Some(7));
    instrument.apply();
    assert!(
        !instrument.voices.voices[2].gate,
        "a voice with no id answers to its key"
    );
    instrument.choke(None);
    instrument.apply();
    assert_eq!(sounding(&instrument), 0);
    // A new note clears the tuning.
    instrument.note_on(60, 1.0, Some(1));
    block(&mut instrument, &settings);
    assert_eq!(played(&instrument, newest(&instrument)).note, 60.0);
}

/// **Each target's reach**, from one slot at full with the wheel at the top, read where the voice
/// hands it on: its patch, pitch and strength, its model, its gains.
#[test]
fn each_voice_target_reaches_what_it_says() {
    let read = |target: Target, amount: f32| {
        let settings = Settings {
            contour: 0.25,
            bow: 0.25,
            brightness: 0.25,
            geometry: 0.25,
            space: 0.25,
            slots: slots(&[(Source::Wheel, target, amount)]),
            ..Settings::default()
        };
        let mut instrument = Instrument::new();
        instrument.set_wheel(1.0);
        instrument.note_on(60, 1.0, None);
        block(&mut instrument, &settings);
        let v = &instrument.voices.voices[0];
        (v.played, v.output.to, v.model())
    };
    assert_eq!(read(Target::Pitch, 1.0).0.note, 72.0);
    assert_eq!(read(Target::Pitch, -0.5).0.note, 54.0);
    assert_eq!(read(Target::Strength, 0.5).0.strength, 1.0);
    assert_eq!(read(Target::Strength, -1.0).0.strength, 0.0);
    assert_eq!(
        read(Target::Contour, 0.5).0.patch.exciter_envelope_shape,
        0.75
    );
    assert_eq!(read(Target::Bow, -1.0).0.patch.exciter_bow_level, 0.0);
    assert_eq!(
        read(Target::Brightness, 0.5).0.patch.resonator_brightness,
        0.75
    );
    assert_eq!(
        read(Target::Brightness, 1.0).0.patch.resonator_brightness,
        0.9995,
        "the module's ceiling"
    );
    assert_eq!(read(Target::Geometry, 0.5).0.patch.resonator_geometry, 0.75);
    assert_eq!(
        read(Target::Space, 0.5).0.patch.space,
        1.25,
        "past the knob, to 2"
    );
    assert_eq!(read(Target::Space, 1.0).0.patch.space, 2.0);
    assert_eq!(read(Target::Model, 1.0).2, 3, "the hidden voice");
    assert_eq!(read(Target::Model, 0.5).2, 2, "the strings");
    assert_eq!(read(Target::Amplitude, -1.0).1, [0.0, 0.0], "silence");
    assert_eq!(read(Target::Amplitude, 1.0).1, [2.0, 2.0], "double");
    assert_eq!(read(Target::Pan, 1.0).1, [0.0, 1.0]);
    assert_eq!(read(Target::Pan, -0.5).1, [1.0, 0.5]);
}

/// An LFO's Rate reaches ±4 octaves.
#[test]
fn a_rate_reaches_four_octaves() {
    let period = |amount: f32| {
        let mut settings = Settings {
            slots: slots(&[(Source::Wheel, Target::Lfo1Rate, amount)]),
            ..Settings::default()
        };
        settings.lfos[0] = crate::LfoSettings {
            rate_hz: 2.0,
            shape: LfoShape::SawUp,
            retrigger: true,
            fade_s: 0.0,
        };
        let mut instrument = Instrument::new();
        instrument.set_wheel(1.0);
        instrument.note_on(60, 1.0, None);
        let mut was = -2.0;
        let mut wraps = Vec::new();
        for b in 0..20_000 {
            block(&mut instrument, &settings);
            let v = published(&instrument, 0, Source::Lfo1);
            if v < was {
                wraps.push(b);
            }
            was = v;
            if wraps.len() == 3 {
                break;
            }
        }
        (wraps[2] - wraps[1]) as f32 * BLOCK as f32 / 32_000.0
    };
    assert!((period(0.0) - 0.5).abs() < 0.001);
    assert!((period(1.0) - 0.5 / 16.0).abs() < 0.001);
    assert!((period(-0.5) - 0.5 * 4.0).abs() < 0.001);
}

/// **A delayed envelope** is one in Gate mode whose Trigger is another's End of rise, as on
/// MATHS: envelope 2 starts when envelope 1's attack ends, and End of cycle is its complement.
#[test]
fn a_delayed_envelope_is_gate_mode_on_end_of_rise() {
    let mut settings = Settings {
        slots: slots(&[(Source::Envelope1Rise, Target::Envelope2Trigger, 1.0)]),
        ..Settings::default()
    };
    settings.envelopes[0].attack_s = 0.02;
    settings.envelopes[1].mode = EnvelopeMode::Gate;
    let mut instrument = Instrument::new();
    instrument.note_on(60, 1.0, None);
    let mut started = None;
    let mut risen = None;
    for b in 0..200 {
        block(&mut instrument, &settings);
        let rise = published(&instrument, 0, Source::Envelope1Rise);
        assert_eq!(
            rise + published(&instrument, 0, Source::Envelope1Cycle),
            1.0
        );
        if risen.is_none() && rise == 1.0 {
            risen = Some(b);
        }
        if started.is_none() && published(&instrument, 0, Source::Envelope2) > 0.0 {
            started = Some(b);
        }
    }
    let (risen, started) = (risen.unwrap(), started.unwrap());
    assert!((39..=40).contains(&risen), "20 ms is 40 blocks: {risen}");
    assert_eq!(
        started, risen,
        "the same block: envelope 1 is computed before 2"
    );
}

/// An envelope's Trigger read from itself is one block late, never a hang: an envelope
/// triggering itself from its End of cycle is a deterministic loop.
#[test]
fn feedback_is_one_block_late_and_deterministic() {
    let mut settings = Settings {
        slots: slots(&[
            (Source::Envelope1Cycle, Target::Envelope1Trigger, 1.0),
            (Source::Envelope1, Target::Envelope1Fall, 0.01),
        ]),
        ..Settings::default()
    };
    settings.envelopes[0] = crate::EnvelopeSettings {
        attack_s: 0.01,
        decay_s: 0.02,
        sustain: 0.0,
        release_s: 0.02,
        response: 0.3,
        cycle: false,
        mode: EnvelopeMode::Gate,
    };
    let run = || {
        let mut instrument = Instrument::new();
        instrument.note_on(60, 1.0, None);
        (0..1_000)
            .map(|_| {
                block(&mut instrument, &settings);
                published(&instrument, 0, Source::Envelope1)
            })
            .collect::<Vec<f32>>()
    };
    let a = run();
    assert_eq!(a, run());
    assert!(a.iter().any(|&v| v > 0.99) && a.iter().all(|v| v.is_finite()));
}

/// **LFO Retrigger on**: each voice's own LFO starts at phase 0 with its note. **Off**: one LFO
/// for every voice. **Fade**: the depth rises from 0 after each note.
#[test]
fn the_lfos_retrigger_share_and_fade() {
    let mut settings = Settings::default();
    settings.lfos[0] = crate::LfoSettings {
        rate_hz: 3.0,
        shape: LfoShape::SawUp,
        retrigger: true,
        fade_s: 0.0,
    };
    settings.lfos[1] = crate::LfoSettings {
        rate_hz: 3.0,
        shape: LfoShape::Sine,
        retrigger: false,
        fade_s: 0.0,
    };
    settings.lfos[2] = crate::LfoSettings {
        rate_hz: 1.0,
        shape: LfoShape::Square,
        retrigger: true,
        fade_s: 0.1,
    };
    let mut instrument = Instrument::new();
    instrument.note_on(60, 1.0, None);
    block(&mut instrument, &settings);
    assert_eq!(published(&instrument, 0, Source::Lfo1), -1.0, "phase 0");
    assert_eq!(published(&instrument, 0, Source::Lfo3), 0.0, "faded out");
    for _ in 0..77 {
        block(&mut instrument, &settings);
    }
    instrument.note_on(64, 1.0, None);
    block(&mut instrument, &settings);
    assert_eq!(
        published(&instrument, 1, Source::Lfo1),
        -1.0,
        "its own, from phase 0"
    );
    assert!(published(&instrument, 0, Source::Lfo1) > -0.9);
    assert_eq!(
        published(&instrument, 0, Source::Lfo2),
        published(&instrument, 1, Source::Lfo2),
        "shared"
    );
    assert!(published(&instrument, 0, Source::Lfo2).abs() > 0.1);
    let fade0 = published(&instrument, 0, Source::Lfo3);
    assert!((fade0 - 78.0 / 200.0).abs() < 1e-3, "{fade0}");
    for _ in 0..210 {
        block(&mut instrument, &settings);
    }
    assert_eq!(published(&instrument, 1, Source::Lfo3), 1.0, "faded in");
}

/// **The host's rate is the 32 kHz stream resampled**, with the reported latency at 44.1, 48, 96
/// and 192 kHz; at 32 kHz none.
#[test]
fn the_host_rate_is_the_32_khz_stream_resampled() {
    let settings = Settings {
        bow: 0.5,
        ..Settings::default()
    };
    let mut plain = Instrument::new();
    assert_eq!(plain.latency_samples(), 0);
    plain.set_sample_rate(32_000.0);
    assert_eq!(plain.latency_samples(), 0);
    let mut resampled = Instrument::new();
    for (rate, latency) in [
        (44_100.0, 100),
        (48_000.0, 108),
        (96_000.0, 216),
        (192_000.0, 432),
    ] {
        resampled.set_sample_rate(rate);
        assert_eq!(resampled.latency_samples(), latency, "{rate}");
        assert_eq!(resampled.sample_rate(), rate);
    }
    resampled.set_sample_rate(96_000.0);
    plain.note_on(57, 1.0, None);
    resampled.note_on(57, 1.0, None);
    let (input, _) = render(&mut plain, &settings, 3_200);
    let (output, _) = render(&mut resampled, &settings, 9_600);
    let mut r = crate::resampler::Resampler::new(96_000);
    let mut n = 0;
    for &y in &output {
        while r.needs_input() {
            r.push(input[n], input[n]);
            n += 1;
        }
        assert_eq!(r.output().0, y);
    }
}

/// **A reset leaves no tail and every state as made**: a phrase rendered after a load, played into
/// its tails (a resonator's ring, the strings' delays, the hidden voice, the reverb), reset and
/// played again renders the same bits as a new instrument.
#[test]
fn a_reset_renders_what_a_load_renders() {
    for model in [0, 2, 3] {
        let settings = Settings {
            model,
            bow: 0.3,
            space: 1.0,
            slots: slots(&[(Source::Lfo1, Target::Brightness, 0.3)]),
            ..Settings::default()
        };
        let play = |instrument: &mut Instrument| {
            instrument.note_on(50, 0.7, None);
            instrument.note_on(57, 0.9, None);
            let first = render(instrument, &settings, 6_000);
            instrument.release_all();
            let tail = render(instrument, &settings, 6_000);
            (first, tail)
        };
        let fresh = play(&mut Instrument::new());
        let mut used = Instrument::new();
        used.set_sample_rate(44_100.0);
        used.note_on(70, 1.0, None);
        render(
            &mut used,
            &Settings {
                model: 1,
                ..settings
            },
            3_000,
        );
        used.reset();
        used.set_sample_rate(32_000.0);
        assert!(!used.is_sounding());
        let (l, _) = render(&mut used, &settings, 320);
        assert!(l.iter().all(|&x| x == 0.0), "no tail");
        used.reset();
        assert_eq!(play(&mut used), fresh, "model {model}");
    }
}

/// Velocity and Random are drawn per note; Key is the glided note.
#[test]
fn key_is_the_glided_note() {
    let settings = Settings {
        glide_s: 0.05,
        ..Settings::default()
    };
    let mut instrument = Instrument::new();
    instrument.note_on(60, 1.0, None);
    block(&mut instrument, &settings);
    assert_eq!(published(&instrument, 0, Source::Key), 0.0);
    instrument.note_on(72, 0.5, None);
    block(&mut instrument, &settings);
    let key = published(&instrument, 1, Source::Key);
    assert!(key < 0.01, "it glides from 60: {key}");
    for _ in 0..400 {
        block(&mut instrument, &settings);
    }
    assert_eq!(published(&instrument, 1, Source::Key), 0.2);
    assert_eq!(published(&instrument, 1, Source::Velocity), -0.5);
    assert_eq!(published(&instrument, 0, Source::Velocity), 0.0);
}

/// The Contour source is the voice's exciter envelope: at the percussive end it rises with the
/// note and falls back to rest while the key is still held.
#[test]
fn contour_is_the_exciter_envelope() {
    let settings = Settings {
        contour: 0.1,
        ..Settings::default()
    };
    let mut instrument = Instrument::new();
    instrument.note_on(60, 1.0, None);
    let mut peak = 0.0f32;
    for _ in 0..4_000 {
        block(&mut instrument, &settings);
        peak = peak.max(published(&instrument, 0, Source::Contour));
    }
    let after = published(&instrument, 0, Source::Contour);
    assert!(peak > 0.9 && after < 0.01, "peak {peak}, then {after}");
}

/// The peak is the loudest host sample of the last render.
#[test]
fn the_peak_is_the_loudest_sample() {
    let settings = Settings::default();
    let mut instrument = Instrument::new();
    instrument.note_on(60, 1.0, None);
    let (l, r) = render(&mut instrument, &settings, 3_200);
    let peak = l.iter().chain(&r).fold(0.0f32, |p, x| p.max(x.abs()));
    assert!(peak > 0.0);
    assert_eq!(instrument.telemetry().peak, peak);
}
