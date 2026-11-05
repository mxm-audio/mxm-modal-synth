//! One of the instrument's voices: an Elements voice (`mxm_modal_voice::Voice`, the module's
//! voice, with its hidden one beside it, `OminousVoice`, as `Part` holds both) with the case
//! around it: the note, glide and tuning on its pitch, the gate a block after its controls, three
//! envelopes, three LFOs (or the shared ones), the slots' targets into its patch, and its gains
//! into the part.
//!
//! Per engine block of 16 samples, in order (the crate's `AGENTS.md`, *The routing*): the
//! performance sources, the LFOs, envelopes 1, 2 and 3, then the voice's targets, the voice's
//! render, and last its exciter envelope as the *Contour* source, which every reader so reads one
//! block late.

use mxm_modal_voice::dsp::{K_MAX_BLOCK_SIZE, K_SAMPLE_RATE};
use mxm_modal_voice::resources::{LUT_MIDI_TO_F_HIGH, LUT_MIDI_TO_F_LOW};
use mxm_modal_voice::stmlib::dsp::dsp::cast_i32;
use mxm_modal_voice::{OminousVoice, Patch, Voice as Engine};
use mxm_modulation::standard;

use crate::envelope::{Envelope, Inputs};
use crate::lfo::{Fade, Lfo};
use crate::part::raw_gain;
use crate::routing::{self, KEY_UNIT, Routing};
use crate::{ENGINE_RATE, ENVELOPES, LFOS, MODELS, Settings, Source, Target};

/// The engine's block: `elements.cc`'s 16.
pub(crate) const BLOCK: usize = K_MAX_BLOCK_SIZE;
/// A block's length, seconds: the modulation's step.
pub(crate) const DT: f32 = BLOCK as f32 / ENGINE_RATE;

/// The model that is the hidden voice; the rest are the voice's resonators.
pub(crate) const HIDDEN: i32 = 3;

/// What `cv_scaler.cc` constrains a CV-able control to (its `BIND`s): most to 0.9995, Strike
/// timbre to 0.995, Space to 2. A pot without a CV (Contour, the three levels) reads at most
/// 65 535 / 65 536.
const TOP: f32 = 0.9995;
const STRIKE_TIMBRE_TOP: f32 = 0.995;
const SPACE_TOP: f32 = 2.0;
const POT_TOP: f32 = 65_535.0 / 65_536.0;

/// Strength with no slot on it: the accent's 0 dB (`10^(1.5 (s − 0.5))`), where the module's
/// unpatched pressure CV rests (`research:physical-modelling/mutable-instruments-elements.md` §6).
pub(crate) const STRENGTH_REST: f32 = 0.5;

/// `Part::Init`'s values for the patch's fields no knob sets, and no seed (ops'
/// `plans/plan-elements.md` D10).
pub(crate) const MODULATION_FREQUENCY: f32 = 0.5 / K_SAMPLE_RATE;
pub(crate) const MODULATION_OFFSET: f32 = 0.1;
pub(crate) const REVERB_DIFFUSION: f32 = 0.625;
pub(crate) const REVERB_LP: f32 = 0.7;

/// `cv_scaler.cc`'s one-poles a block on the controls it smooths (`BIND`'s coefficients).
const FLOW_LP: f32 = 0.05;
const MALLET_LP: f32 = 0.05;
const GEOMETRY_LP: f32 = 0.05;
const POSITION_LP: f32 = 0.01;
const SPACE_LP: f32 = 0.01;

/// A block is silent when every output of the voice is below this (−120 dB).
const SILENCE: f32 = 1e-6;
/// Silent blocks a released voice must give before it is idle: 128 ms, longer than half a period
/// of the lowest note, so a resonator's ring is not cut at its zero crossings.
const SILENT_BLOCKS: u32 = 256;
/// A released voice still sounding this long after its key went up, its envelopes at rest, is
/// faded out and stopped, so a released note always ends: only a resonator that never ends on its
/// own gets there (a string's damping at its top crossfades to an infinite decay). Thirty
/// seconds, the longest time an envelope sets.
const GUARD_BLOCKS: u32 = (30.0 / DT) as u32;
/// That fade, 10 ms; also the end of the hidden voice's dry drone.
const GUARD_FADE: f32 = 0.01;
/// Glide: the time to within 1 % of the interval is the Glide setting.
const GLIDE_TIME_CONSTANTS: f32 = 4.605_17;

/// The engine's inputs, which this instrument leaves silent (ops' `plans/plan-elements.md` §5.6).
const QUIET_INPUT: [f32; BLOCK] = [0.0; BLOCK];

/// What the instrument hands every voice for a block.
pub(crate) struct Shared<'a> {
    pub settings: &'a Settings,
    pub routing: &'a Routing,
    pub wheel: f32,
    pub pressure: f32,
    pub bend: f32,
    /// The shared LFOs' outputs this block (an LFO with Retrigger off).
    pub lfos: [f32; LFOS],
}

/// The controls `cv_scaler.cc` smooths, after their one-poles.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Smoothed {
    flow: f32,
    mallet: f32,
    geometry: f32,
    position: f32,
    space: f32,
}

/// A voice's block, for the part: the engine's three outputs, which group it mixes in, its Space
/// and its gains.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Output {
    /// The hidden voice's, which the part's soft limit leaves out.
    pub hidden: bool,
    pub raw: [f32; BLOCK],
    pub center: [f32; BLOCK],
    pub sides: [f32; BLOCK],
    /// Space as the voice's patch has it: its spread and dry gain the voice's, the reverb's the
    /// newest voice's.
    pub space: f32,
    /// Left and right gains at the block's start and end (Amplitude, Pan, the guard's fade),
    /// ramped across it.
    pub from: [f32; 2],
    pub to: [f32; 2],
}

impl Default for Output {
    fn default() -> Self {
        Self {
            hidden: false,
            raw: [0.0; BLOCK],
            center: [0.0; BLOCK],
            sides: [0.0; BLOCK],
            space: 0.0,
            from: [1.0; 2],
            to: [1.0; 2],
        }
    }
}

/// What the voice handed its engine in a block, for the test that holds it to `Part`.
#[cfg(test)]
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Played {
    pub patch: Patch,
    pub gate: bool,
    pub note: f32,
    pub strength: f32,
    pub model: i32,
}

pub(crate) struct Voice {
    engine: Box<Engine>,
    hidden: Box<OminousVoice>,
    /// The engines are as `init` left them: nothing rendered since.
    fresh: bool,
    /// Rendered; an idle voice is not, and adds nothing.
    pub active: bool,
    /// The key is held.
    pub gate: bool,
    pub note: u8,
    pub id: Option<i32>,
    /// Which note-on this voice took, counting from one.
    pub started: u64,
    /// The published Velocity (`v − 1`) and Random (`2u − 1`), drawn per note.
    velocity: f32,
    random: f32,
    /// Per-note pitch expression, semitones.
    pub tuning: f32,
    /// The glide's offset from the note, semitones, decaying to zero.
    glide: f32,
    /// The gate the engine sees next block: the key's, one block after its controls.
    staged: bool,
    /// The gate the engine saw last block.
    engine_gate: bool,
    /// The engine's gate low for the next block, so it sees a new edge.
    retrigger: bool,
    /// A new note for the envelopes and the model's latch in the next block.
    note_on: bool,
    /// The model, latched at each note: 0 to 2 the resonators, 3 the hidden voice.
    model: i32,
    /// `None` until the voice's first block, which starts its smoothing at its targets.
    smoothed: Option<Smoothed>,
    /// Blocks in a row whose outputs were all silent.
    silent: u32,
    /// Blocks rendered with the gate low.
    quiet: u32,
    /// A fade to idle, once it has begun: its gain, and whether the guard began it.
    ending: Option<(f32, bool)>,
    envelopes: [Envelope; ENVELOPES],
    lfos: [Lfo; LFOS],
    fades: [Fade; LFOS],
    frame: routing::Frame,
    /// The last block's gains; `None` before the first, which then starts at its own.
    gains: Option<[f32; 2]>,
    /// The last block, for the part.
    pub output: Output,
    #[cfg(test)]
    pub(crate) played: Played,
    /// Notes the guard ended.
    #[cfg(test)]
    pub(crate) guarded: u32,
}

impl Voice {
    /// Voice `index`: its engines, each as `Part::Init` leaves it, and its LFOs' random streams,
    /// each its own.
    pub(crate) fn new(index: usize) -> Self {
        let mut engine = Engine::new();
        engine.init();
        let mut hidden = OminousVoice::new();
        hidden.init();
        Self {
            engine,
            hidden,
            fresh: true,
            active: false,
            gate: false,
            note: 60,
            id: None,
            started: 0,
            velocity: 0.0,
            random: 0.0,
            tuning: 0.0,
            glide: 0.0,
            staged: false,
            engine_gate: false,
            retrigger: false,
            note_on: false,
            model: 0,
            smoothed: None,
            silent: 0,
            quiet: 0,
            ending: None,
            envelopes: [Envelope::new(); ENVELOPES],
            lfos: std::array::from_fn(|l| Lfo::new(lfo_seed(index, l))),
            fades: [Fade::default(); LFOS],
            frame: routing::Frame::new(),
            gains: None,
            output: Output::default(),
            #[cfg(test)]
            played: Played::default(),
            #[cfg(test)]
            guarded: 0,
        }
    }

    /// Every state as `new` made it; allocates nothing.
    pub(crate) fn clear(&mut self) {
        self.engine.clear();
        self.engine.init();
        self.hidden.clear();
        self.hidden.init();
        self.fresh = true;
        self.active = false;
        self.gate = false;
        self.note = 60;
        self.id = None;
        self.started = 0;
        self.velocity = 0.0;
        self.random = 0.0;
        self.tuning = 0.0;
        self.glide = 0.0;
        self.staged = false;
        self.engine_gate = false;
        self.retrigger = false;
        self.note_on = false;
        self.model = 0;
        self.smoothed = None;
        self.silent = 0;
        self.quiet = 0;
        self.ending = None;
        for e in &mut self.envelopes {
            e.reset();
        }
        for l in &mut self.lfos {
            l.reset();
        }
        self.fades = [Fade::default(); LFOS];
        self.frame.reset();
        self.gains = None;
        self.output = Output::default();
    }

    /// The glided pitch, semitones.
    pub(crate) fn pitch(&self) -> f32 {
        f32::from(self.note) + self.glide
    }

    /// A new note: an idle voice starts as the engines' `init` leaves them; a sounding one keeps
    /// its state (its resonator rings into the new note, as `Part`'s one voice's does) and
    /// restarts its envelopes' attacks from where they are, with its engine's gate dropped for a
    /// block if it was high. `glide_from` is the pitch it glides from, if it glides.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn start(
        &mut self,
        settings: &Settings,
        note: u8,
        id: Option<i32>,
        velocity: f32,
        random: f32,
        order: u64,
        glide_from: Option<f32>,
    ) {
        if !self.active {
            if !self.fresh {
                self.engine.init();
                self.hidden.init();
                self.fresh = true;
            }
            for e in &mut self.envelopes {
                e.reset();
            }
            self.frame.reset();
            self.gains = None;
            self.smoothed = None;
            self.staged = false;
            self.engine_gate = false;
        }
        self.retrigger = self.engine_gate;
        self.active = true;
        self.gate = true;
        self.note_on = true;
        self.quiet = 0;
        self.silent = 0;
        self.ending = None;
        self.glide = glide_from.map_or(0.0, |from| from - f32::from(note));
        self.note = note;
        self.id = id;
        self.started = order;
        self.velocity = velocity;
        self.random = random;
        self.tuning = 0.0;
        for (l, s) in settings.lfos.iter().enumerate() {
            if s.retrigger {
                self.lfos[l].restart();
            }
            self.fades[l].restart(s.fade_s);
        }
    }

    /// The held note moves to another without a new gate (Legato's tie, or a release returning
    /// to the key held before it), gliding if Glide is set.
    pub(crate) fn tie(&mut self, settings: &Settings, note: u8, id: Option<i32>) {
        self.glide = if settings.glide_s > 0.0 {
            self.pitch() - f32::from(note)
        } else {
            0.0
        };
        self.note = note;
        self.id = id;
        self.tuning = 0.0;
    }

    pub(crate) fn release(&mut self) {
        self.gate = false;
    }

    /// Silent at once: idle, and the next note starts it afresh.
    pub(crate) fn choke(&mut self) {
        self.gate = false;
        self.active = false;
        self.staged = false;
        self.engine_gate = false;
    }

    /// `Part`'s panic: the resonator reset when it blows up.
    pub(crate) fn panic(&mut self) {
        self.engine.panic();
    }

    /// Whether the voice answers to a note: by its id when both have one, else by its key.
    pub(crate) fn answers(&self, note: u8, id: Option<i32>) -> bool {
        match (id, self.id) {
            (Some(a), Some(b)) => a == b,
            _ => self.note == note,
        }
    }

    /// The source's value in the voice's frame, as a slot would read it now.
    #[cfg(test)]
    pub(crate) fn published(&self, source: Source) -> f32 {
        self.frame.read(source.index())
    }

    /// The model the voice latched at its note.
    #[cfg(test)]
    pub(crate) fn model(&self) -> i32 {
        self.model
    }

    /// One block: the modulation, the engine's 16 samples, into [`Voice::output`].
    pub(crate) fn process(&mut self, shared: &Shared) {
        let s = shared.settings;
        let r = shared.routing;
        let key = self.pitch();
        if s.glide_s > 0.0 && self.glide != 0.0 {
            self.glide *= (-DT * GLIDE_TIME_CONSTANTS / s.glide_s).exp();
            if self.glide.abs() < 1e-4 {
                self.glide = 0.0;
            }
        } else {
            self.glide = 0.0;
        }

        // The performance sources.
        let f = &mut self.frame;
        f.begin_sample();
        f.write(Source::Key.index(), standard::key(key, KEY_UNIT));
        f.write(Source::Velocity.index(), self.velocity);
        f.write(Source::Wheel.index(), shared.wheel);
        f.write(Source::Pressure.index(), shared.pressure);
        f.write(Source::Bend.index(), shared.bend);
        f.write(Source::Random.index(), self.random);

        // The LFOs: a voice's own with Retrigger on, else the shared one; each faded in.
        for l in 0..LFOS {
            let ls = &s.lfos[l];
            let value = if ls.retrigger {
                let octaves = r.sum(Target::lfo_rate(l), f);
                self.lfos[l].advance(ls.shape, ls.rate_hz * octaves.exp2(), DT)
            } else {
                shared.lfos[l]
            };
            let depth = self.fades[l].advance(ls.fade_s, DT);
            f.write(Source::lfo(l).index(), value * depth);
        }

        // Envelopes 1, 2 and 3.
        for e in 0..ENVELOPES {
            let t = Target::envelope(e);
            let inputs = Inputs {
                note_gate: self.gate,
                note_on: self.note_on,
                rise: r.sum(t.rise, f),
                fall: r.sum(t.fall, f),
                time: r.sum(t.time, f),
                cycle: r.sum(t.cycle, f),
                trigger: r.sum(t.trigger, f),
                input: r.sum(t.input, f),
            };
            let env = &mut self.envelopes[e];
            env.process(&s.envelopes[e], &inputs, DT);
            let [level, rise, cycle] = Source::envelope(e);
            let eor = if env.end_of_rise() { 1.0 } else { 0.0 };
            f.write(level.index(), env.value());
            f.write(rise.index(), eor);
            f.write(cycle.index(), 1.0 - eor);
        }

        // The model, latched at the note, as the module's hidden gestures set it for a whole
        // performance: a slot on it picks each note's.
        if self.note_on {
            let model = (s.model as f32 + r.sum(Target::Model, f)).round();
            self.model = (model as i32).clamp(0, MODELS - 1);
        }
        self.note_on = false;

        // The voice's targets, into the module's patch, each constrained as `cv_scaler.cc` does.
        let control =
            |base: f32, target: Target, top: f32| (base + r.sum(target, f)).clamp(0.0, top);
        let targets = Smoothed {
            flow: control(s.flow, Target::Flow, TOP),
            mallet: control(s.mallet, Target::Mallet, TOP),
            geometry: control(s.geometry, Target::Geometry, TOP),
            position: control(s.position, Target::Position, TOP),
            space: control(s.space, Target::Space, SPACE_TOP),
        };
        let smoothed = match self.smoothed {
            None => targets,
            Some(was) => Smoothed {
                flow: was.flow + (targets.flow - was.flow) * FLOW_LP,
                mallet: was.mallet + (targets.mallet - was.mallet) * MALLET_LP,
                geometry: was.geometry + (targets.geometry - was.geometry) * GEOMETRY_LP,
                position: was.position + (targets.position - was.position) * POSITION_LP,
                space: was.space + (targets.space - was.space) * SPACE_LP,
            },
        };
        self.smoothed = Some(smoothed);
        let patch = Patch {
            exciter_envelope_shape: control(s.contour, Target::Contour, POT_TOP),
            exciter_bow_level: control(s.bow, Target::Bow, POT_TOP),
            exciter_bow_timbre: control(s.bow_timbre, Target::BowTimbre, TOP),
            exciter_blow_level: control(s.blow, Target::Blow, POT_TOP),
            exciter_blow_meta: smoothed.flow,
            exciter_blow_timbre: control(s.blow_timbre, Target::BlowTimbre, TOP),
            exciter_strike_level: control(s.strike, Target::Strike, POT_TOP),
            exciter_strike_meta: smoothed.mallet,
            exciter_strike_timbre: control(
                s.strike_timbre,
                Target::StrikeTimbre,
                STRIKE_TIMBRE_TOP,
            ),
            exciter_signature: 0.0,
            resonator_geometry: smoothed.geometry,
            resonator_brightness: control(s.brightness, Target::Brightness, TOP),
            resonator_damping: control(s.damping, Target::Damping, TOP),
            resonator_position: smoothed.position,
            resonator_modulation_frequency: MODULATION_FREQUENCY,
            resonator_modulation_offset: MODULATION_OFFSET,
            reverb_diffusion: REVERB_DIFFUSION,
            reverb_lp: REVERB_LP,
            space: smoothed.space,
            modulation_frequency: 0.0,
        };
        let strength = (STRENGTH_REST + r.sum(Target::Strength, f)).clamp(0.0, 1.0);
        let note = key
            + s.coarse_st
            + s.fine_st
            + shared.bend * s.bend_range_st
            + self.tuning
            + r.sum(Target::Pitch, f);

        // The gate, a block after the note's controls: the module reads its CVs a block before
        // they reach the part and its gate two, so a strike lands after its pitch and strength.
        let gate = self.staged && !self.retrigger;
        self.retrigger = false;
        self.staged = self.gate;
        self.engine_gate = gate;

        let hidden = self.model == HIDDEN;
        let o = &mut self.output;
        if hidden {
            self.hidden.process(
                &patch,
                note,
                strength,
                gate,
                &QUIET_INPUT,
                &QUIET_INPUT,
                &mut o.raw,
                &mut o.center,
                &mut o.sides,
                BLOCK,
            );
        } else {
            // `Part::Process`: the MIDI pitch to a frequency, its `CONSTRAIN` a clamp.
            let pitch = cast_i32((note + 48.0) * 256.0).clamp(0, 65535) as usize;
            self.engine.set_resonator_model(self.model as u8);
            self.engine.process(
                &patch,
                LUT_MIDI_TO_F_HIGH[pitch >> 8] * LUT_MIDI_TO_F_LOW[pitch & 0xff],
                strength,
                gate,
                &QUIET_INPUT,
                &QUIET_INPUT,
                &mut o.raw,
                &mut o.center,
                &mut o.sides,
                BLOCK,
            );
        }
        self.fresh = false;
        let contour = if hidden {
            self.hidden.envelope()
        } else {
            self.engine.envelope()
        };
        self.frame.write(Source::Contour.index(), contour);

        // The gains: Amplitude scales, Pan balances (centre is unity in both channels, so a voice
        // at rest is the module's mix exactly), the guard fades.
        let amplitude = standard::amplitude_factor(r.sum(Target::Amplitude, &self.frame));
        let pan = r.sum(Target::Pan, &self.frame).clamp(-1.0, 1.0);
        let fade = self.ending.map_or(1.0, |(gain, _)| gain);
        let to = [
            (1.0 - pan.max(0.0)) * amplitude * fade,
            (1.0 + pan.min(0.0)) * amplitude * fade,
        ];
        o.from = self.gains.unwrap_or(to);
        o.to = to;
        o.hidden = hidden;
        o.space = smoothed.space;
        self.gains = Some(to);
        #[cfg(test)]
        {
            self.played = Played {
                patch,
                gate,
                note,
                strength,
                model: self.model,
            };
        }

        // Idle once the key is up and the engine has seen it, its envelopes at rest (those a slot
        // reads) and the outputs the mix hears silent for 128 ms. The hidden voice's dry path is
        // its oscillators, ungated (the module's drone): once its gated outputs are silent, that
        // fades over 10 ms, so a released note ends. Else, after 30 s, the guard.
        if self.gate || self.staged || gate {
            self.quiet = 0;
        } else {
            self.quiet = self.quiet.saturating_add(1);
        }
        let o = &self.output;
        let quiet = |x: &[f32; BLOCK]| x.iter().all(|x| x.abs() < SILENCE);
        let raw_heard = raw_gain(o.space) > 0.0;
        let drone = hidden && raw_heard && !quiet(&o.raw);
        if quiet(&o.center) && quiet(&o.sides) && (!raw_heard || hidden || quiet(&o.raw)) {
            self.silent = self.silent.saturating_add(1);
        } else {
            self.silent = 0;
        }
        let ended = self.quiet > 0 && self.silent >= SILENT_BLOCKS && self.envelopes_at_rest(r);
        if let Some((gain, guard)) = self.ending {
            let gain = gain - DT / GUARD_FADE;
            if gain <= 0.0 {
                self.active = false;
                self.ending = None;
                #[cfg(test)]
                if guard {
                    self.guarded += 1;
                }
                #[cfg(not(test))]
                let _ = guard;
            } else {
                self.ending = Some((gain, guard));
            }
        } else if ended {
            if drone {
                self.ending = Some((1.0 - DT / GUARD_FADE, false));
            } else {
                self.active = false;
            }
        } else if self.quiet >= GUARD_BLOCKS && self.envelopes_at_rest(r) {
            self.ending = Some((1.0 - DT / GUARD_FADE, true));
        }
    }

    /// Every envelope a slot reads is still: at rest, or in Follow mode holding its input's level
    /// (one following Key never rests, and its voice must still be freed once silent).
    fn envelopes_at_rest(&self, r: &Routing) -> bool {
        self.envelopes
            .iter()
            .enumerate()
            .all(|(e, env)| env.still() || !Source::envelope(e).iter().any(|&s| r.reads(s)))
    }
}

/// Each voice's LFO has its own random stream.
fn lfo_seed(voice: usize, lfo: usize) -> u32 {
    0x2545_f491_u32.wrapping_mul((voice * LFOS + lfo + 1) as u32)
}
