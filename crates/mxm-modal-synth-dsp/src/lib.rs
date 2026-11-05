//! MXM Modal Synth's instrument: eight voices of the ported modal synthesis voice
//! (`mxm-modal-voice`), each with its envelopes and LFOs, the sixteen route slots, the part they
//! share (their mix, one soft limit and one reverb, as Elements' `Part` has), and the resampler
//! from the voices' 32 kHz to the host's rate. Framework-free: plain values in, samples out.
//!
//! **The interface below is the contract with the plugin** (`plugins/mxm-modal-synth`): the plugin
//! fills [`Settings`] from its parameters once per call, sends notes and expressions as they come,
//! and calls [`Instrument::render`] for each stretch of host samples between events. Changing it is
//! a change to both crates. The rules behind each part are in this crate's `AGENTS.md` and ops'
//! `plans/plan-elements.md` §5.

#![forbid(unsafe_code)]

mod envelope;
mod lfo;
mod part;
mod resampler;
mod routing;
mod settings;
mod voice;
mod voices;

#[cfg(test)]
mod conformance;
#[cfg(test)]
mod tests;

pub use mxm_modulation::standard::Offer;

use part::Part;
use resampler::Resampler;
use routing::Routing;
use voice::{BLOCK, DT, Shared};
use voices::Voices;

/// The voices' rate: every voice runs at it, as the module does.
pub const ENGINE_RATE: f32 = 32_000.0;
/// Models: modal, string, strings, the hidden voice.
pub const MODELS: i32 = 4;
/// The host rates the resampler serves, in Hz: [`Instrument::set_sample_rate`] ignores any other,
/// and the plugin refuses to activate at one, rather than play at the wrong speed.
pub const HOST_RATES: std::ops::RangeInclusive<f32> = 8_000.0..=1_536_000.0;
/// Voices in Poly mode.
pub const VOICES: usize = 8;
/// Route slots.
pub const SLOTS: usize = 16;
/// Envelopes, and LFOs, per voice.
pub const ENVELOPES: usize = 3;
pub const LFOS: usize = 3;

/// How the voices take notes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    /// Eight voices; a new note takes a free voice, or the oldest.
    #[default]
    Poly,
    /// One voice; every new note retriggers it.
    Mono,
    /// One voice; a note played over a held one glides to it without a new trigger.
    Legato,
}

/// What drives an envelope (the plan's §5.4).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EnvelopeMode {
    /// The note's gate: attack on note on, release on note off.
    #[default]
    Note,
    /// Only its Trigger target: rising starts the attack, it sustains while high, falling releases.
    Gate,
    /// No trigger: it follows its Input target, rising at its attack rate and falling at its
    /// release rate.
    Follow,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LfoShape {
    #[default]
    Sine,
    Triangle,
    SawUp,
    SawDown,
    Square,
    SampleAndHold,
    SmoothRandom,
}

/// A route's source: nineteen (ops' `plans/plan-elements.md` §5.3 and §5.4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Envelope1,
    Envelope2,
    Envelope3,
    /// High from the end of the attack until the envelope is back at rest or attacks again.
    Envelope1Rise,
    /// The complement of the end of rise: high at rest and through the attack.
    Envelope1Cycle,
    Envelope2Rise,
    Envelope2Cycle,
    Envelope3Rise,
    Envelope3Cycle,
    Lfo1,
    Lfo2,
    Lfo3,
    /// The voice's own exciter envelope, the module's Contour.
    Contour,
    Key,
    Velocity,
    Wheel,
    Pressure,
    Bend,
    Random,
}

impl Source {
    pub const ALL: [Self; 19] = [
        Self::Envelope1,
        Self::Envelope2,
        Self::Envelope3,
        Self::Envelope1Rise,
        Self::Envelope1Cycle,
        Self::Envelope2Rise,
        Self::Envelope2Cycle,
        Self::Envelope3Rise,
        Self::Envelope3Cycle,
        Self::Lfo1,
        Self::Lfo2,
        Self::Lfo3,
        Self::Contour,
        Self::Key,
        Self::Velocity,
        Self::Wheel,
        Self::Pressure,
        Self::Bend,
        Self::Random,
    ];
}

/// A route's target: forty (ops' `plans/plan-elements.md` §5.3 and §5.4): the module's CV jacks,
/// the controls added, the instrument's, and each envelope's six.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// V/OCT and FM, in semitones.
    Pitch,
    /// The module's pressure CV: the exciters' accent.
    Strength,
    Contour,
    Bow,
    BowTimbre,
    Blow,
    Flow,
    BlowTimbre,
    Strike,
    Mallet,
    StrikeTimbre,
    Geometry,
    Brightness,
    Damping,
    Position,
    Space,
    /// Latched at each note: the resonator, or the hidden voice.
    Model,
    Amplitude,
    Pan,
    Lfo1Rate,
    Lfo2Rate,
    Lfo3Rate,
    Envelope1Rise,
    Envelope1Fall,
    Envelope1Time,
    Envelope1Cycle,
    Envelope1Trigger,
    Envelope1Input,
    Envelope2Rise,
    Envelope2Fall,
    Envelope2Time,
    Envelope2Cycle,
    Envelope2Trigger,
    Envelope2Input,
    Envelope3Rise,
    Envelope3Fall,
    Envelope3Time,
    Envelope3Cycle,
    Envelope3Trigger,
    Envelope3Input,
}

impl Target {
    pub const ALL: [Self; 40] = [
        Self::Pitch,
        Self::Strength,
        Self::Contour,
        Self::Bow,
        Self::BowTimbre,
        Self::Blow,
        Self::Flow,
        Self::BlowTimbre,
        Self::Strike,
        Self::Mallet,
        Self::StrikeTimbre,
        Self::Geometry,
        Self::Brightness,
        Self::Damping,
        Self::Position,
        Self::Space,
        Self::Model,
        Self::Amplitude,
        Self::Pan,
        Self::Lfo1Rate,
        Self::Lfo2Rate,
        Self::Lfo3Rate,
        Self::Envelope1Rise,
        Self::Envelope1Fall,
        Self::Envelope1Time,
        Self::Envelope1Cycle,
        Self::Envelope1Trigger,
        Self::Envelope1Input,
        Self::Envelope2Rise,
        Self::Envelope2Fall,
        Self::Envelope2Time,
        Self::Envelope2Cycle,
        Self::Envelope2Trigger,
        Self::Envelope2Input,
        Self::Envelope3Rise,
        Self::Envelope3Fall,
        Self::Envelope3Time,
        Self::Envelope3Cycle,
        Self::Envelope3Trigger,
        Self::Envelope3Input,
    ];
}

/// What a slot's amount of ±1 moves its target by, in the target's own unit: semitones on Pitch
/// (12), models on Model (3, so one slot reaches every one from any), octaves on an LFO's Rate and
/// an envelope's Time (4), the factor's swing on Amplitude (×0…×2: 1), 2 on Space (the module's
/// CV takes it to 2, past the freeze), and 1 on every other target, its whole range (a control,
/// Strength, Pan, an envelope's Rise and Fall as a fraction of their 0.5 ms–30 s range, a gate,
/// an Input). A Key slot reaches per octave of keyboard instead (the collection's standard).
/// Which pairs a slot moves at all is [`Target::offer`].
pub fn reach(target: Target) -> f32 {
    target.reach()
}

/// One route slot. In use when both its source and target are set; the amount is a signed
/// fraction of the target's reach. Two slots on one source and target add.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Slot {
    pub source: Option<Source>,
    pub target: Option<Target>,
    pub amount: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EnvelopeSettings {
    pub attack_s: f32,
    pub decay_s: f32,
    pub sustain: f32,
    pub release_s: f32,
    /// −1 logarithmic, 0 linear, +1 exponential: one curve for every segment.
    pub response: f32,
    pub cycle: bool,
    pub mode: EnvelopeMode,
}

impl Default for EnvelopeSettings {
    fn default() -> Self {
        Self {
            attack_s: 0.005,
            decay_s: 0.3,
            sustain: 0.7,
            release_s: 0.3,
            response: 0.0,
            cycle: false,
            mode: EnvelopeMode::Note,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LfoSettings {
    /// Already resolved by the plugin, tempo sync included.
    pub rate_hz: f32,
    pub shape: LfoShape,
    /// Restart at each note, per voice; otherwise one LFO for all voices, free-running.
    pub retrigger: bool,
    pub fade_s: f32,
}

impl Default for LfoSettings {
    fn default() -> Self {
        Self {
            rate_hz: 5.0,
            shape: LfoShape::Sine,
            retrigger: false,
            fade_s: 0.0,
        }
    }
}

/// Every setting, as plain values; the plugin fills it from its parameters. The module's knobs
/// are in its own units (`Patch`'s, 0 to 1, Space 0 to 1 as its knob), the instrument's in
/// theirs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    /// 0 modal, 1 string, 2 strings, 3 the hidden voice.
    pub model: i32,
    /// Transposition, semitones, and the fine tuning.
    pub coarse_st: f32,
    pub fine_st: f32,
    pub contour: f32,
    pub bow: f32,
    pub bow_timbre: f32,
    pub blow: f32,
    pub flow: f32,
    pub blow_timbre: f32,
    pub strike: f32,
    pub mallet: f32,
    pub strike_timbre: f32,
    pub geometry: f32,
    pub brightness: f32,
    pub damping: f32,
    pub position: f32,
    pub space: f32,
    pub mode: Mode,
    pub glide_s: f32,
    pub bend_range_st: f32,
    /// Linear gain.
    pub volume: f32,
    pub envelopes: [EnvelopeSettings; ENVELOPES],
    pub lfos: [LfoSettings; LFOS],
    pub slots: [Slot; SLOTS],
}

impl Default for Settings {
    /// The module as `Part::Init` sets it: struck, Contour at its sustaining end.
    fn default() -> Self {
        Self {
            model: 0,
            coarse_st: 0.0,
            fine_st: 0.0,
            contour: 1.0,
            bow: 0.0,
            bow_timbre: 0.5,
            blow: 0.0,
            flow: 0.5,
            blow_timbre: 0.5,
            strike: 0.8,
            mallet: 0.5,
            strike_timbre: 0.5,
            geometry: 0.2,
            brightness: 0.5,
            damping: 0.25,
            position: 0.3,
            space: 0.5,
            mode: Mode::Poly,
            glide_s: 0.0,
            bend_range_st: 2.0,
            volume: 1.0,
            envelopes: [EnvelopeSettings::default(); ENVELOPES],
            lfos: [LfoSettings::default(); LFOS],
            slots: [Slot::default(); SLOTS],
        }
    }
}

/// What the editor shows of the sound, read after each render.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Telemetry {
    pub peak: f32,
    pub voices_sounding: usize,
}

/// Volume's smoothing step a block: `1 − e^(−0.5 ms / 5 ms)`, 5 ms.
const SMOOTHING: f32 = 0.095_162_58;

/// A note event, kept until the next render has taken in its settings: an event acts at the
/// next engine block either way, and so it acts under the settings that block plays with (the
/// mode, an LFO's Retrigger and Fade, Glide), not the last call's.
#[derive(Clone, Copy, Debug)]
enum Event {
    On(u8, f32, Option<i32>),
    Off(u8, Option<i32>),
    OffId(i32),
    ReleaseAll,
    Choke(Option<u8>),
    ChokeId(i32),
    Tuning(u8, Option<i32>, f32),
    TuningId(i32, f32),
}

/// Events kept between two renders. Past it the kept ones are applied at once, under the last
/// render's settings, so nothing is lost: only more than 256 events at one sample reach it, as the
/// plugin renders at every event's sample.
const EVENTS: usize = 256;

/// The instrument: everything is allocated in [`Instrument::new`]; nothing after.
///
/// A note event (an on, an off, a choke, a tuning) takes effect at the next engine block, under
/// the settings the latest [`Instrument::render`] brought; the expressions (bend, wheel, pressure)
/// at once.
pub struct Instrument {
    host_rate: f32,
    telemetry: Telemetry,
    /// The last settings, every value finite and in range.
    settings: Settings,
    routing: Routing,
    voices: Voices,
    part: Part,
    events: [Event; EVENTS],
    events_len: usize,
    /// The LFOs with Retrigger off: one each, for every voice.
    lfos: [lfo::Lfo; LFOS],
    /// The sources every voice shares, which the shared LFOs' Rate targets read.
    global: routing::Frame,
    bend: f32,
    wheel: f32,
    pressure: f32,
    /// The current engine block, mixed, and the next frame of it to play.
    block: [[f32; 2]; BLOCK],
    position: usize,
    /// Volume after its smoothing; `None` until the first block, which starts at it.
    volume: Option<f32>,
    /// `None` at 32 kHz.
    resampler: Option<Resampler>,
}

impl Instrument {
    pub fn new() -> Box<Self> {
        Box::new(Self {
            host_rate: ENGINE_RATE,
            telemetry: Telemetry::default(),
            settings: Settings::default(),
            routing: Routing::new(),
            voices: Voices::new(),
            part: Part::new(),
            events: [Event::ReleaseAll; EVENTS],
            events_len: 0,
            lfos: std::array::from_fn(|l| lfo::Lfo::new(0x85eb_ca6b ^ (l as u32 + 1))),
            global: routing::Frame::new(),
            bend: 0.0,
            wheel: 0.0,
            pressure: 0.0,
            block: [[0.0; 2]; BLOCK],
            position: BLOCK,
            volume: None,
            resampler: None,
        })
    }

    /// From the plugin's `activate`: the host's rate. Prepares the resampler; may allocate.
    pub fn set_sample_rate(&mut self, host_rate: f32) {
        if !HOST_RATES.contains(&host_rate) {
            return;
        }
        self.host_rate = host_rate;
        let rate = host_rate.round().min(u32::MAX as f32) as u32;
        self.resampler = (rate != ENGINE_RATE as u32).then(|| Resampler::new(rate));
    }

    /// The host's rate, as `set_sample_rate` last took it.
    pub fn sample_rate(&self) -> f32 {
        self.host_rate
    }

    /// The resampler's fixed latency at the current rate, in host samples; 0 at 32 kHz.
    pub fn latency_samples(&self) -> u32 {
        self.resampler.as_ref().map_or(0, Resampler::latency)
    }

    /// Every voice silent and every state cleared, as `new` made it; allocates nothing. A render
    /// after it is the render after a load.
    pub fn reset(&mut self) {
        self.voices.clear();
        self.part.clear();
        self.events_len = 0;
        for l in &mut self.lfos {
            l.reset();
        }
        self.global.reset();
        self.bend = 0.0;
        self.wheel = 0.0;
        self.pressure = 0.0;
        self.block = [[0.0; 2]; BLOCK];
        self.position = BLOCK;
        self.volume = None;
        if let Some(r) = &mut self.resampler {
            r.reset();
        }
        self.telemetry = Telemetry::default();
    }

    pub fn note_on(&mut self, note: u8, velocity: f32, note_id: Option<i32>) {
        self.push(Event::On(note, velocity, note_id));
    }
    pub fn note_off(&mut self, note: u8, note_id: Option<i32>) {
        self.push(Event::Off(note, note_id));
    }
    /// `None`: every note.
    pub fn choke(&mut self, note: Option<u8>) {
        self.push(Event::Choke(note));
    }
    /// −1 to 1, the lever.
    pub fn set_bend(&mut self, bend: f32) {
        if bend.is_finite() {
            self.bend = mxm_modulation::standard::bend(bend);
        }
    }
    /// 0 to 1.
    pub fn set_wheel(&mut self, wheel: f32) {
        if wheel.is_finite() {
            self.wheel = mxm_modulation::standard::wheel(wheel);
        }
    }
    /// 0 to 1, channel pressure.
    pub fn set_pressure(&mut self, pressure: f32) {
        if pressure.is_finite() {
            self.pressure = mxm_modulation::standard::pressure(pressure);
        }
    }
    /// A per-note pitch expression, in semitones (the conventions' *Per-note pitch expression*).
    pub fn set_tuning(&mut self, note: u8, note_id: Option<i32>, semitones: f32) {
        self.push(Event::Tuning(note, note_id, semitones));
    }

    /// Every voice's gate off, as the note offs of every held key (CC 123).
    pub fn release_all(&mut self) {
        self.push(Event::ReleaseAll);
    }
    /// A note off that names only a note id: the voice playing it, or nothing.
    pub fn note_off_id(&mut self, note_id: i32) {
        self.push(Event::OffId(note_id));
    }
    /// A choke that names only a note id: the voice playing it, or nothing.
    pub fn choke_id(&mut self, note_id: i32) {
        self.push(Event::ChokeId(note_id));
    }
    /// A pitch expression that names only a note id: the voice playing it, or nothing.
    pub fn set_tuning_id(&mut self, note_id: i32, semitones: f32) {
        self.push(Event::TuningId(note_id, semitones));
    }

    fn push(&mut self, event: Event) {
        if self.events_len == EVENTS {
            self.apply_events();
        }
        self.events[self.events_len] = event;
        self.events_len += 1;
    }

    /// The kept events, in order, under the settings taken in.
    fn apply_events(&mut self) {
        let s = &self.settings;
        let v = &mut self.voices;
        for &event in &self.events[..self.events_len] {
            match event {
                Event::On(note, velocity, id) => v.note_on(s, note, velocity, id),
                Event::Off(note, id) => v.note_off(s, note, id),
                Event::OffId(id) => v.note_off_id(s, id),
                Event::ReleaseAll => v.release_all(),
                Event::Choke(note) => v.choke(note),
                Event::ChokeId(id) => v.choke_id(id),
                Event::Tuning(note, id, semitones) => v.set_tuning(note, id, semitones),
                Event::TuningId(id, semitones) => v.set_tuning_id(id, semitones),
            }
        }
        self.events_len = 0;
    }

    /// The kept events applied now, for a test that reads the voices without rendering.
    #[cfg(test)]
    pub(crate) fn apply(&mut self) {
        self.apply_events();
    }

    /// Renders `left.len()` host samples (`right` the same length). The settings are taken in
    /// first; the queued events wait for the next engine block, which applies them under the
    /// settings it plays, however many calls drain the frames of the block before it.
    pub fn render(&mut self, settings: &Settings, left: &mut [f32], right: &mut [f32]) {
        self.absorb(settings);
        let n = left.len().min(right.len());
        let mut peak = 0.0f32;
        if let Some(mut resampler) = self.resampler.take() {
            for (l, r) in left[..n].iter_mut().zip(&mut right[..n]) {
                while resampler.needs_input() {
                    let [a, b] = self.next_frame();
                    resampler.push(a, b);
                }
                (*l, *r) = resampler.output();
                peak = peak.max(l.abs()).max(r.abs());
            }
            self.resampler = Some(resampler);
        } else {
            for (l, r) in left[..n].iter_mut().zip(&mut right[..n]) {
                [*l, *r] = self.next_frame();
                peak = peak.max(l.abs()).max(r.abs());
            }
        }
        left[n..].fill(0.0);
        right[n..].fill(0.0);
        self.telemetry = Telemetry {
            peak,
            voices_sounding: self.voices.voices.iter().filter(|v| v.active).count(),
        };
    }

    pub fn telemetry(&self) -> Telemetry {
        self.telemetry
    }

    /// Whether anything still sounds: a voice, the reverb's tail, the resampler's history. A
    /// note event not yet rendered counts.
    pub fn is_sounding(&self) -> bool {
        self.events_len > 0
            || self.voices.voices.iter().any(|v| v.active)
            || self.part.ringing()
            || self.resampler.as_ref().is_some_and(Resampler::ringing)
    }

    /// Host samples until silence: 0 when nothing sounds, else `u32::MAX`, so the plugin keeps
    /// itself alive: a resonator's ring and the reverb's tail are not counted ahead, only seen to
    /// end ([`Instrument::is_sounding`]).
    pub fn tail_samples(&self) -> u32 {
        if self.is_sounding() { u32::MAX } else { 0 }
    }

    /// The settings taken in: a non-finite value ignored, a change of mode releasing every voice,
    /// the slots' grid rebuilt if a slot changed.
    fn absorb(&mut self, new: &Settings) {
        let mode = self.settings.mode;
        settings::absorb(&mut self.settings, new);
        if self.settings.mode != mode {
            self.voices.release_all();
        }
        self.routing.update(&self.settings.slots);
    }

    /// The next 32 kHz frame, mixed: a new block when the last is played.
    #[inline]
    fn next_frame(&mut self) -> [f32; 2] {
        if self.position == BLOCK {
            self.render_block();
        }
        let frame = self.block[self.position];
        self.position += 1;
        frame
    }

    /// One engine block: the shared sources and LFOs, every sounding voice into the part, the
    /// part's frames read back as samples, Volume.
    fn render_block(&mut self) {
        self.apply_events();
        let s = &self.settings;
        let from = self.volume.unwrap_or(s.volume);
        let to = match self.volume {
            Some(was) => was + (s.volume - was) * SMOOTHING,
            None => s.volume,
        };
        self.volume = Some(to);

        let g = &mut self.global;
        g.begin_sample();
        g.write(Source::Wheel.index(), self.wheel);
        g.write(Source::Pressure.index(), self.pressure);
        g.write(Source::Bend.index(), self.bend);
        let mut lfos = [0.0; LFOS];
        // An LFO each voice runs for itself is not a shared source: zero, never the last value it
        // had while it was shared.
        for l in 0..LFOS {
            if s.lfos[l].retrigger {
                g.write(Source::lfo(l).index(), 0.0);
            }
        }
        for (l, value) in lfos.iter_mut().enumerate() {
            let ls = &s.lfos[l];
            if !ls.retrigger {
                let octaves = self.routing.sum(Target::lfo_rate(l), g);
                *value = self.lfos[l].advance(ls.shape, ls.rate_hz * octaves.exp2(), DT);
                g.write(Source::lfo(l).index(), *value);
            }
        }

        let shared = Shared {
            settings: &self.settings,
            routing: &self.routing,
            wheel: self.wheel,
            pressure: self.pressure,
            bend: self.bend,
            lfos,
        };
        let frames = self.part.process(&shared, &mut self.voices.voices);
        let mut out = [[0.0f32; 2]; BLOCK];
        for (i, (o, f)) in out.iter_mut().zip(&frames).enumerate() {
            let t = (i + 1) as f32 / BLOCK as f32;
            let volume = from + (to - from) * t;
            o[0] = f32::from(f[0]) / 32768.0 * volume;
            o[1] = f32::from(f[1]) / 32768.0 * volume;
        }
        self.block = out;
        self.position = 0;
    }
}
