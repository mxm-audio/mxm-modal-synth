//! The 105 host parameters (ops' `plans/plan-elements.md` §5.5): the voice's 21, each
//! envelope's 7, each LFO's 5 and the sixteen route slots' 48 ([`crate::routes`]).
//!
//! **Every parameter is frozen** (E5, 2026-11-05): its `#[id]`, kind, range and default, and every
//! list's values in their order (Model's models, the voice modes, the envelope modes, the LFO
//! shapes, and the route slots' sources and targets), as `frozen-parameters.txt` holds them
//! (`tests::the_parameters_are_the_frozen_tables`).
//!
//! **Smooth signals, not coefficients.** What is added to or multiplies the sound is smoothed;
//! times, rates, modes and switches configure the state machines and are not. [`ModalSynthParams::
//! settings`] reads the patch once per process call; [`ModalSynthParams::advance`] moves the smoothed
//! signals once per internal block.
//!
//! **The Init rule** (`docs/plugin-conventions.md` in mxm-kit): every amount starts at zero, every
//! configuration somewhere musically useful, and Init is these defaults. The amounts are
//! [`AMOUNT_IDS`]; `init_is_every_amount_at_zero_and_nothing_routed` holds the rule.

use crate::routes::{ROUTE_IDS, Routes};
use mxm_modal_synth_dsp::{
    ENVELOPES, EnvelopeMode, EnvelopeSettings, LFOS, LfoSettings, LfoShape, Mode, SLOTS, Settings,
};
use mxm_params::text::{self, StringToValue, ValueToString};
use nice_plug::prelude::*;
use std::sync::{Arc, RwLock};

/// The models, as the voice numbers them: the module's three resonators, chosen on it by a
/// hidden gesture, and its hidden second voice. The instrument's own names, which Model reads.
pub const MODEL_NAMES: [&str; 4] = ["Modal", "String", "Strings", "Dark FM"];

/// Init's model: the modal resonator, as `Part::Init` sets it.
pub const INIT_MODEL: i32 = 0;

const SIGNAL_SMOOTHING_MS: f32 = 10.0;
const PITCH_SMOOTHING_MS: f32 = 20.0;
const GAIN_SMOOTHING_MS: f32 = 20.0;

/// Coarse's reach either side of the note, in semitones: three octaves (the module's Coarse
/// spans five).
pub const COARSE_REACH_ST: i32 = 36;
/// Fine's reach either side, in semitones: the module's (`cv_scaler.cc`: ±3.3 × 2 / 3.3).
pub const FINE_REACH_ST: f32 = 2.0;
pub const BEND_RANGE_MAX_ST: i32 = 24;
pub const GLIDE_MAX_S: f32 = 5.0;
/// An envelope's times, each on its own parameter: the DSP's range (its `AGENTS.md`).
pub const ENVELOPE_MIN_S: f32 = 0.0005;
pub const ENVELOPE_MAX_S: f32 = 30.0;
/// An LFO's free rate: the DSP's range.
pub const LFO_MIN_HZ: f32 = 0.01;
pub const LFO_MAX_HZ: f32 = 50.0;
pub const FADE_MAX_S: f32 = 10.0;
pub const VOLUME_MIN_DB: f32 = -60.0;
pub const VOLUME_MAX_DB: f32 = 0.0;
pub const VOLUME_INIT_DB: f32 = -3.0;

/// **Every LFO's tempo sync** (`docs/plugin-conventions.md`, *Tempo sync is one button*): the
/// collection's LFO ladder, 1/32 to four bars, the top of the Rate the fastest.
pub const LFO_SYNC: mxm_tempo::Ladder =
    mxm_tempo::Ladder::new(mxm_tempo::Span::LFO, mxm_tempo::Direction::Rate);

/// How the voices take notes.
#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoiceMode {
    #[id = "poly"]
    #[name = "Poly"]
    Poly,
    #[id = "mono"]
    #[name = "Mono"]
    Mono,
    #[id = "legato"]
    #[name = "Legato"]
    Legato,
}

impl From<VoiceMode> for Mode {
    fn from(mode: VoiceMode) -> Self {
        match mode {
            VoiceMode::Poly => Self::Poly,
            VoiceMode::Mono => Self::Mono,
            VoiceMode::Legato => Self::Legato,
        }
    }
}

/// What drives an envelope.
#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnvelopeModeChoice {
    #[id = "note"]
    #[name = "Note"]
    Note,
    #[id = "gate"]
    #[name = "Gate"]
    Gate,
    #[id = "follow"]
    #[name = "Follow"]
    Follow,
}

impl From<EnvelopeModeChoice> for EnvelopeMode {
    fn from(mode: EnvelopeModeChoice) -> Self {
        match mode {
            EnvelopeModeChoice::Note => Self::Note,
            EnvelopeModeChoice::Gate => Self::Gate,
            EnvelopeModeChoice::Follow => Self::Follow,
        }
    }
}

/// An LFO's shape.
#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum LfoShapeChoice {
    #[id = "sine"]
    #[name = "Sine"]
    Sine,
    #[id = "triangle"]
    #[name = "Triangle"]
    Triangle,
    #[id = "saw_up"]
    #[name = "Saw up"]
    SawUp,
    #[id = "saw_down"]
    #[name = "Saw down"]
    SawDown,
    #[id = "square"]
    #[name = "Square"]
    Square,
    #[id = "sample_and_hold"]
    #[name = "Sample and hold"]
    SampleAndHold,
    #[id = "smooth_random"]
    #[name = "Smooth random"]
    SmoothRandom,
}

impl From<LfoShapeChoice> for LfoShape {
    fn from(shape: LfoShapeChoice) -> Self {
        match shape {
            LfoShapeChoice::Sine => Self::Sine,
            LfoShapeChoice::Triangle => Self::Triangle,
            LfoShapeChoice::SawUp => Self::SawUp,
            LfoShapeChoice::SawDown => Self::SawDown,
            LfoShapeChoice::Square => Self::Square,
            LfoShapeChoice::SampleAndHold => Self::SampleAndHold,
            LfoShapeChoice::SmoothRandom => Self::SmoothRandom,
        }
    }
}

/// Tenths of a millisecond below a second, hundredths of a second from it — **the unit chosen from
/// the rounded tenths**, so the host's round trip is idempotent at the switch: chosen from the raw
/// value, 0.99995 s would print `1000.0 ms`, which parses to one second and prints `1.00 s`. Tenths,
/// because an envelope's fastest time is half a millisecond. Reads back through
/// `mxm_params::text::parse_time`. mxm-mono-pr1's `v2s_time` is the same reading: the second copy,
/// due to move into `mxm_params::text`.
fn time_text() -> ValueToString {
    Arc::new(|seconds| {
        let ms = seconds * 1_000.0;
        if (ms * 10.0).round() >= 10_000.0 {
            format!("{seconds:.2} s")
        } else {
            format!("{ms:.1} ms")
        }
    })
}

/// Semitones as typed: `+12`, `-0.5 st`, `7st`.
fn parse_semitones() -> StringToValue {
    Arc::new(|text| {
        text.trim()
            .trim_end_matches("st")
            .trim()
            .parse::<f32>()
            .ok()
    })
}

/// A value of 0 to 1, read as a percentage.
fn unit(name: impl Into<String>, default: f32) -> FloatParam {
    FloatParam::new(name, default, FloatRange::Linear { min: 0.0, max: 1.0 })
        .with_value_to_string(text::percent())
        .with_string_to_value(text::parse_percent())
}

/// A signal of 0 to 1, smoothed.
fn signal(name: impl Into<String>, default: f32) -> FloatParam {
    unit(name, default).with_smoother(SmoothingStyle::Linear(SIGNAL_SMOOTHING_MS))
}

/// A time from `min` to `max` seconds, skewed toward the short end, unsmoothed: it configures a
/// state machine.
fn seconds(name: impl Into<String>, default: f32, min: f32, max: f32) -> FloatParam {
    FloatParam::new(
        name,
        default,
        FloatRange::Skewed {
            min,
            max,
            factor: FloatRange::skew_factor(-2.0),
        },
    )
    .with_value_to_string(time_text())
    .with_string_to_value(text::parse_time())
}

fn lfo_rate_range() -> FloatRange {
    FloatRange::Skewed {
        min: LFO_MIN_HZ,
        max: LFO_MAX_HZ,
        factor: FloatRange::skew_factor(-2.0),
    }
}

/// An LFO's Rate, free in hertz; synced, its position picks a division on [`LFO_SYNC`]. A typed
/// division selects the position that picks it, as the drum machine's.
fn lfo_rate(name: String, default: f32) -> FloatParam {
    let free_parser = formatters::s2v_f32_hz_then_khz();
    FloatParam::new(name, default, lfo_rate_range())
        .with_unit(" Hz")
        .with_value_to_string(formatters::v2s_f32_rounded(2))
        .with_string_to_value(Arc::new(move |text| {
            match mxm_tempo::Division::parse(text).filter(|d| LFO_SYNC.span.contains(*d)) {
                Some(division) => Some(lfo_rate_range().unnormalize(LFO_SYNC.position(division))),
                None => free_parser(text),
            }
        }))
}

/// One envelope's seven: an ADSR with MATHS' functions (the plan's §5.4).
#[derive(Params)]
pub struct EnvelopeParams {
    #[id = "attack"]
    pub attack: FloatParam,
    #[id = "decay"]
    pub decay: FloatParam,
    #[id = "sustain"]
    pub sustain: FloatParam,
    #[id = "release"]
    pub release: FloatParam,
    /// One curve for every segment: −1 logarithmic, 0 linear, +1 exponential.
    #[id = "response"]
    pub response: FloatParam,
    #[id = "cycle"]
    pub cycle: BoolParam,
    #[id = "mode"]
    pub mode: EnumParam<EnvelopeModeChoice>,
}

impl EnvelopeParams {
    fn new(n: usize) -> Self {
        let name = |what: &str| format!("Envelope {n} {what}");
        Self {
            attack: seconds(name("attack"), 0.005, ENVELOPE_MIN_S, ENVELOPE_MAX_S),
            decay: seconds(name("decay"), 0.3, ENVELOPE_MIN_S, ENVELOPE_MAX_S),
            sustain: signal(name("sustain"), 0.7),
            release: seconds(name("release"), 0.3, ENVELOPE_MIN_S, ENVELOPE_MAX_S),
            response: FloatParam::new(
                name("response"),
                0.0,
                FloatRange::Linear {
                    min: -1.0,
                    max: 1.0,
                },
            )
            .with_value_to_string(text::percent())
            .with_string_to_value(text::parse_percent()),
            cycle: BoolParam::new(name("cycle"), false),
            mode: EnumParam::new(name("mode"), EnvelopeModeChoice::Note),
        }
    }

    /// The envelope as the DSP takes it, the patch as set.
    fn settings(&self) -> EnvelopeSettings {
        EnvelopeSettings {
            attack_s: self.attack.value(),
            decay_s: self.decay.value(),
            sustain: self.sustain.value(),
            release_s: self.release.value(),
            response: self.response.value(),
            cycle: self.cycle.value(),
            mode: self.mode.value().into(),
        }
    }
}

/// One LFO's five.
#[derive(Params)]
pub struct LfoParams {
    #[id = "rate"]
    pub rate: FloatParam,
    #[id = "shape"]
    pub shape: EnumParam<LfoShapeChoice>,
    /// The Rate's tempo sync: its position picks a division of the host's tempo.
    #[id = "sync"]
    pub sync: BoolParam,
    /// Restart with each note, per voice; off, one LFO for all voices, free-running.
    #[id = "retrigger"]
    pub retrigger: BoolParam,
    /// A fade-in after each note: a time, and still an amount (how much fade there is), so zero at
    /// Init, as glide is.
    #[id = "fade"]
    pub fade: FloatParam,
}

impl LfoParams {
    fn new(n: usize, rate_hz: f32) -> Self {
        let name = |what: &str| format!("LFO {n} {what}");
        Self {
            rate: lfo_rate(name("rate"), rate_hz),
            shape: EnumParam::new(name("shape"), LfoShapeChoice::Sine),
            sync: BoolParam::new(name("sync"), false),
            retrigger: BoolParam::new(name("retrigger"), false),
            fade: seconds(name("fade"), 0.0, 0.0, FADE_MAX_S),
        }
    }

    /// The Rate while its sync follows the host, or `None` for its free value: the modulated
    /// position picks a division on [`LFO_SYNC`]. Missing or invalid tempo is the free value.
    #[must_use]
    pub fn synced_rate(&self, tempo: Option<f64>) -> Option<f32> {
        LFO_SYNC
            .resolve(
                self.sync.value(),
                tempo,
                self.rate.modulated_normalized_value(),
                f64::from(self.rate.preview_plain(0.0)),
                f64::from(self.rate.preview_plain(1.0)),
            )
            .map(|hz| hz as f32)
    }

    fn settings(&self, tempo: Option<f64>) -> LfoSettings {
        LfoSettings {
            rate_hz: self.synced_rate(tempo).unwrap_or(self.rate.value()),
            shape: self.shape.value().into(),
            retrigger: self.retrigger.value(),
            fade_s: self.fade.value(),
        }
    }
}

#[derive(Params)]
pub struct ModalSynthParams {
    // The voice's twenty-one: the module's knobs, then the instrument's.
    /// The model, 0–3 in [`MODEL_NAMES`]' order.
    #[id = "model"]
    pub model: IntParam,
    #[id = "coarse"]
    pub coarse: IntParam,
    #[id = "fine"]
    pub fine: FloatParam,
    /// The exciters: Contour, the envelope over them; the bow, the blow and the strike.
    #[id = "contour"]
    pub contour: FloatParam,
    #[id = "bow"]
    pub bow: FloatParam,
    #[id = "bow_timbre"]
    pub bow_timbre: FloatParam,
    #[id = "blow"]
    pub blow: FloatParam,
    #[id = "flow"]
    pub flow: FloatParam,
    #[id = "blow_timbre"]
    pub blow_timbre: FloatParam,
    #[id = "strike"]
    pub strike: FloatParam,
    #[id = "mallet"]
    pub mallet: FloatParam,
    #[id = "strike_timbre"]
    pub strike_timbre: FloatParam,
    /// The resonator.
    #[id = "geometry"]
    pub geometry: FloatParam,
    #[id = "brightness"]
    pub brightness: FloatParam,
    #[id = "damping"]
    pub damping: FloatParam,
    #[id = "position"]
    pub position: FloatParam,
    /// The dry signal, the stereo spread, then the reverb.
    #[id = "space"]
    pub space: FloatParam,
    #[id = "mode"]
    pub mode: EnumParam<VoiceMode>,
    #[id = "glide"]
    pub glide: FloatParam,
    #[id = "bend_range"]
    pub bend_range: IntParam,
    /// Linear gain, read in decibels.
    #[id = "volume"]
    pub volume: FloatParam,

    #[nested(id_prefix = "env1", group = "Envelope 1")]
    pub env1: EnvelopeParams,
    #[nested(id_prefix = "env2", group = "Envelope 2")]
    pub env2: EnvelopeParams,
    #[nested(id_prefix = "env3", group = "Envelope 3")]
    pub env3: EnvelopeParams,

    #[nested(id_prefix = "lfo1", group = "LFO 1")]
    pub lfo1: LfoParams,
    #[nested(id_prefix = "lfo2", group = "LFO 2")]
    pub lfo2: LfoParams,
    #[nested(id_prefix = "lfo3", group = "LFO 3")]
    pub lfo3: LfoParams,

    /// The sixteen route slots, `route<r>_{source,target,amount}`.
    #[nested(group = "Routes")]
    pub routes: Routes,

    /// Loaded preset identity and canonical baseline, persisted with plugin state.
    #[persist = "preset"]
    pub preset: RwLock<mxm_preset::PresetIdentity>,
}

/// The voice's twenty-one ids, in declaration order.
pub const VOICE_IDS: [&str; 21] = [
    "model",
    "coarse",
    "fine",
    "contour",
    "bow",
    "bow_timbre",
    "blow",
    "flow",
    "blow_timbre",
    "strike",
    "mallet",
    "strike_timbre",
    "geometry",
    "brightness",
    "damping",
    "position",
    "space",
    "mode",
    "glide",
    "bend_range",
    "volume",
];

macro_rules! envelope_ids {
    ($($n:literal),*) => {
        [$([
            concat!("env", $n, "_attack"),
            concat!("env", $n, "_decay"),
            concat!("env", $n, "_sustain"),
            concat!("env", $n, "_release"),
            concat!("env", $n, "_response"),
            concat!("env", $n, "_cycle"),
            concat!("env", $n, "_mode"),
        ]),*]
    };
}

macro_rules! lfo_ids {
    ($($n:literal),*) => {
        [$([
            concat!("lfo", $n, "_rate"),
            concat!("lfo", $n, "_shape"),
            concat!("lfo", $n, "_sync"),
            concat!("lfo", $n, "_retrigger"),
            concat!("lfo", $n, "_fade"),
        ]),*]
    };
}

/// Each envelope's seven ids, in declaration order.
pub const ENVELOPE_IDS: [[&str; 7]; ENVELOPES] = envelope_ids!(1, 2, 3);
/// Each LFO's five ids, in declaration order.
pub const LFO_IDS: [[&str; 5]; LFOS] = lfo_ids!(1, 2, 3);

/// How many parameters the host holds: the plan's §5.5.
pub const PARAMETER_COUNT: usize =
    VOICE_IDS.len() + ENVELOPES * 7 + LFOS * 5 + SLOTS * ROUTE_IDS[0].len();

/// **The amounts**, every one zero at Init: how much of an effect there is. Glide (a time, and
/// still an amount: how much portamento there is), each LFO's Fade and every route's amount.
/// Envelope times and LFO rates are configuration: the envelopes always run. The exciters'
/// levels are the sound's sources, not amounts: Init is `Part::Init`'s patch, struck.
pub const AMOUNT_IDS: [&str; 20] = [
    "glide",
    "lfo1_fade",
    "lfo2_fade",
    "lfo3_fade",
    "route1_amount",
    "route2_amount",
    "route3_amount",
    "route4_amount",
    "route5_amount",
    "route6_amount",
    "route7_amount",
    "route8_amount",
    "route9_amount",
    "route10_amount",
    "route11_amount",
    "route12_amount",
    "route13_amount",
    "route14_amount",
    "route15_amount",
    "route16_amount",
];

impl Default for ModalSynthParams {
    fn default() -> Self {
        Self {
            model: IntParam::new("Model", INIT_MODEL, IntRange::Linear { min: 0, max: 3 })
                .with_value_to_string(Arc::new(|v| MODEL_NAMES[v.clamp(0, 3) as usize].to_owned()))
                // A model's name, or its number.
                .with_string_to_value(Arc::new(|s| {
                    let s = s.trim();
                    MODEL_NAMES
                        .iter()
                        .position(|name| name.eq_ignore_ascii_case(s))
                        .map(|i| i as i32)
                        .or_else(|| s.parse().ok())
                })),
            coarse: IntParam::new(
                "Coarse",
                0,
                IntRange::Linear {
                    min: -COARSE_REACH_ST,
                    max: COARSE_REACH_ST,
                },
            )
            .with_unit(" st")
            .with_value_to_string(Arc::new(|v| {
                if v == 0 {
                    "0".to_owned()
                } else {
                    format!("{v:+}")
                }
            }))
            .with_string_to_value(Arc::new(|s| {
                parse_semitones()(s).map(|st| st.round() as i32)
            })),
            fine: FloatParam::new(
                "Fine",
                0.0,
                FloatRange::Linear {
                    min: -FINE_REACH_ST,
                    max: FINE_REACH_ST,
                },
            )
            .with_smoother(SmoothingStyle::Linear(PITCH_SMOOTHING_MS))
            .with_unit(" st")
            .with_value_to_string(Arc::new(|v| text::signed(v, 2)))
            .with_string_to_value(parse_semitones()),
            // `Part::Init`'s patch.
            contour: signal("Contour", 1.0),
            bow: signal("Bow", 0.0),
            bow_timbre: signal("Bow timbre", 0.5),
            blow: signal("Blow", 0.0),
            flow: signal("Flow", 0.5),
            blow_timbre: signal("Blow timbre", 0.5),
            strike: signal("Strike", 0.8),
            mallet: signal("Mallet", 0.5),
            strike_timbre: signal("Strike timbre", 0.5),
            geometry: signal("Geometry", 0.2),
            brightness: signal("Brightness", 0.5),
            damping: signal("Damping", 0.25),
            position: signal("Position", 0.3),
            space: signal("Space", 0.5),
            mode: EnumParam::new("Voice mode", VoiceMode::Poly),
            glide: seconds("Glide", 0.0, 0.0, GLIDE_MAX_S),
            bend_range: IntParam::new(
                "Bend range",
                2,
                IntRange::Linear {
                    min: 0,
                    max: BEND_RANGE_MAX_ST,
                },
            )
            .with_unit(" st")
            .with_value_to_string(Arc::new(|v| v.to_string()))
            .with_string_to_value(Arc::new(|s| {
                parse_semitones()(s).map(|st| st.round() as i32)
            })),
            volume: FloatParam::new(
                "Volume",
                util::db_to_gain(VOLUME_INIT_DB),
                FloatRange::Skewed {
                    min: util::db_to_gain(VOLUME_MIN_DB),
                    max: util::db_to_gain(VOLUME_MAX_DB),
                    factor: FloatRange::gain_skew_factor(VOLUME_MIN_DB, VOLUME_MAX_DB),
                },
            )
            .with_smoother(SmoothingStyle::Logarithmic(GAIN_SMOOTHING_MS))
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_gain_to_db(1))
            .with_string_to_value(formatters::s2v_f32_gain_to_db()),

            env1: EnvelopeParams::new(1),
            env2: EnvelopeParams::new(2),
            env3: EnvelopeParams::new(3),

            // A vibrato, a slow sweep and a drift: each useful the moment a route takes it.
            lfo1: LfoParams::new(1, 5.0),
            lfo2: LfoParams::new(2, 1.0),
            lfo3: LfoParams::new(3, 0.2),

            routes: Routes::default(),
            preset: RwLock::new(mxm_preset::PresetIdentity::none()),
        }
    }
}

impl ModalSynthParams {
    /// The three envelopes in order.
    #[must_use]
    pub fn envelopes(&self) -> [&EnvelopeParams; ENVELOPES] {
        [&self.env1, &self.env2, &self.env3]
    }

    /// The three LFOs in order.
    #[must_use]
    pub fn lfos(&self) -> [&LfoParams; LFOS] {
        [&self.lfo1, &self.lfo2, &self.lfo3]
    }

    /// **Every parameter with its permanent id, in declaration order** — the order a preset file is
    /// written in and Init writes them, and the editor's coverage inventory. `the_parameters_are_
    /// the_param_map_in_order` holds it to nice-plug's own list.
    pub fn all_parameters(&self) -> Vec<(&'static str, &dyn mxm_params::ErasedParam)> {
        let voice: [&dyn mxm_params::ErasedParam; 21] = [
            &self.model,
            &self.coarse,
            &self.fine,
            &self.contour,
            &self.bow,
            &self.bow_timbre,
            &self.blow,
            &self.flow,
            &self.blow_timbre,
            &self.strike,
            &self.mallet,
            &self.strike_timbre,
            &self.geometry,
            &self.brightness,
            &self.damping,
            &self.position,
            &self.space,
            &self.mode,
            &self.glide,
            &self.bend_range,
            &self.volume,
        ];
        let mut out: Vec<(&'static str, &dyn mxm_params::ErasedParam)> =
            Vec::with_capacity(PARAMETER_COUNT);
        out.extend(VOICE_IDS.into_iter().zip(voice));
        for (ids, envelope) in ENVELOPE_IDS.iter().zip(self.envelopes()) {
            let params: [&dyn mxm_params::ErasedParam; 7] = [
                &envelope.attack,
                &envelope.decay,
                &envelope.sustain,
                &envelope.release,
                &envelope.response,
                &envelope.cycle,
                &envelope.mode,
            ];
            out.extend(ids.iter().copied().zip(params));
        }
        for (ids, lfo) in LFO_IDS.iter().zip(self.lfos()) {
            let params: [&dyn mxm_params::ErasedParam; 5] =
                [&lfo.rate, &lfo.shape, &lfo.sync, &lfo.retrigger, &lfo.fade];
            out.extend(ids.iter().copied().zip(params));
        }
        for (ids, route) in ROUTE_IDS.iter().zip(self.routes.all()) {
            let params: [&dyn mxm_params::ErasedParam; 3] =
                [&route.source, &route.target, &route.amount];
            out.extend(ids.iter().copied().zip(params));
        }
        out
    }

    /// **The patch as the DSP takes it**, read once per process call: every value as set (the
    /// modulated value where a host modulates it), each LFO's Rate resolved through its sync at
    /// `tempo`. The smoothed signals are then moved by [`Self::advance`] once per internal block.
    /// Allocates nothing.
    #[must_use]
    pub fn settings(&self, tempo: Option<f64>) -> Settings {
        let envelopes = self.envelopes().map(EnvelopeParams::settings);
        let lfos = self.lfos().map(|lfo| lfo.settings(tempo));
        let slots = self.routes.all().map(crate::routes::RouteParams::slot);
        Settings {
            model: self.model.value(),
            coarse_st: self.coarse.value() as f32,
            fine_st: self.fine.value(),
            contour: self.contour.value(),
            bow: self.bow.value(),
            bow_timbre: self.bow_timbre.value(),
            blow: self.blow.value(),
            flow: self.flow.value(),
            blow_timbre: self.blow_timbre.value(),
            strike: self.strike.value(),
            mallet: self.mallet.value(),
            strike_timbre: self.strike_timbre.value(),
            geometry: self.geometry.value(),
            brightness: self.brightness.value(),
            damping: self.damping.value(),
            position: self.position.value(),
            space: self.space.value(),
            mode: self.mode.value().into(),
            glide_s: self.glide.value(),
            bend_range_st: self.bend_range.value() as f32,
            volume: self.volume.value(),
            envelopes,
            lfos,
            slots,
        }
    }

    /// **The smoothed signals, moved `samples` on**: each smoother steps once per sample of the
    /// block it serves, so a block reads where its ramp has reached and no smoother falls behind.
    /// Everything [`Self::settings`] read unsmoothed stays as it is. Allocates nothing.
    pub fn advance(&self, settings: &mut Settings, samples: u32) {
        if samples == 0 {
            return;
        }
        settings.fine_st = self.fine.smoothed.next_step(samples);
        settings.contour = self.contour.smoothed.next_step(samples);
        settings.bow = self.bow.smoothed.next_step(samples);
        settings.bow_timbre = self.bow_timbre.smoothed.next_step(samples);
        settings.blow = self.blow.smoothed.next_step(samples);
        settings.flow = self.flow.smoothed.next_step(samples);
        settings.blow_timbre = self.blow_timbre.smoothed.next_step(samples);
        settings.strike = self.strike.smoothed.next_step(samples);
        settings.mallet = self.mallet.smoothed.next_step(samples);
        settings.strike_timbre = self.strike_timbre.smoothed.next_step(samples);
        settings.geometry = self.geometry.smoothed.next_step(samples);
        settings.brightness = self.brightness.smoothed.next_step(samples);
        settings.damping = self.damping.smoothed.next_step(samples);
        settings.position = self.position.smoothed.next_step(samples);
        settings.space = self.space.smoothed.next_step(samples);
        settings.volume = self.volume.smoothed.next_step(samples);
        for (envelope, params) in settings.envelopes.iter_mut().zip(self.envelopes()) {
            envelope.sustain = params.sustain.smoothed.next_step(samples);
        }
        for (slot, route) in settings.slots.iter_mut().zip(self.routes.all()) {
            slot.amount = route.amount.smoothed.next_step(samples);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routes::{SourceChoice, TargetChoice};
    use nice_plug::params::InternalParamMut;

    fn set_normalized<P: InternalParamMut>(param: &P, normalized: f32) {
        // SAFETY: exclusive test ownership; no process or GUI thread exists.
        unsafe {
            let _ = param._internal_set_normalized_value(normalized);
        }
    }

    /// **Every parameter's text survives the host's round trip**, with the unit on, across the
    /// collection's grids and switch points (`mxm_plugin_test::text_checks`). The readings here
    /// switch only where the collection's list already probes: a time at one second.
    #[test]
    fn every_parameter_text_survives_the_hosts_round_trip() {
        mxm_plugin_test::text_checks::every_text_round_trips(&ModalSynthParams::default(), &[]);
    }

    /// **105 parameters, every id unique**, the plan's §5.5 count.
    #[test]
    fn the_ids_are_unique_and_there_are_105() {
        let params = ModalSynthParams::default();
        let ids: Vec<String> = params.param_map().into_iter().map(|(id, ..)| id).collect();
        assert_eq!(ids.len(), 105);
        assert_eq!(PARAMETER_COUNT, 105);
        let mut unique = ids.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), ids.len(), "a duplicate id in {ids:?}");
    }

    /// The ids a list's values are stored by, in its order: `None` for a parameter that is no list.
    fn list_ids(id: &str) -> Option<&'static [&'static str]> {
        if id == "mode" {
            VoiceMode::ids()
        } else if id.starts_with("env") && id.ends_with("_mode") {
            EnvelopeModeChoice::ids()
        } else if id.ends_with("_shape") {
            LfoShapeChoice::ids()
        } else if id.ends_with("_source") {
            SourceChoice::ids()
        } else if id.ends_with("_target") {
            TargetChoice::ids()
        } else {
            None
        }
    }

    /// One line of `frozen-parameters.txt` for each parameter, in declaration order: its id, kind,
    /// steps, default, its plain value at five points of its travel, and a list's ids.
    fn frozen_lines(params: &ModalSynthParams) -> Vec<String> {
        params
            .param_map()
            .into_iter()
            .map(|(id, ptr, _)| {
                let kind = match ptr {
                    ParamPtr::FloatParam(_) => "float",
                    ParamPtr::IntParam(_) => "int",
                    ParamPtr::BoolParam(_) => "bool",
                    ParamPtr::EnumParam(_) => "enum",
                };
                let values = list_ids(&id);
                assert_eq!(kind == "enum", values.is_some(), "{id}: a list's ids");
                // SAFETY: the parameters outlive the pointers, read on this thread alone.
                let (steps, default, travel) = unsafe {
                    (
                        ptr.step_count().map_or("-".to_owned(), |n| n.to_string()),
                        format!("{:?}", ptr.default_plain_value()),
                        // A switch's travel is its two values, which its steps and default hold.
                        [0.0, 0.25, 0.5, 0.75, 1.0]
                            .map(|at| match kind {
                                "bool" => format!("{:>12}", "-"),
                                _ => format!("{:>12}", format!("{:?}", ptr.preview_plain(at))),
                            })
                            .join(" "),
                    )
                };
                let values = values.map_or("-".to_owned(), |ids| ids.join(","));
                format!("{id:<15} {kind:<5} {steps:>3} {default:>12} {travel} {values}")
            })
            .collect()
    }

    /// Two lines agree: the same words, and numbers within rounding.
    fn agree(frozen: &str, now: &str) -> bool {
        let (frozen, now): (Vec<&str>, Vec<&str>) = (
            frozen.split_whitespace().collect(),
            now.split_whitespace().collect(),
        );
        frozen.len() == now.len()
            && frozen
                .iter()
                .zip(&now)
                .all(|(a, b)| match (a.parse::<f64>(), b.parse::<f64>()) {
                    (Ok(a), Ok(b)) => (a - b).abs() <= 1e-6 * a.abs().max(1.0),
                    _ => a == b,
                })
    }

    /// **The parameters are as frozen at E5** (2026-11-05), `frozen-parameters.txt` beside
    /// `Cargo.toml`: every id, its kind, steps and default, its range as the plain value at five
    /// points of its travel (which holds a skew as well as the ends), and each list's values by the
    /// ids a saved state stores them by, in their order. A saved state stores plain values and a
    /// list's ids; a preset and VST3 automation store the travel, a list's value as its index over
    /// its length; so none of it may move, and no list may grow. A line never changes and no id is
    /// removed or reused; a parameter added later is appended, the line this test prints. Numbers
    /// agree within rounding: each platform's maths library rounds a skew its own way.
    #[test]
    fn the_parameters_are_the_frozen_tables() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/frozen-parameters.txt");
        let file = std::fs::read_to_string(path).expect("frozen-parameters.txt");
        let frozen: Vec<&str> = file
            .lines()
            .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
            .collect();
        let id = |line: &str| {
            line.split_whitespace()
                .next()
                .unwrap_or_default()
                .to_owned()
        };
        let now = frozen_lines(&ModalSynthParams::default());
        for was in &frozen {
            let line = now.iter().find(|line| id(line) == id(was));
            let line = line.unwrap_or_else(|| panic!("{} is frozen: never removed", id(was)));
            assert!(
                agree(was, line),
                "{} is frozen, and changed:\n  was {was}\n  now {line}",
                id(was)
            );
        }
        let new: Vec<&String> = now
            .iter()
            .filter(|line| !frozen.iter().any(|was| id(was) == id(line)))
            .collect();
        assert!(
            new.is_empty(),
            "not frozen; append to frozen-parameters.txt:\n{}",
            new.iter()
                .map(|line| line.as_str())
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

    /// The preset system's list is nice-plug's, id for id and in order, so neither can omit or
    /// reorder a parameter the other has.
    #[test]
    fn the_parameters_are_the_param_map_in_order() {
        let params = ModalSynthParams::default();
        let map: Vec<String> = params.param_map().into_iter().map(|(id, ..)| id).collect();
        let listed: Vec<&str> = params.all_parameters().iter().map(|(id, _)| *id).collect();
        assert_eq!(listed, map);
        let names: Vec<String> = params
            .param_map()
            .into_iter()
            .map(|(_, ptr, _)| unsafe { ptr.name() }.to_owned())
            .collect();
        for ((_, param), name) in params.all_parameters().iter().zip(names) {
            assert_eq!(param.name(), name, "the same parameter on both lists");
        }
    }

    /// **The Init rule**: every amount starts at zero, nothing is routed, nothing follows the
    /// host's tempo and one plain source sounds (the strike into the modal resonator, `Part::Init`'s
    /// patch). Pins the rule, not the taste: a retune of a configuration passes, a non-zero depth
    /// fails.
    #[test]
    fn init_is_every_amount_at_zero_and_nothing_routed() {
        let params = ModalSynthParams::default();
        let all = params.all_parameters();
        for id in AMOUNT_IDS {
            let (_, param) = all
                .iter()
                .find(|(candidate, _)| *candidate == id)
                .unwrap_or_else(|| panic!("{id} is not a parameter"));
            let zero = unsafe {
                let (_, ptr, _) = params
                    .param_map()
                    .into_iter()
                    .find(|(candidate, ..)| candidate == id)
                    .unwrap();
                ptr.preview_normalized(0.0)
            };
            assert_eq!(param.default_normalised(), zero, "{id} is an amount");
        }
        for route in params.routes.all() {
            assert_eq!(route.source.default_plain_value(), SourceChoice(None));
            assert_eq!(route.target.default_plain_value(), TargetChoice(None));
        }
        for lfo in params.lfos() {
            assert!(!lfo.sync.default_plain_value(), "no sync at Init");
        }
        assert_eq!(params.model.default_plain_value(), INIT_MODEL);
        assert_eq!(
            (
                params.bow.default_plain_value(),
                params.blow.default_plain_value()
            ),
            (0.0, 0.0),
            "the strike alone sounds"
        );

        // And the patch the DSP gets from Init says the same.
        let settings = params.settings(Some(120.0));
        assert!(
            settings.slots.iter().all(|slot| {
                slot.source.is_none() && slot.target.is_none() && slot.amount == 0.0
            })
        );
        assert_eq!(settings.glide_s, 0.0);
        // And Init is the module's own patch, as `Part::Init` sets it.
        let module = mxm_modal_synth_dsp::Settings::default();
        assert_eq!(
            (
                settings.model,
                settings.contour,
                settings.strike,
                settings.geometry,
                settings.damping,
                settings.position,
                settings.space
            ),
            (
                module.model,
                module.contour,
                module.strike,
                module.geometry,
                module.damping,
                module.position,
                module.space
            )
        );
        assert!(settings.lfos.iter().all(|lfo| lfo.fade_s == 0.0));
    }

    /// **Every configuration starts somewhere it can be heard**: envelope times inside their range
    /// and short enough to play, LFO rates in the musical band, Volume below full scale.
    #[test]
    fn init_configurations_are_useful() {
        let settings = ModalSynthParams::default().settings(None);
        for envelope in settings.envelopes {
            for time in [envelope.attack_s, envelope.decay_s, envelope.release_s] {
                assert!((ENVELOPE_MIN_S..=1.0).contains(&time), "{envelope:?}");
            }
            assert!(envelope.sustain > 0.0 && !envelope.cycle);
        }
        for lfo in settings.lfos {
            assert!((0.1..=10.0).contains(&lfo.rate_hz), "{lfo:?}");
        }
        assert!(settings.volume > 0.0 && settings.volume < 1.0);
        assert_eq!(settings.bend_range_st, 2.0);
        assert_eq!(settings.mode, Mode::Poly);
    }

    /// **Each LFO's sync picks a division and is inert without a tempo**: off, or with no tempo,
    /// the Rate's own hertz stand; on at 120 bpm the ends are the ladder's ends the range can hold,
    /// the top the fastest; and the patch the DSP gets carries the resolved rate.
    #[test]
    fn the_lfo_sync_picks_a_division_and_is_inert_without_a_tempo() {
        let p = ModalSynthParams::default();
        for (i, lfo) in p.lfos().into_iter().enumerate() {
            set_normalized(&lfo.rate, 1.0);
            assert_eq!(lfo.synced_rate(Some(120.0)), None, "off is the free rate");
            set_normalized(&lfo.sync, 1.0);
            assert_eq!(lfo.synced_rate(None), None, "no tempo is the free rate");
            assert_eq!(p.settings(None).lfos[i].rate_hz, LFO_MAX_HZ);

            let top = lfo.synced_rate(Some(120.0)).expect("synced at a tempo");
            set_normalized(&lfo.rate, 0.0);
            let bottom = lfo.synced_rate(Some(120.0)).expect("synced at a tempo");
            assert!(top > bottom, "the top of a rate is the fastest");
            let reach = LFO_SYNC
                .reachable(120.0, f64::from(LFO_MIN_HZ), f64::from(LFO_MAX_HZ))
                .divisions();
            assert!((top - reach[0].hz(120.0) as f32).abs() < 1e-4);
            assert!((bottom - reach[reach.len() - 1].hz(120.0) as f32).abs() < 1e-4);
            assert_eq!(p.settings(Some(120.0)).lfos[i].rate_hz, bottom);
        }
    }

    /// A typed division picks the Rate position that selects it.
    #[test]
    fn a_typed_division_selects_its_position() {
        let p = ModalSynthParams::default();
        let normalized = p
            .lfo1
            .rate
            .string_to_normalized_value("1/8")
            .expect("a division parses");
        assert!((normalized - LFO_SYNC.position(mxm_tempo::Division::Eighth)).abs() < 1e-6);
    }

    /// **The smoothed signals arrive in blocks**: `advance` steps a ramp by the block's length and
    /// leaves what `settings` read unsmoothed alone.
    #[test]
    fn advance_moves_the_smoothed_signals_and_nothing_else() {
        let p = ModalSynthParams::default();
        for (_, ptr, _) in p.param_map() {
            unsafe { ptr._internal_update_smoother(48_000.0, true) };
        }
        set_normalized(&p.brightness, 1.0);
        set_normalized(&p.glide, 1.0);
        unsafe {
            p.brightness._internal_update_smoother(48_000.0, false);
        }
        let mut settings = p.settings(None);
        assert_eq!(settings.brightness, 1.0, "the patch as set");
        p.advance(&mut settings, 48);
        assert!(
            settings.brightness > 0.5 && settings.brightness < 1.0,
            "1 ms into a {SIGNAL_SMOOTHING_MS} ms ramp: {}",
            settings.brightness
        );
        assert_eq!(settings.glide_s, GLIDE_MAX_S, "a time is not smoothed");
        p.advance(&mut settings, 48_000);
        assert_eq!(settings.brightness, 1.0);
    }

    /// **The product's names are its own** (root `AGENTS.md`, *The names*): no parameter, model,
    /// mode, shape, source or target names the module, its hidden voice or the maker it comes
    /// from.
    #[test]
    fn no_name_is_the_originals() {
        use crate::routes::{SourceChoice, TargetChoice};
        let params = ModalSynthParams::default();
        let mut names: Vec<String> = params
            .all_parameters()
            .iter()
            .map(|(_, p)| p.name().to_owned())
            .collect();
        names.extend(MODEL_NAMES.iter().map(|n| (*n).to_owned()));
        for list in [
            VoiceMode::variants(),
            EnvelopeModeChoice::variants(),
            LfoShapeChoice::variants(),
            SourceChoice::variants(),
            TargetChoice::variants(),
        ] {
            names.extend(list.iter().map(|n| (*n).to_owned()));
        }
        for name in names {
            let lower = name.to_lowercase();
            for word in [
                "elements",
                "ominous",
                "mutable",
                "maths",
                "make noise",
                "plaits",
            ] {
                assert!(!lower.contains(word), "{name:?} names {word:?}");
            }
        }
    }
}
