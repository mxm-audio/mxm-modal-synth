//! MXM Modal Synth: a modal resonator, a string, strings and a dark FM voice, struck, bowed and
//! blown, as a polyphonic instrument, with three envelopes that are function generators too, three
//! LFOs and sixteen route slots (ops' `plans/plan-elements.md` §5). The instrument is
//! `mxm-modal-synth-dsp`'s, behind the interface in its `src/lib.rs`; this crate owns the host's
//! side of it: identity, the 105 parameters, events, the realtime callback,
//! state, the preset system's wiring, the control map and the editor.
//!
//! **Each process call**: the tempo is read and the patch filled into the DSP's `Settings` once;
//! the buffer is split at every event and capped at [`MAX_BLOCK`] samples; for each piece the
//! smoothed signals step on and the instrument renders it. An event takes effect at its own sample.

pub mod editor;
pub mod params;
pub mod preset;
pub mod routes;
pub mod telemetry;

use mxm_modal_synth_dsp::{Instrument, Settings};
use nice_plug::midi::{Key, VoiceID};
use nice_plug::prelude::*;
use params::ModalSynthParams;
use std::sync::Arc;
use telemetry::Telemetry;

/// The plugin's name, and the only place it is written in this crate.
macro_rules! plugin_name {
    () => {
        "mxm-modal-synth"
    };
}

/// What the host displays.
pub const NAME: &str = plugin_name!();

/// The CLAP identifier, built from the name. Permanent from the gate (E5).
pub const CLAP_ID: &str = concat!("dk.mxm.", plugin_name!());

/// The longest stretch rendered between looks at the parameters: an event-free buffer is still
/// many blocks, so a smoothed signal never steps by more than this.
pub const MAX_BLOCK: usize = 64;

/// The developer channel (`docs/plugin-conventions.md`, *A developer channel in every editor*):
/// with `MXM_DEV_CC` set, these CCs are the editor's requests, not the sound's.
const DEV_VIEW_CC: u8 = 119;
const DEV_DISCLOSURE_CC: u8 = 118;
const DEV_BROWSER_CC: u8 = 117;
const DEV_THEME_CC: u8 = 116;
const DEV_CC_ENV: &str = "MXM_DEV_CC";

/// MIDI's keys.
const KEYS: usize = 128;

pub struct MxmModalSynth {
    params: Arc<ModalSynthParams>,
    /// Everything it holds is allocated in `Instrument::new` and `set_sample_rate`.
    instrument: Box<Instrument>,
    /// The patch for this process call, its smoothed signals moved on per block.
    settings: Settings,
    telemetry: Arc<Telemetry>,
    /// How many note-ons each key has had without a note-off: what CC 123 releases.
    held: [u8; KEYS],
    /// The two channels a mono layout renders through before they are summed.
    scratch: [[f32; MAX_BLOCK]; 2],
    dev_cc: bool,
}

impl Default for MxmModalSynth {
    fn default() -> Self {
        let params = Arc::new(ModalSynthParams::default());
        let settings = params.settings(None);
        Self {
            params,
            instrument: Instrument::new(),
            settings,
            telemetry: Telemetry::shared(),
            held: [0; KEYS],
            scratch: [[0.0; MAX_BLOCK]; 2],
            dev_cc: std::env::var_os(DEV_CC_ENV).is_some(),
        }
    }
}

impl MxmModalSynth {
    /// The shared observations an editor reads.
    #[must_use]
    pub fn telemetry(&self) -> Arc<Telemetry> {
        self.telemetry.clone()
    }

    fn note_on(&mut self, key: u8, velocity: f32, voice_id: VoiceID) {
        self.instrument
            .note_on(key, velocity.clamp(0.0, 1.0), voice_id.id());
        let held = &mut self.held[usize::from(key)];
        *held = held.saturating_add(1);
    }

    fn note_off(&mut self, key: Key, voice_id: VoiceID) {
        match key.number() {
            Some(key) => {
                self.instrument.note_off(key, voice_id.id());
                let held = &mut self.held[usize::from(key)];
                *held = held.saturating_sub(1);
            }
            // A key-less note-off names a note by its id, or, as a wildcard, every note.
            None => match voice_id.id() {
                Some(id) if !voice_id.is_wildcard() => self.instrument.note_off_id(id),
                _ => self.all_notes_off(),
            },
        }
    }

    /// CC 123: every held key let go, the tails left to ring.
    fn all_notes_off(&mut self) {
        self.instrument.release_all();
        self.held = [0; KEYS];
    }

    fn handle_event(&mut self, event: NoteEvent<()>) {
        match event {
            // A note-on at no velocity is a note-off, as MIDI has it.
            NoteEvent::NoteOn {
                voice_id,
                key: Key::Number(key),
                velocity,
                ..
            } if velocity.is_finite() => {
                if velocity > 0.0 {
                    self.note_on(key, velocity, voice_id);
                } else {
                    self.note_off(Key::Number(key), voice_id);
                }
            }
            NoteEvent::NoteOff { voice_id, key, .. } => self.note_off(key, voice_id),
            // A wildcard key is every note.
            NoteEvent::Choke { key, voice_id, .. } => match (key.number(), voice_id.id()) {
                (Some(key), _) => {
                    self.instrument.choke(Some(key));
                    self.held[usize::from(key)] = 0;
                }
                (None, Some(id)) if !voice_id.is_wildcard() => self.instrument.choke_id(id),
                (None, _) => {
                    self.instrument.choke(None);
                    self.held = [0; KEYS];
                }
            },
            // Per-note pitch, in semitones, routed by its note; a non-finite one is dropped.
            NoteEvent::PolyTuning {
                voice_id,
                key: Key::Number(key),
                tuning,
                ..
            } if tuning.is_finite() => self.instrument.set_tuning(key, voice_id.id(), tuning),
            // One that names only a note id goes to the voice playing it.
            NoteEvent::PolyTuning {
                voice_id, tuning, ..
            } if tuning.is_finite() && !voice_id.is_wildcard() => {
                if let Some(id) = voice_id.id() {
                    self.instrument.set_tuning_id(id, tuning);
                }
            }
            NoteEvent::MidiPitchBend { value, .. } if value.is_finite() => {
                self.instrument
                    .set_bend(((value - 0.5) * 2.0).clamp(-1.0, 1.0));
            }
            NoteEvent::MidiChannelPressure { pressure, .. } if pressure.is_finite() => {
                self.instrument.set_pressure(pressure.clamp(0.0, 1.0));
            }
            NoteEvent::MidiCC { cc, value, .. } if value.is_finite() => {
                self.control_change(cc, value)
            }
            // Per-note pressure, vibrato, brightness and expression are dropped: only the per-note
            // pitch is accepted (`docs/plugin-conventions.md`, *Per-note pitch expression*).
            _ => {}
        }
    }

    fn control_change(&mut self, cc: u8, value: f32) {
        let value = value.clamp(0.0, 1.0);
        match cc {
            DEV_VIEW_CC if self.dev_cc => {
                self.telemetry
                    .dev
                    .request_view((value * 127.0).round() as u8);
            }
            DEV_DISCLOSURE_CC if self.dev_cc => self.telemetry.dev.request_disclosure(value >= 0.5),
            DEV_BROWSER_CC if self.dev_cc => self.telemetry.dev.request_browser(value >= 0.5),
            DEV_THEME_CC if self.dev_cc => {
                self.telemetry
                    .dev
                    .request_theme((value * 127.0).round() as u8);
            }
            control_change::MODULATION_MSB => self.instrument.set_wheel(value),
            control_change::ALL_SOUND_OFF => {
                self.instrument.choke(None);
                self.held = [0; KEYS];
            }
            control_change::ALL_NOTES_OFF => self.all_notes_off(),
            _ => {}
        }
    }

    /// Renders `start..end` of the buffer, at most [`MAX_BLOCK`] samples, and publishes what it
    /// left.
    fn render(&mut self, output: &mut [&mut [f32]], start: usize, end: usize) {
        let len = end - start;
        self.params.advance(&mut self.settings, len as u32);
        match output {
            [left, right, ..] => {
                self.instrument.render(
                    &self.settings,
                    &mut left[start..end],
                    &mut right[start..end],
                );
            }
            [mono] => {
                let [left, right] = &mut self.scratch;
                self.instrument
                    .render(&self.settings, &mut left[..len], &mut right[..len]);
                for (out, (l, r)) in mono[start..end]
                    .iter_mut()
                    .zip(left.iter().zip(right.iter()))
                {
                    *out = 0.5 * (l + r);
                }
            }
            [] => {
                let [left, right] = &mut self.scratch;
                self.instrument
                    .render(&self.settings, &mut left[..len], &mut right[..len]);
            }
        }
        self.telemetry.publish(self.instrument.telemetry());
    }

    /// **Sounding keeps the host processing; silence lets it sleep.** While a key is held, an
    /// envelope cycles in Gate mode or a note waits, the instrument can't say how long it will
    /// sound (`tail_samples` is `u32::MAX`), so `KeepAlive`; otherwise the tail it counts; and
    /// `Normal` once every voice is idle, when the output is exact zero.
    fn status(&self) -> ProcessStatus {
        if !self.instrument.is_sounding() {
            return ProcessStatus::Normal;
        }
        match self.instrument.tail_samples() {
            u32::MAX => ProcessStatus::KeepAlive,
            samples => ProcessStatus::Tail(samples),
        }
    }
}

impl Plugin for MxmModalSynth {
    const NAME: &'static str = crate::NAME;
    const VENDOR: &'static str = "mxm";
    const URL: &'static str = "https://mxm.dk";
    const EMAIL: &'static str = "plugins@mxm.dk";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    /// An instrument: no main input. Stereo, or mono as the two channels' average.
    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[
        AudioIOLayout {
            main_input_channels: None,
            main_output_channels: NonZeroU32::new(2),
            ..AudioIOLayout::const_default()
        },
        AudioIOLayout {
            main_input_channels: None,
            main_output_channels: NonZeroU32::new(1),
            ..AudioIOLayout::const_default()
        },
    ];

    /// `MidiCCs`: pitch bend, CC 1, channel pressure, CC 120 and CC 123, so a host's panic clears
    /// a held key.
    const MIDI_INPUT: MidiConfig = MidiConfig::MidiCCs;
    const SAMPLE_ACCURATE_AUTOMATION: bool = false;

    type Editor = editor::MxmModalSynthEditor;
    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Self::Editor> {
        editor::create(self.params.clone(), self.telemetry.clone())
    }

    /// The host's rate goes to the instrument, which prepares its resampler (and may allocate:
    /// this is not the audio thread), and **the resampler's latency is reported here, once**, as
    /// nice-plug takes it: the activate context sends it when `activate` returns. It is fixed for
    /// the activation (`docs/plugin-conventions.md`, *Latency is fixed*).
    fn activate(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        context: &mut impl ActivateContext<Self>,
    ) -> bool {
        let rate = buffer_config.sample_rate;
        if !rate.is_finite() || rate <= 0.0 {
            return false;
        }
        self.instrument.set_sample_rate(rate);
        self.instrument.reset();
        context.set_latency_samples(self.instrument.latency_samples());
        self.held = [0; KEYS];
        // A new activation starts with no tempo: the first callback reports it.
        self.telemetry.tempo.publish(None);
        self.settings = self.params.settings(None);
        true
    }

    /// Every voice silent and every state cleared; allocates nothing.
    fn reset(&mut self) {
        self.instrument.reset();
        self.held = [0; KEYS];
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        let samples = buffer.samples();
        // The patch and the LFOs' tempo sync, once per call (`docs/plugin-conventions.md`, *Tempo
        // sync is one button*), and the tempo in force for the editor's reading.
        let tempo = context.transport().tempo;
        self.telemetry.tempo.publish(tempo);
        self.settings = self.params.settings(tempo);

        let mut next_event = context.next_event();
        let mut start = 0;
        while start < samples {
            while let Some(event) = next_event {
                if event.timing() as usize > start {
                    break;
                }
                self.handle_event(event);
                next_event = context.next_event();
            }
            let mut end = (start + MAX_BLOCK).min(samples);
            if let Some(event) = next_event {
                end = end.min(event.timing() as usize);
            }
            self.render(buffer.as_slice(), start, end);
            start = end;
        }
        // nice-plug lets an event sit at the buffer's end, an empty buffer's included: it takes
        // effect at the next sample.
        while let Some(event) = next_event {
            self.handle_event(event);
            next_event = context.next_event();
        }
        self.status()
    }
}

impl ClapPlugin for MxmModalSynth {
    const CLAP_ID: &'static str = CLAP_ID;
    const CLAP_DESCRIPTION: Option<&'static str> = Some(
        "A polyphonic modal synthesizer: bars, plates, strings and tubes, struck, bowed and blown, with function-generator envelopes, LFOs and free modulation routing",
    );
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::Instrument,
        ClapFeature::Synthesizer,
        ClapFeature::Stereo,
        ClapFeature::Mono,
    ];

    // Its factory presets, for a host to list and load (`mxm_preset::clap`).
    const CLAP_PRESETS: Option<ClapPresets> = Some(mxm_preset::clap::PRESETS);

    fn preset_loader(&self) -> Option<Arc<dyn ClapPresetLoader>> {
        Some(Arc::new(mxm_preset::clap::Loader(self.params.clone())))
    }
}

impl Vst3Plugin for MxmModalSynth {
    /// The CLAP id's hash written out, held to it by
    /// `identity::the_vst3_class_id_is_the_clap_ids_hash`.
    const VST3_CLASS_ID: [u8; 16] = [
        0x4f, 0x62, 0xd1, 0x96, 0x31, 0xee, 0x2e, 0xf1, 0x1a, 0x3d, 0xfa, 0xb7, 0x7a, 0x8f, 0x17,
        0xcf,
    ];
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Instrument, Vst3SubCategory::Synth];
}

nice_export_clap!(MxmModalSynth);
nice_export_vst3!(MxmModalSynth);

#[cfg(test)]
mod identity {
    use super::{CLAP_ID, NAME};
    use mxm_plugin_test::formats;

    #[test]
    fn the_vst3_class_id_is_the_clap_ids_hash() {
        formats::the_vst3_class_id_is_the_clap_ids_hash::<super::MxmModalSynth>();
    }

    #[test]
    fn the_vst3_categories_follow_the_clap_features() {
        formats::the_vst3_categories_follow_the_clap_features::<super::MxmModalSynth>();
    }

    #[test]
    fn the_id_is_the_name_under_the_project_domain() {
        assert_eq!(CLAP_ID, format!("dk.mxm.{NAME}"));
    }

    #[test]
    fn the_bundle_is_named_after_this_plugin() {
        mxm_plugin_test::bundle::is_named(env!("CARGO_MANIFEST_DIR"), env!("CARGO_PKG_NAME"), NAME);
    }

    /// The line a host's plugin browser shows names no maker or model, and neither does it name
    /// the module this is a tribute to (root `AGENTS.md`, *The names*).
    #[test]
    fn the_host_description_speaks_to_the_player() {
        mxm_plugin_test::hover_text::host_description_speaks_to_the_player(env!(
            "CARGO_MANIFEST_DIR"
        ));
        let description =
            <super::MxmModalSynth as nice_plug::prelude::ClapPlugin>::CLAP_DESCRIPTION
                .unwrap()
                .to_lowercase();
        for word in [
            "elements",
            "ominous",
            "mutable",
            "maths",
            "make noise",
            "plaits",
        ] {
            assert!(!description.contains(word), "{word}");
        }
    }
}

/// What a player reads on hover in the editor speaks to the player about the sound, never about
/// the module, the machine or the code (`mxm_plugin_test::hover_text`).
#[cfg(test)]
mod speaks_to_the_player {
    #[test]
    fn hover_text() {
        mxm_plugin_test::hover_text::speaks_to_the_player(env!("CARGO_MANIFEST_DIR"));
    }

    /// Nor do they name the module this is a tribute to (root `AGENTS.md`, *The names*).
    #[test]
    fn no_editor_sentence_names_the_original() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = vec![dir.join("editor.rs")];
        for entry in std::fs::read_dir(dir.join("editor")).unwrap().flatten() {
            files.push(entry.path());
        }
        for file in files {
            let text = std::fs::read_to_string(&file).unwrap().to_lowercase();
            for word in [
                "elements",
                "ominous",
                "mutable",
                "maths",
                "make noise",
                "plaits",
            ] {
                assert!(!text.contains(word), "{} names {word:?}", file.display());
            }
        }
    }
}

/// **`control-map.json` holds to the standard and to these parameters**: every role one the
/// standard declares, every reference a parameter here, a stepped role on a stepped parameter.
#[cfg(test)]
mod control_map {
    use super::{CLAP_ID, NAME};
    use crate::params::ModalSynthParams;
    use mxm_control_map::{Curve, InstrumentMap, ParamRef};
    use nice_plug::prelude::Params;

    #[test]
    fn the_map_holds_to_the_standard_and_names_real_parameters() {
        let map = InstrumentMap::parse(include_str!("../control-map.json")).expect("it parses");
        let standard = mxm_control_map::shipped();
        assert_eq!(map.instruments.len(), 1);
        let instrument = &map.instruments[0];
        assert_eq!(instrument.clap_id, CLAP_ID);
        assert_eq!(instrument.name, NAME);
        standard
            .check_instrument(instrument)
            .expect("every role is the standard's");

        let params = ModalSynthParams::default();
        let map = params.param_map();
        assert!(!instrument.params.is_empty());
        for (role, reference) in &instrument.params {
            let ParamRef::Id(id) = reference else {
                panic!("{role}: name a parameter by its id");
            };
            let (_, ptr, _) = map
                .iter()
                .find(|(candidate, ..)| candidate == id)
                .unwrap_or_else(|| panic!("{role} names `{id}`, which is not a parameter"));
            // SAFETY: `params` owns the parameter and outlives the call.
            let stepped = unsafe { ptr.step_count() }.is_some();
            if standard.curve_for(role) == Curve::Stepped {
                assert!(stepped, "{role} is stepped, `{id}` is continuous");
            }
        }
    }

    /// Route amounts are a slot's, not a destination's: a slot can be aimed anywhere, so no
    /// controller role names one.
    #[test]
    fn no_role_names_a_route_slot() {
        let text = include_str!("../control-map.json");
        assert!(!text.contains("\"route"), "a slot moves with its routing");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nice_plug::context::process::SendEventError;
    use nice_plug::midi::Channel;
    use std::collections::VecDeque;

    struct TestContext {
        events: VecDeque<NoteEvent<()>>,
        transport: Transport,
    }

    impl ProcessContext<MxmModalSynth> for TestContext {
        fn request_restart(&self) {}
        fn plugin_api(&self) -> PluginApi {
            PluginApi::Clap
        }
        fn execute_background(&self, _task: ()) {}
        fn execute_gui(&self, _task: ()) {}
        fn transport(&self) -> &Transport {
            &self.transport
        }
        fn next_event(&mut self) -> Option<NoteEvent<()>> {
            self.events.pop_front()
        }
        fn try_send_event(
            &mut self,
            _event: NoteEvent<()>,
        ) -> Result<(), (NoteEvent<()>, SendEventError)> {
            Ok(())
        }
        fn set_latency_samples(&self, _samples: u32) {}
        fn set_current_voice_capacity(&self, _capacity: u32) {}
    }

    /// An activate context that remembers the latency it is told.
    struct Activation {
        latency: std::cell::Cell<Option<u32>>,
    }

    impl ActivateContext<MxmModalSynth> for Activation {
        fn plugin_api(&self) -> PluginApi {
            PluginApi::Clap
        }
        fn execute(&self, _task: ()) {}
        fn set_latency_samples(&self, samples: u32) {
            self.latency.set(Some(samples));
        }
        fn set_current_voice_capacity(&self, _capacity: u32) {}
    }

    fn activated(rate: f32) -> (MxmModalSynth, Option<u32>) {
        let mut plugin = MxmModalSynth::default();
        let mut context = Activation {
            latency: std::cell::Cell::new(None),
        };
        let config = BufferConfig {
            sample_rate: rate,
            min_buffer_size: None,
            max_buffer_size: 1024,
            process_mode: ProcessMode::Realtime,
        };
        assert!(plugin.activate(&MxmModalSynth::AUDIO_IO_LAYOUTS[0], &config, &mut context));
        for (_, ptr, _) in plugin.params.param_map() {
            unsafe { ptr._internal_update_smoother(rate, true) };
        }
        (plugin, context.latency.get())
    }

    fn process(
        plugin: &mut MxmModalSynth,
        channels: usize,
        frames: usize,
        events: impl IntoIterator<Item = NoteEvent<()>>,
        tempo: Option<f64>,
    ) -> (Vec<Vec<f32>>, ProcessStatus) {
        let mut samples = vec![vec![f32::NAN; frames]; channels];
        let mut buffer = Buffer::default();
        unsafe {
            buffer.set_slices(frames, |slices| {
                slices.clear();
                for channel in samples.iter_mut() {
                    slices.push(channel.as_mut_slice());
                }
            });
        }
        let mut inputs = [];
        let mut outputs = [];
        let mut aux = AuxiliaryBuffers {
            inputs: &mut inputs,
            outputs: &mut outputs,
        };
        let mut transport = Transport::new(48_000.0);
        transport.tempo = tempo;
        let mut context = TestContext {
            events: events.into_iter().collect(),
            transport,
        };
        let status = plugin.process(&mut buffer, &mut aux, &mut context);
        drop(buffer);
        (samples, status)
    }

    fn note_on(timing: u32, key: u8) -> NoteEvent<()> {
        NoteEvent::NoteOn {
            timing,
            voice_id: VoiceID::Wildcard,
            channel: Channel::Number(0),
            key: Key::Number(key),
            velocity: 0.8,
        }
    }

    fn note_off(timing: u32, key: u8) -> NoteEvent<()> {
        NoteEvent::NoteOff {
            timing,
            voice_id: VoiceID::Wildcard,
            channel: Channel::Number(0),
            key: Key::Number(key),
            velocity: 0.0,
        }
    }

    fn cc(timing: u32, cc: u8, value: f32) -> NoteEvent<()> {
        NoteEvent::MidiCC {
            timing,
            channel: 0,
            cc,
            value,
        }
    }

    /// **The latency is the instrument's, reported in `activate`** through nice-plug's activate
    /// context, at every host rate.
    #[test]
    fn activate_reports_the_instruments_latency() {
        for rate in [44_100.0, 48_000.0, 96_000.0, 192_000.0] {
            let (plugin, latency) = activated(rate);
            assert_eq!(latency, Some(plugin.instrument.latency_samples()), "{rate}");
        }
        let mut plugin = MxmModalSynth::default();
        let mut context = Activation {
            latency: std::cell::Cell::new(None),
        };
        for rate in [0.0, -1.0, f32::NAN] {
            let config = BufferConfig {
                sample_rate: rate,
                min_buffer_size: None,
                max_buffer_size: 64,
                process_mode: ProcessMode::Realtime,
            };
            assert!(
                !plugin.activate(&MxmModalSynth::AUDIO_IO_LAYOUTS[0], &config, &mut context),
                "a rate of {rate} is refused"
            );
        }
    }

    /// **Every sample of every layout is written**, through events at every offset — before the
    /// first sample, mid-block, past a block cap, at the very end — and in an empty buffer. Says
    /// nothing about the sound: only that the buffer is the instrument's, whole.
    #[test]
    fn every_sample_is_rendered_through_events_at_every_offset() {
        for channels in [2, 1] {
            let (mut plugin, _) = activated(48_000.0);
            let events = [
                note_on(0, 60),
                note_on(1, 64),
                cc(63, 1, 0.5),
                note_on(64, 67),
                NoteEvent::MidiPitchBend {
                    timing: 130,
                    channel: 0,
                    value: 0.75,
                },
                NoteEvent::PolyTuning {
                    timing: 200,
                    voice_id: VoiceID::Wildcard,
                    channel: Channel::Number(0),
                    key: Key::Number(60),
                    tuning: 0.5,
                },
                NoteEvent::MidiChannelPressure {
                    timing: 300,
                    channel: 0,
                    pressure: 0.3,
                },
                note_off(511, 60),
                note_off(1000, 64),
            ];
            let (out, _) = process(&mut plugin, channels, 1000, events, Some(120.0));
            for channel in &out {
                assert!(channel.iter().all(|s| s.is_finite()), "{channels} channels");
            }
            let (out, _) = process(&mut plugin, channels, 0, [note_off(0, 67)], None);
            assert!(out.iter().all(Vec::is_empty));
            assert_eq!(plugin.held.iter().map(|&n| u32::from(n)).sum::<u32>(), 0);
        }
    }

    /// **Held keys are counted, so CC 123 lets each go and CC 120 and a wildcard choke clear them**.
    #[test]
    fn the_keys_held_follow_notes_chokes_and_the_channel_modes() {
        let (mut plugin, _) = activated(48_000.0);
        process(
            &mut plugin,
            2,
            64,
            [note_on(0, 60), note_on(0, 60), note_on(5, 62)],
            None,
        );
        assert_eq!((plugin.held[60], plugin.held[62]), (2, 1));
        process(&mut plugin, 2, 64, [note_off(0, 60)], None);
        assert_eq!(plugin.held[60], 1);
        process(&mut plugin, 2, 64, [cc(0, 123, 1.0)], None);
        assert!(
            plugin.held.iter().all(|&n| n == 0),
            "CC 123 lets every key go"
        );

        process(&mut plugin, 2, 64, [note_on(0, 40), note_on(0, 41)], None);
        process(
            &mut plugin,
            2,
            64,
            [NoteEvent::Choke {
                timing: 0,
                voice_id: VoiceID::Wildcard,
                channel: Channel::Wildcard,
                key: Key::Number(40),
            }],
            None,
        );
        assert_eq!((plugin.held[40], plugin.held[41]), (0, 1));
        process(
            &mut plugin,
            2,
            64,
            [NoteEvent::Choke {
                timing: 0,
                voice_id: VoiceID::Wildcard,
                channel: Channel::Wildcard,
                key: Key::Wildcard,
            }],
            None,
        );
        assert!(
            plugin.held.iter().all(|&n| n == 0),
            "a wildcard is every note"
        );

        process(&mut plugin, 2, 64, [note_on(0, 50)], None);
        process(&mut plugin, 2, 64, [cc(0, 120, 1.0)], None);
        assert!(
            plugin.held.iter().all(|&n| n == 0),
            "CC 120 is all sound off"
        );

        // A note-on at no velocity is a note-off.
        process(&mut plugin, 2, 64, [note_on(0, 70)], None);
        let mut silent = note_on(0, 70);
        if let NoteEvent::NoteOn { velocity, .. } = &mut silent {
            *velocity = 0.0;
        }
        process(&mut plugin, 2, 64, [silent], None);
        assert_eq!(plugin.held[70], 0);
    }

    /// **The status is the instrument's**: `Normal` once silent, otherwise its tail, or
    /// `KeepAlive` while it can't count one.
    #[test]
    fn the_status_follows_the_instrument() {
        let (mut plugin, _) = activated(48_000.0);
        let (_, status) = process(&mut plugin, 2, 64, [], None);
        let expected = if !plugin.instrument.is_sounding() {
            ProcessStatus::Normal
        } else {
            match plugin.instrument.tail_samples() {
                u32::MAX => ProcessStatus::KeepAlive,
                samples => ProcessStatus::Tail(samples),
            }
        };
        assert_eq!(status, expected);
    }

    /// **The tempo in force is published once per call**, and an activation forgets it.
    #[test]
    fn the_tempo_is_published_and_an_activation_forgets_it() {
        let (mut plugin, _) = activated(48_000.0);
        process(&mut plugin, 2, 32, [], Some(97.0));
        assert_eq!(plugin.telemetry.tempo.get(), Some(97.0));
        let mut context = Activation {
            latency: std::cell::Cell::new(None),
        };
        let config = BufferConfig {
            sample_rate: 48_000.0,
            min_buffer_size: None,
            max_buffer_size: 64,
            process_mode: ProcessMode::Realtime,
        };
        assert!(plugin.activate(&MxmModalSynth::AUDIO_IO_LAYOUTS[0], &config, &mut context));
        assert_eq!(plugin.telemetry.tempo.get(), None);
    }

    /// **The developer channel is off unless the environment asked for it**
    /// (`docs/plugin-conventions.md`, *A developer channel in every editor*): without `MXM_DEV_CC`
    /// CC 116–119 request nothing; with it, each is the editor's request, taken once.
    #[test]
    fn the_developer_channel_is_off_unless_the_environment_asked_for_it() {
        let mut plugin = MxmModalSynth {
            dev_cc: false,
            ..Default::default()
        };
        for dev in [DEV_VIEW_CC, DEV_DISCLOSURE_CC, DEV_BROWSER_CC, DEV_THEME_CC] {
            plugin.handle_event(cc(0, dev, 1.0));
        }
        assert_eq!(plugin.telemetry.dev.take_view(), None);
        assert_eq!(plugin.telemetry.dev.take_disclosure(), None);
        assert_eq!(plugin.telemetry.dev.take_browser(), None);
        assert_eq!(plugin.telemetry.dev.take_theme(), None);

        plugin.dev_cc = true;
        plugin.handle_event(cc(0, DEV_VIEW_CC, 1.0 / 127.0));
        plugin.handle_event(cc(0, DEV_DISCLOSURE_CC, 1.0));
        plugin.handle_event(cc(0, DEV_BROWSER_CC, 0.0));
        plugin.handle_event(cc(0, DEV_THEME_CC, 1.0 / 127.0));
        assert_eq!(plugin.telemetry.dev.take_view(), Some(1));
        assert_eq!(plugin.telemetry.dev.take_disclosure(), Some(true));
        assert_eq!(plugin.telemetry.dev.take_browser(), Some(false));
        assert_eq!(plugin.telemetry.dev.take_theme(), Some(1));
    }

    /// The layouts are stereo, then mono, with no input of any kind.
    #[test]
    fn the_layouts_are_stereo_then_mono_with_no_inputs() {
        assert_eq!(MxmModalSynth::AUDIO_IO_LAYOUTS.len(), 2);
        for (i, layout) in MxmModalSynth::AUDIO_IO_LAYOUTS.iter().enumerate() {
            assert!(layout.main_input_channels.is_none());
            assert!(layout.aux_input_ports.is_empty());
            assert_eq!(
                layout.main_output_channels.unwrap().get(),
                if i == 0 { 2 } else { 1 }
            );
        }
    }
}
