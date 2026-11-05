//! **The factory set's levels.** Every factory file is loaded through the preset system as a host
//! loads it (`mxm_preset::clap::load_file`, into the parameters), the DSP's `Settings` filled from
//! them as the plugin's `process` fills them (once per host buffer, the smoothed signals stepped
//! per piece of at most `MAX_BLOCK` samples), and played through `mxm_modal_synth_dsp::Instrument`
//! at 48 kHz at a host tempo of 120 bpm, as its category is played: a chord for a pad, strings or
//! keys, a phrase for a bass, a lead, a pluck, eighths for percussion, a held note for an effect
//! or a drone; and its first note on until it falls silent, which every sound must within
//! [`ENDS_WITHIN`] of its key going up, never by the instrument's 30 s guard.
//!
//! **The measure is ITU-R BS.1770's momentary loudness at its loudest** (LUFS, [`loudness`]): the
//! power of the loudest 400 ms of the sounding part, K-weighted as a listener weighs it. **The
//! window**: every sound within [`WINDOW_LU`] of the set's median, none silent, and no sample past
//! full scale. A sound out of it is fixed in its design, never by moving Volume, which factory
//! content leaves at Init (`preset::tests::factory_content_does_not_move_volume`).
//!
//! ```text
//! cargo test -p mxm-modal-synth --test factory_levels -- --nocapture
//! ```
//!
//! prints each sound's loudness, peak and how long it rang on.

use mxm_modal_synth::params::ModalSynthParams;
use mxm_modal_synth::preset::factory_folder;
use mxm_modal_synth_dsp::Instrument;
use mxm_preset::{Category, Preset};
use nice_plug::params::internals::ParamPtr;
use nice_plug::prelude::{GuiContextInner, ParamSetter, Params as _, PluginApi, PluginState};

const RATE: f32 = 48_000.0;
/// The host's buffer: the patch is read once per buffer, as `process` reads it once per call.
const HOST_BUFFER: usize = 512;
const VELOCITY: f32 = 0.8;
/// The host's tempo, which a synced LFO follows.
const TEMPO: f64 = 120.0;
/// Every sound is silent this long after its last key is let go.
const ENDS_WITHIN: f32 = 20.0;
/// How far a sound may sit from the set's median, LU.
const WINDOW_LU: f64 = 5.0;
/// Quieter than this, at its loudest, is silent.
const SILENT_LUFS: f64 = -45.0;

/// A host that applies every edit it is sent, so a load lands in the parameters.
struct Applying;

impl GuiContextInner for Applying {
    fn request_restart(&self) {}
    fn plugin_api(&self) -> PluginApi {
        PluginApi::Clap
    }
    unsafe fn raw_begin_set_parameter(&self, _param: ParamPtr) {}
    unsafe fn raw_set_parameter_normalized(&self, param: ParamPtr, normalized: f32) {
        // SAFETY: the parameters outlive the load, and nothing else touches them meanwhile.
        unsafe {
            param._internal_set_normalized_value(normalized);
        }
    }
    unsafe fn raw_end_set_parameter(&self, _param: ParamPtr) {}
    fn get_state(&self) -> PluginState {
        PluginState {
            version: String::new(),
            params: Default::default(),
            fields: Default::default(),
        }
    }
    fn set_state(&self, _state: PluginState) {}
}

/// One note: its key and its seconds on and off.
#[derive(Clone, Copy)]
struct Note {
    key: u8,
    on: f32,
    off: f32,
}

const fn note(key: u8, on: f32, off: f32) -> Note {
    Note { key, on, off }
}

/// **What a category is played with**, and the render's length in seconds. A sound that plays a
/// chord from one key (the strings, a chord chosen) is played one key at a time where its category
/// would take a chord.
fn phrase(category: Category, one_key_chords: bool) -> (Vec<Note>, f32) {
    let chord = |keys: &[u8], on: f32, off: f32| -> Vec<Note> {
        let keys = if one_key_chords { &keys[..1] } else { keys };
        keys.iter().map(|&k| note(k, on, off)).collect()
    };
    match category {
        Category::Bass => (
            vec![
                note(36, 0.0, 0.3),
                note(36, 0.4, 0.7),
                note(43, 0.8, 1.1),
                note(39, 1.2, 1.5),
            ],
            2.5,
        ),
        Category::Lead | Category::Brass => (
            // Each note over the last, so a legato sound ties them.
            vec![note(60, 0.0, 0.8), note(67, 0.7, 1.5), note(72, 1.4, 2.4)],
            3.4,
        ),
        Category::Pad | Category::Strings => (chord(&[48, 55, 60, 64], 0.0, 3.0), 5.0),
        Category::Keys => {
            let mut notes = chord(&[60, 64, 67], 0.0, 1.2);
            notes.extend(chord(&[57, 60, 64], 1.25, 2.45));
            (notes, 4.0)
        }
        Category::Pluck => (
            [60, 64, 67, 72, 67, 64]
                .iter()
                .enumerate()
                .map(|(i, &k)| note(k, 0.25 * i as f32, 0.25 * i as f32 + 0.15))
                .collect(),
            3.0,
        ),
        // Eighths at 120 bpm.
        Category::Percussion => (
            (0..8)
                .map(|i| note(48, 0.25 * i as f32, 0.25 * i as f32 + 0.1))
                .collect(),
            3.0,
        ),
        Category::Sequence => (
            (0..8)
                .map(|i| note(60, 0.15 * i as f32, 0.15 * i as f32 + 0.1))
                .collect(),
            2.5,
        ),
        Category::Drone => (vec![note(48, 0.0, 4.0)], 5.0),
        _ => (vec![note(60, 0.0, 3.0)], 5.0),
    }
}

/// The sample a time falls on, on a piece boundary.
fn sample(seconds: f32) -> usize {
    let block = mxm_modal_synth::MAX_BLOCK;
    ((seconds * RATE) as usize / block) * block
}

/// The parameters as `file` sets them, loaded as a host loads it, their smoothers at rest.
fn loaded(file: &str) -> ModalSynthParams {
    let params = ModalSynthParams::default();
    let setter = ParamSetter::new(&Applying);
    let path = factory_folder().join(file);
    mxm_preset::clap::load_file(&params, &path, &setter).unwrap_or_else(|e| panic!("{file}: {e}"));
    for (_, ptr, _) in params.param_map() {
        // SAFETY: the parameters are this test's alone.
        unsafe { ptr._internal_update_smoother(RATE, true) };
    }
    params
}

/// `notes` played through the instrument as the plugin plays them, `seconds` long, or with
/// `ringing` on until nothing sounds, for at most [`ENDS_WITHIN`] after the last key is let go;
/// with how long after it the instrument fell silent, if it did.
fn render(
    params: &ModalSynthParams,
    notes: &[Note],
    seconds: f32,
    ringing: bool,
) -> ([Vec<f32>; 2], Option<f32>) {
    let last_off = notes.iter().map(|n| sample(n.off)).max().unwrap_or(0);
    let limit = if ringing {
        sample(seconds).max(last_off + sample(ENDS_WITHIN))
    } else {
        sample(seconds)
    };
    let mut left = vec![0.0f32; limit];
    let mut right = vec![0.0f32; limit];
    let mut instrument = Instrument::new();
    instrument.set_sample_rate(RATE);
    instrument.reset();
    let offs: Vec<(usize, u8)> = notes.iter().map(|n| (sample(n.off), n.key)).collect();
    let ons: Vec<(usize, u8)> = notes.iter().map(|n| (sample(n.on), n.key)).collect();
    let mut edges: Vec<usize> = offs.iter().chain(&ons).map(|&(s, _)| s).collect();
    edges.sort_unstable();
    let mut buffer = 0;
    while buffer < limit {
        if ringing && buffer > last_off && !instrument.is_sounding() {
            let total = sample(seconds).max(buffer);
            left.truncate(total);
            right.truncate(total);
            return ([left, right], Some((buffer - last_off) as f32 / RATE));
        }
        let mut settings = params.settings(Some(TEMPO));
        let buffer_end = (buffer + HOST_BUFFER).min(limit);
        let mut start = buffer;
        while start < buffer_end {
            for &(at, key) in &offs {
                if at == start {
                    instrument.note_off(key, None);
                }
            }
            for &(at, key) in &ons {
                if at == start {
                    instrument.note_on(key, VELOCITY, None);
                }
            }
            let mut end = (start + mxm_modal_synth::MAX_BLOCK).min(buffer_end);
            if let Some(&edge) = edges.iter().find(|&&e| e > start) {
                end = end.min(edge);
            }
            params.advance(&mut settings, (end - start) as u32);
            instrument.render(&settings, &mut left[start..end], &mut right[start..end]);
            start = end;
        }
        buffer = buffer_end;
    }
    ([left, right], None)
}

/// A biquad, direct form I.
struct Biquad {
    b: [f64; 3],
    a: [f64; 2],
    x: [f64; 2],
    y: [f64; 2],
}

impl Biquad {
    const fn new(b: [f64; 3], a: [f64; 2]) -> Self {
        Self {
            b,
            a,
            x: [0.0; 2],
            y: [0.0; 2],
        }
    }

    fn process(&mut self, x: f64) -> f64 {
        let y = self.b[0] * x + self.b[1] * self.x[0] + self.b[2] * self.x[1]
            - self.a[0] * self.y[0]
            - self.a[1] * self.y[1];
        self.x = [x, self.x[0]];
        self.y = [y, self.y[0]];
        y
    }
}

/// One channel K-weighted: BS.1770's shelf and high-pass, its coefficients at 48 kHz.
fn k_weighted(channel: &[f32]) -> Vec<f64> {
    let mut shelf = Biquad::new(
        [
            1.535_124_859_586_97,
            -2.691_696_189_406_38,
            1.198_392_810_852_85,
        ],
        [-1.690_659_293_182_41, 0.732_480_774_215_85],
    );
    let mut high_pass = Biquad::new(
        [1.0, -2.0, 1.0],
        [-1.990_047_454_833_98, 0.990_072_250_366_21],
    );
    channel
        .iter()
        .map(|&x| high_pass.process(shelf.process(f64::from(x))))
        .collect()
}

/// **A sound's loudness as a mixing engineer's meter reads it**: BS.1770's momentary loudness,
/// LUFS (both channels K-weighted, their mean squares over 400 ms summed) at its loudest, read every
/// 10 ms, as mxm-model-drums balances its kits: a pad's held chord and a drum's hit are each heard
/// at their loudest, and a decay's silence counts for nothing.
fn loudness([left, right]: &[Vec<f32>; 2]) -> f64 {
    const WINDOW: usize = 19_200;
    const HOP: usize = 480;
    let mut sums = vec![0.0f64; left.len() + 1];
    for (i, (l, r)) in k_weighted(left).iter().zip(k_weighted(right)).enumerate() {
        sums[i + 1] = sums[i] + l * l + r * r;
    }
    let loudest = (0..=left.len().saturating_sub(WINDOW))
        .step_by(HOP)
        .map(|start| (sums[(start + WINDOW).min(left.len())] - sums[start]) / WINDOW as f64)
        .fold(0.0, f64::max);
    -0.691 + 10.0 * loudest.log10()
}

/// Whether the loaded sound plays a chord from one key: the strings, whose chord is
/// `round(10 × Geometry)`, its first the octaves, one note.
fn plays_chords_from_one_key(params: &ModalSynthParams) -> bool {
    params.model.value() == 2 && (params.geometry.value() * 10.0).round() >= 1.0
}

/// **Every factory sound is heard, never past full scale, and within the set's loudness window.**
#[test]
fn every_factory_sound_sits_in_the_sets_loudness_window() {
    let files = mxm_preset::read_factory_dir(&factory_folder());
    assert!(!files.is_empty(), "no factory files were read");
    let mut measured = Vec::new();
    for (file, json) in &files {
        let preset = Preset::parse(json, mxm_modal_synth::CLAP_ID).unwrap();
        let params = loaded(file);
        let (notes, seconds) = phrase(preset.category, plays_chords_from_one_key(&params));
        let (out, _) = render(&params, &notes, seconds, false);
        // Its voices are independent: one note of the phrase shows how long the sound rings on.
        let (_, ended) = render(&params, &notes[..1], seconds, true);
        let peak = out.iter().flatten().fold(0.0f32, |peak, &x| {
            if x.is_finite() {
                peak.max(x.abs())
            } else {
                f32::INFINITY
            }
        });
        let lufs = loudness(&out);
        println!(
            "{:20} {:10} {:7.1} LUFS  peak {:6.1} dBFS  silent {:4.1} s after",
            preset.name,
            preset.category.label(),
            lufs,
            20.0 * peak.log10(),
            ended.unwrap_or(f32::INFINITY)
        );
        assert!(peak <= 1.0, "{file} reaches {peak} (full scale is 1)");
        assert!(lufs > SILENT_LUFS, "{file} is silent: {lufs:.1} LUFS");
        assert!(
            ended.is_some(),
            "{file} still sounds {ENDS_WITHIN} s after its keys are let go"
        );
        measured.push((file.clone(), lufs));
    }
    let mut sorted: Vec<f64> = measured.iter().map(|(_, l)| *l).collect();
    sorted.sort_by(f64::total_cmp);
    let median = sorted[sorted.len() / 2];
    println!("median {median:.1} LUFS, window ±{WINDOW_LU} LU");
    let outside: Vec<String> = measured
        .iter()
        .filter(|(_, l)| (l - median).abs() > WINDOW_LU)
        .map(|(file, l)| format!("{file}: {:+.1} LU", l - median))
        .collect();
    assert!(
        outside.is_empty(),
        "outside ±{WINDOW_LU} LU of the median ({median:.1} LUFS): {outside:?}"
    );
}
