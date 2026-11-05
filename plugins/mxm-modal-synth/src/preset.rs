//! The instrument's side of the collection's preset system (`mxm-preset`): its id, version,
//! parameters and identity slot. Init is generated from the parameter defaults and has no file;
//! the factory set is the files in this crate's `presets/`, in `order.txt`'s order, which
//! `cargo xtask bundle` checks and installs for the plugin's version, where the plugin reads them.
//!
//! The factory sounds are designed in the test module's [`tests::design`], as overrides on Init in
//! each parameter's own units, and written to `presets/` by the ignored
//! `write_the_factory_presets`; `the_shipped_files_are_the_design` catches a forgotten regenerate,
//! and `tests/factory_levels.rs` holds the set's loudness.

use crate::params::ModalSynthParams;
use std::sync::RwLock;

pub use mxm_preset::{Category, INIT_NAME, Preset, PresetIdentity, Value};

impl mxm_preset::Instrument for ModalSynthParams {
    fn clap_id(&self) -> &'static str {
        crate::CLAP_ID
    }

    /// The plugin's version, its `Plugin::VERSION`: its factory presets are installed for it.
    fn version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    fn parameters(&self) -> Vec<(&'static str, &dyn mxm_params::ErasedParam)> {
        self.all_parameters()
    }

    fn identity(&self) -> &RwLock<PresetIdentity> {
        &self.preset
    }

    /// The installed set, but in a unit test the repository's, never the machine's.
    #[cfg(test)]
    fn factory_files(&self) -> Vec<(String, String)> {
        mxm_preset::read_factory_dir(&factory_folder())
    }
}

/// This crate's `presets/`: what the bundle installs, and what the tests read.
#[must_use]
pub fn factory_folder() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("presets")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::params::{
        ENVELOPE_IDS, EnvelopeModeChoice, INIT_MODEL, LFO_IDS, LFO_SYNC, LfoShapeChoice,
        MODEL_NAMES, VoiceMode,
    };
    use crate::routes::{ROUTE_IDS, SourceChoice, TargetChoice};
    use mxm_modal_synth_dsp::{SLOTS, Source, Target};
    use mxm_preset::Instrument as _;
    use nice_plug::prelude::{Enum as _, Param as _, ParamSetter, Params as _};

    /// One factory sound: its name, its category, and what it changes from Init, each value in its
    /// parameter's own units (seconds, hertz, a fraction, a model's or a list's index).
    type Design = (&'static str, Category, Vec<(&'static str, f32)>);

    fn source(source: Source) -> f32 {
        SourceChoice(Some(source)).to_index() as f32
    }

    fn target(target: Target) -> f32 {
        TargetChoice(Some(target)).to_index() as f32
    }

    /// Mallet in each of its four regions, at `t` (0 to 1) inside it: the voice maps Mallet `m` to
    /// `m ≤ 0.4 ? 0.625 m : 1.25 m − 0.25` and splits that in quarters, so the recorded hits are
    /// below 0.4, the mallet 0.4 to 0.6, the plectrum 0.6 to 0.8 and the particles above.
    fn hits(t: f32) -> f32 {
        0.4 * t
    }
    fn mallet(t: f32) -> f32 {
        0.4 + 0.2 * t
    }
    fn plectrum(t: f32) -> f32 {
        0.6 + 0.2 * t
    }
    fn particles(t: f32) -> f32 {
        0.8 + 0.1995 * t
    }

    /// Geometry at the middle of the strings' chord `k` of eleven (octaves, minor seventh, minor,
    /// minor ninth, minor eleventh, fifths, major eleventh, major ninth, major, major seventh,
    /// suspended fourth): the voice picks `round(10 × Geometry)`.
    fn chord(k: u8) -> f32 {
        f32::from(k) / 10.0
    }

    /// A factory sound as it is written: Init with each change made, its routes filling the slots
    /// from the first.
    struct Sound {
        name: &'static str,
        category: Category,
        set: Vec<(&'static str, f32)>,
        routes: usize,
    }

    impl Sound {
        fn new(name: &'static str, category: Category, model: usize) -> Self {
            Self {
                name,
                category,
                set: vec![("model", model as f32)],
                routes: 0,
            }
        }

        fn set(mut self, id: &'static str, plain: f32) -> Self {
            self.set.push((id, plain));
            self
        }

        /// The bow's level and timbre.
        fn bow(self, level: f32, timbre: f32) -> Self {
            self.set("bow", level).set("bow_timbre", timbre)
        }

        /// The blow's level, flow and timbre.
        fn blow(self, level: f32, flow: f32, timbre: f32) -> Self {
            self.set("blow", level)
                .set("flow", flow)
                .set("blow_timbre", timbre)
        }

        /// The strike's level, mallet and timbre.
        fn strike(self, level: f32, mallet: f32, timbre: f32) -> Self {
            self.set("strike", level)
                .set("mallet", mallet)
                .set("strike_timbre", timbre)
        }

        /// The resonator's Geometry, Brightness, Damping and Position.
        fn resonator(self, geometry: f32, brightness: f32, damping: f32, position: f32) -> Self {
            self.set("geometry", geometry)
                .set("brightness", brightness)
                .set("damping", damping)
                .set("position", position)
        }

        fn contour(self, contour: f32) -> Self {
            self.set("contour", contour)
        }

        fn space(self, space: f32) -> Self {
            self.set("space", space)
        }

        fn voice(self, mode: VoiceMode) -> Self {
            self.set("mode", mode.to_index() as f32)
        }

        /// Envelope `n`'s attack, decay, sustain and release.
        fn env(self, n: usize, attack: f32, decay: f32, sustain: f32, release: f32) -> Self {
            let [a, d, s, r, ..] = ENVELOPE_IDS[n - 1];
            self.set(a, attack)
                .set(d, decay)
                .set(s, sustain)
                .set(r, release)
        }

        /// Envelope `n` cycles: attack and decay repeat, an LFO.
        fn cycling(self, n: usize) -> Self {
            self.set(ENVELOPE_IDS[n - 1][5], 1.0)
        }

        fn env_mode(self, n: usize, mode: EnvelopeModeChoice) -> Self {
            self.set(ENVELOPE_IDS[n - 1][6], mode.to_index() as f32)
        }

        fn lfo(self, n: usize, rate_hz: f32, shape: LfoShapeChoice) -> Self {
            let [rate, form, ..] = LFO_IDS[n - 1];
            self.set(rate, rate_hz).set(form, shape.to_index() as f32)
        }

        /// LFO `n` follows the host's tempo at `division`; free, without a tempo, at the rate whose
        /// position picks it.
        fn synced(self, n: usize, division: mxm_tempo::Division) -> Self {
            let [rate, _, sync, ..] = LFO_IDS[n - 1];
            let position = LFO_SYNC.position(division);
            let hz = ModalSynthParams::default()
                .lfo1
                .rate
                .preview_plain(position);
            self.set(rate, hz).set(sync, 1.0)
        }

        /// LFO `n` restarts with each note, each voice its own.
        fn retriggered(self, n: usize) -> Self {
            self.set(LFO_IDS[n - 1][3], 1.0)
        }

        fn fade(self, n: usize, seconds: f32) -> Self {
            self.set(LFO_IDS[n - 1][4], seconds)
        }

        /// A route in the next free slot.
        fn route(mut self, from: Source, to: Target, amount: f32) -> Self {
            assert!(
                self.routes < SLOTS,
                "{} has more routes than slots",
                self.name
            );
            let [s, t, a] = ROUTE_IDS[self.routes];
            self.routes += 1;
            self.set(s, source(from)).set(t, target(to)).set(a, amount)
        }

        /// **The strike follows velocity**: Velocity into Strength, so a soft note is up to 15 dB
        /// under a hard one, the accent the module's pressure CV gives.
        fn touch(self) -> Self {
            self.route(Velocity, Strength, 0.5)
        }

        /// **The sound's own level**, in decibels from the module's accent: Envelope 3, held for
        /// the note, into Strength at `dB / 30` (the accent spans 30 dB across Strength's range,
        /// `10^(1.5 (s − 0.5))`), so the exciters are played harder or softer and the set is
        /// balanced in the sound, never by Volume. Strength stops at its top, +15 dB; a larger
        /// value only reaches it sooner while the envelope rises, which is when a strike lands
        /// (`Plectrum tick`'s 20). The accent returns to rest within 0.3 s of the
        /// release: a strike is over by then and a sustained exciter only fades the sooner, but
        /// one cut by the accent would swell, so a bowed or blown sound is never cut by it (its
        /// levels and its contour set it; `designed` holds that).
        fn accent(self, db: f32) -> Self {
            self.env(3, 0.0005, 0.1, 1.0, 0.3)
                .route(Envelope3, Strength, db / 30.0)
        }

        /// A slow vibrato on LFO 1, faded in.
        fn vibrato(self, depth: f32) -> Self {
            self.lfo(1, 5.0, Triangle)
                .fade(1, 0.5)
                .route(Lfo1, Pitch, depth)
        }

        fn designed(self) -> Design {
            let value = |id: &str| {
                self.set
                    .iter()
                    .rev()
                    .find(|(set, _)| *set == id)
                    .map(|(_, v)| *v)
            };
            let sustained = value("bow").unwrap_or(0.0) > 0.0 || value("blow").unwrap_or(0.0) > 0.0;
            let cut = ROUTE_IDS.iter().any(|[s, t, a]| {
                value(s) == Some(source(Envelope3))
                    && value(t) == Some(target(Strength))
                    && value(a).is_some_and(|a| a < 0.0)
            });
            assert!(
                !(sustained && cut),
                "{} is bowed or blown and cut by the accent: it would swell on release",
                self.name
            );
            (self.name, self.category, self.set)
        }
    }

    use Category::{Bass, Brass, Drone, Fx, Keys, Lead, Pad, Percussion, Pluck, Sequence, Strings};
    use EnvelopeModeChoice::Follow;
    use LfoShapeChoice::{SampleAndHold, SmoothRandom, Square, Triangle};
    use Source::{
        Contour, Envelope1, Envelope2, Envelope3, Key, Lfo1, Lfo2, Lfo3, Pressure, Random, Velocity,
    };
    use Target::{
        Bow, Brightness, Damping, Envelope2Input, Flow, Geometry, Mallet, Model, Pan, Pitch,
        Position, Space, Strength,
    };

    /// The models, as `params::MODEL_NAMES` numbers them.
    const MODAL: usize = 0;
    const STRING: usize = 1;
    const STRINGS: usize = 2;
    const DARK: usize = 3;

    /// **The factory sounds, readable**: a first impression, then by category, sorted into
    /// `Category::ALL`'s order for the browser (stable, so each category keeps the order written
    /// here; `the_browser_order_is_a_first_impression_then_the_categories`). Every model has sounds that show what it does best: the modal resonator struck,
    /// bowed and blown, the string plucked and bowed, the strings' chords, the dark FM voice; the
    /// modulation the instrument adds to the module is shown off along the way, each such sound
    /// saying what it shows. Each one's loudness is held by `tests/factory_levels.rs`, and is set
    /// in the sound, never by Volume.
    pub(super) fn design() -> Vec<Design> {
        let sounds = vec![
            // The first impression: glassy struck bars, velocity on the strike, a slow LFO
            // moving where they are heard.
            Sound::new("Glass keys", Keys, MODAL)
                .accent(2.0)
                .strike(0.8, mallet(0.5), 0.6)
                .resonator(0.25, 0.65, 0.45, 0.3)
                .space(0.75)
                .lfo(2, 0.3, Triangle)
                .touch()
                .route(Lfo2, Position, 0.15),
            // Keys.
            Sound::new("Struck bars", Keys, MODAL)
                .accent(7.0)
                .strike(0.8, mallet(0.3), 0.35)
                .resonator(0.3, 0.4, 0.35, 0.25)
                .space(0.6)
                .touch(),
            Sound::new("Celesta box", Keys, MODAL)
                .accent(-0.7)
                .strike(0.75, mallet(0.6), 0.75)
                .resonator(0.55, 0.75, 0.5, 0.35)
                .space(0.7)
                .touch(),
            Sound::new("Toy piano", Keys, MODAL)
                .accent(4.5)
                .strike(0.75, mallet(0.7), 0.8)
                .resonator(0.65, 0.7, 0.35, 0.2)
                .space(0.55)
                .touch(),
            Sound::new("Soft mallets", Keys, MODAL)
                .accent(7.0)
                .strike(0.85, mallet(0.1), 0.2)
                .resonator(0.2, 0.3, 0.5, 0.3)
                .space(0.65)
                .touch(),
            // A shared LFO pans each note's tines: the instrument's routes on the module's sound.
            Sound::new("Electric tines", Keys, MODAL)
                .accent(2.5)
                .strike(0.8, mallet(0.45), 0.55)
                .resonator(0.18, 0.55, 0.55, 0.15)
                .space(0.7)
                .lfo(1, 3.5, Triangle)
                .touch()
                .route(Lfo1, Pan, 0.35),
            Sound::new("Tubular bells", Keys, MODAL)
                .accent(-0.3)
                .strike(0.8, mallet(0.5), 0.6)
                .resonator(0.6, 0.6, 0.62, 0.4)
                .space(0.8)
                .touch(),
            Sound::new("Small bells", Keys, MODAL)
                .accent(0.3)
                .strike(0.75, mallet(0.6), 0.7)
                .resonator(0.75, 0.8, 0.6, 0.35)
                .space(0.85)
                .touch(),
            // Key into Brightness: low notes dark, high ones bright, as a real set of bars.
            Sound::new("Wood and metal", Keys, MODAL)
                .accent(5.0)
                .strike(0.8, mallet(0.35), 0.45)
                .resonator(0.45, 0.45, 0.45, 0.3)
                .space(0.65)
                .touch()
                .route(Key, Brightness, 0.6)
                .route(Key, Geometry, 0.3),
            // Percussion.
            Sound::new("Wooden bars", Percussion, MODAL)
                .accent(11.5)
                .strike(0.85, mallet(0.25), 0.4)
                .resonator(0.3, 0.45, 0.2, 0.3)
                .space(0.55)
                .touch(),
            Sound::new("Metal plate", Percussion, MODAL)
                .accent(7.0)
                .strike(0.8, mallet(0.5), 0.6)
                .resonator(0.8, 0.7, 0.4, 0.25)
                .space(0.7)
                .touch(),
            Sound::new("Hand drum", Percussion, MODAL)
                .accent(10.0)
                .strike(0.9, hits(0.85), 0.35)
                .resonator(0.08, 0.3, 0.15, 0.4)
                .space(0.55)
                .touch(),
            Sound::new("Tuned can", Percussion, MODAL)
                .accent(-0.2)
                .strike(0.85, hits(0.5), 0.5)
                .resonator(0.7, 0.55, 0.25, 0.3)
                .space(0.5)
                .touch(),
            Sound::new("Knock", Percussion, MODAL)
                .accent(9.0)
                .strike(0.9, hits(0.7), 0.3)
                .resonator(0.4, 0.2, 0.1, 0.45)
                .space(0.5)
                .touch(),
            // Random into Position and Brightness: no two hits alike.
            Sound::new("Loose hits", Percussion, MODAL)
                .accent(-0.1)
                .strike(0.85, hits(0.3), 0.5)
                .resonator(0.5, 0.5, 0.3, 0.3)
                .space(0.6)
                .touch()
                .route(Random, Position, 0.3)
                .route(Random, Brightness, 0.2),
            Sound::new("Plectrum tick", Percussion, STRING)
                .accent(20.0)
                .strike(0.85, plectrum(0.9), 0.8)
                .resonator(0.3, 0.65, 0.35, 0.2)
                .space(0.5)
                .touch(),
            Sound::new("Struck strings", Percussion, STRINGS)
                .accent(8.5)
                .strike(0.85, mallet(0.6), 0.5)
                .resonator(chord(5), 0.5, 0.35, 0.3)
                .space(0.65)
                .touch(),
            // Pluck.
            Sound::new("Plucked harp", Pluck, STRING)
                .accent(-6.0)
                .strike(0.8, plectrum(0.5), 0.5)
                .resonator(0.25, 0.6, 0.6, 0.3)
                .space(0.7)
                .touch(),
            Sound::new("Nylon pluck", Pluck, STRING)
                .accent(-5.0)
                .strike(0.8, plectrum(0.3), 0.4)
                .resonator(0.25, 0.45, 0.55, 0.25)
                .space(0.55)
                .touch(),
            Sound::new("Steel pluck", Pluck, STRING)
                .accent(-1.0)
                .strike(0.8, plectrum(0.6), 0.7)
                .resonator(0.3, 0.7, 0.6, 0.2)
                .space(0.6)
                .touch(),
            // Low Geometry curves the string's bridge: a buzz as it rings.
            Sound::new("Buzzing pluck", Pluck, STRING)
                .accent(-4.0)
                .strike(0.8, plectrum(0.5), 0.55)
                .resonator(0.05, 0.6, 0.55, 0.3)
                .space(0.6)
                .touch(),
            Sound::new("Strummed chord", Pluck, STRINGS)
                .accent(2.5)
                .strike(0.8, plectrum(0.5), 0.55)
                .resonator(chord(8), 0.6, 0.6, 0.3)
                .space(0.7)
                .touch(),
            Sound::new("Mallet pluck", Pluck, MODAL)
                .accent(7.0)
                .strike(0.8, mallet(0.8), 0.6)
                .resonator(0.22, 0.55, 0.4, 0.15)
                .space(0.6)
                .touch(),
            // Pad.
            Sound::new("Bowed glass", Pad, MODAL)
                .strike(0.0, mallet(0.5), 0.5)
                .bow(0.44, 0.5)
                .contour(0.6)
                .resonator(0.45, 0.6, 0.7, 0.3)
                .space(0.9)
                .lfo(2, 0.2, SmoothRandom)
                .route(Lfo2, Position, 0.2),
            Sound::new("Breath pad", Pad, MODAL)
                .strike(0.0, mallet(0.5), 0.5)
                .blow(0.09, 0.3, 0.4)
                .contour(0.6)
                .resonator(0.3, 0.5, 0.65, 0.3)
                .space(0.95),
            Sound::new("Tube choir", Pad, MODAL)
                .strike(0.0, mallet(0.5), 0.5)
                .blow(0.75, 0.5, 0.25)
                .contour(0.6)
                .resonator(0.2, 0.35, 0.45, 0.3)
                .space(0.9),
            // An envelope as a function generator: a slow swell of the bow under each chord.
            Sound::new("Swelling bow", Pad, MODAL)
                .strike(0.0, mallet(0.5), 0.5)
                .bow(0.33, 0.45)
                .contour(0.55)
                .resonator(0.35, 0.55, 0.7, 0.3)
                .space(0.9)
                .env(1, 1.5, 1.0, 0.8, 1.5)
                .route(Envelope1, Bow, 0.3),
            Sound::new("Frozen air", Pad, MODAL)
                .strike(0.0, mallet(0.5), 0.5)
                .blow(0.12, 0.8, 0.7)
                .contour(0.6)
                .resonator(0.6, 0.6, 0.7, 0.3)
                .space(0.98)
                .lfo(2, 0.15, Triangle)
                .route(Lfo2, Geometry, 0.15),
            Sound::new("Dark FM pad", Pad, DARK)
                .strike(0.6, 0.35, 0.3)
                .blow(0.7, 0.3, 0.3)
                .contour(0.6)
                .resonator(0.5, 0.4, 0.5, 0.4)
                .space(0.8),
            Sound::new("Chord pad", Pad, STRINGS)
                .accent(10.0)
                .strike(0.0, mallet(0.5), 0.5)
                .bow(0.6, 0.5)
                .contour(0.6)
                .resonator(chord(3), 0.55, 0.65, 0.3)
                .space(0.9),
            // Strings.
            Sound::new("Bowed cello", Strings, STRING)
                .strike(0.0, mallet(0.5), 0.5)
                .bow(0.8, 0.45)
                .contour(0.6)
                .resonator(0.25, 0.5, 0.55, 0.3)
                .space(0.6)
                .vibrato(0.02),
            Sound::new("Bowed ensemble", Strings, STRINGS)
                .accent(8.0)
                .strike(0.0, mallet(0.5), 0.5)
                .bow(0.7, 0.5)
                .contour(0.65)
                .resonator(chord(1), 0.55, 0.65, 0.3)
                .space(0.85),
            Sound::new("Glass strings", Strings, MODAL)
                .strike(0.0, mallet(0.5), 0.5)
                .bow(0.43, 0.6)
                .contour(0.6)
                .resonator(0.2, 0.65, 0.65, 0.25)
                .space(0.8)
                .vibrato(0.015),
            // Pressure leans on the bow.
            Sound::new("Leaning bow", Strings, STRING)
                .accent(2.0)
                .strike(0.0, mallet(0.5), 0.5)
                .bow(0.5, 0.5)
                .contour(0.6)
                .resonator(0.3, 0.5, 0.55, 0.3)
                .space(0.7)
                .route(Pressure, Bow, 0.4)
                .route(Pressure, Brightness, 0.2),
            // Lead.
            Sound::new("Flute lead", Lead, MODAL)
                .accent(0.8)
                .strike(0.0, mallet(0.5), 0.5)
                .blow(0.6, 0.25, 0.45)
                .contour(0.7)
                .resonator(0.15, 0.55, 0.5, 0.3)
                .space(0.6)
                .voice(VoiceMode::Legato)
                .set("glide", 0.06)
                .vibrato(0.02),
            Sound::new("Reed lead", Lead, MODAL)
                .strike(0.0, mallet(0.5), 0.5)
                .blow(0.75, 0.4, 0.22)
                .contour(0.7)
                .resonator(0.2, 0.38, 0.45, 0.3)
                .space(0.55)
                .voice(VoiceMode::Mono)
                .vibrato(0.015),
            Sound::new("Bowed lead", Lead, STRING)
                .strike(0.0, mallet(0.5), 0.5)
                .bow(0.85, 0.55)
                .contour(0.65)
                .resonator(0.25, 0.55, 0.5, 0.3)
                .space(0.6)
                .voice(VoiceMode::Legato)
                .set("glide", 0.05)
                .vibrato(0.025),
            Sound::new("Dark FM lead", Lead, DARK)
                .strike(0.6, 0.3, 0.3)
                .blow(1.0, 0.5, 0.5)
                .contour(0.7)
                .resonator(0.43, 0.7, 0.4, 0.3)
                .space(0.6)
                .voice(VoiceMode::Mono)
                .set("glide", 0.04),
            Sound::new("Glass whistle", Lead, MODAL)
                .strike(0.0, mallet(0.5), 0.5)
                .blow(0.17, 0.1, 0.8)
                .contour(0.7)
                .resonator(0.5, 0.8, 0.5, 0.3)
                .space(0.65)
                .voice(VoiceMode::Mono),
            // Bass.
            Sound::new("Plucked bass", Bass, STRING)
                .accent(7.5)
                .strike(0.85, plectrum(0.4), 0.4)
                .resonator(0.25, 0.35, 0.5, 0.3)
                .space(0.35)
                .set("coarse", -12.0)
                .voice(VoiceMode::Mono)
                .touch(),
            Sound::new("Wood bass", Bass, MODAL)
                .accent(10.5)
                .strike(0.85, mallet(0.25), 0.35)
                .resonator(0.2, 0.3, 0.4, 0.3)
                .space(0.35)
                .set("coarse", -12.0)
                .voice(VoiceMode::Mono)
                .touch(),
            Sound::new("Dark FM bass", Bass, DARK)
                .strike(1.0, 0.25, 0.5)
                .blow(0.8, 0.25, 0.4)
                .contour(0.65)
                .resonator(0.35, 0.55, 0.3, 0.3)
                .space(0.3)
                .set("coarse", -12.0)
                .voice(VoiceMode::Mono),
            Sound::new("Bowed bass", Bass, STRING)
                .accent(13.0)
                .strike(0.0, mallet(0.5), 0.5)
                .bow(0.8, 0.4)
                .contour(0.5)
                .resonator(0.25, 0.4, 0.5, 0.3)
                .space(0.35)
                .set("coarse", -12.0)
                .voice(VoiceMode::Mono),
            Sound::new("Thump bass", Bass, MODAL)
                .accent(13.0)
                .strike(0.9, hits(0.9), 0.3)
                .resonator(0.1, 0.25, 0.25, 0.4)
                .space(0.35)
                .set("coarse", -12.0)
                .voice(VoiceMode::Mono)
                .touch(),
            // Brass.
            // A second envelope opens the brightness as the note speaks, as a horn's does.
            Sound::new("Tube brass", Brass, MODAL)
                .strike(0.0, mallet(0.5), 0.5)
                .blow(0.85, 0.6, 0.5)
                .contour(0.7)
                .resonator(0.15, 0.45, 0.4, 0.3)
                .space(0.6)
                .env(2, 0.08, 0.4, 0.6, 0.2)
                .route(Envelope2, Brightness, 0.3),
            Sound::new("Muted horn", Brass, MODAL)
                .strike(0.0, mallet(0.5), 0.5)
                .blow(0.85, 0.5, 0.35)
                .contour(0.7)
                .resonator(0.2, 0.4, 0.45, 0.3)
                .space(0.55),
            // Drone.
            Sound::new("Dark FM drone", Drone, DARK)
                .strike(1.0, 0.4, 0.4)
                .blow(1.0, 0.35, 0.4)
                .contour(0.6)
                .resonator(0.38, 0.6, 0.5, 0.6)
                .space(0.9)
                .lfo(3, 0.08, SmoothRandom)
                .route(Lfo3, Flow, 0.1)
                .route(Lfo3, Mallet, -0.1),
            Sound::new("Singing bowl", Drone, MODAL)
                .accent(0.8)
                .strike(0.3, mallet(0.3), 0.3)
                .bow(0.6, 0.4)
                .contour(0.55)
                .resonator(0.85, 0.45, 0.7, 0.3)
                .space(0.9),
            Sound::new("Bowed metal drone", Drone, MODAL)
                .strike(0.0, mallet(0.5), 0.5)
                .bow(0.46, 0.5)
                .contour(0.6)
                .resonator(0.9, 0.5, 0.7, 0.35)
                .space(0.95),
            Sound::new("Buzzing drone", Drone, STRINGS)
                .accent(6.0)
                .strike(0.0, mallet(0.5), 0.5)
                .bow(0.4, 0.6)
                .contour(0.6)
                .resonator(chord(0), 0.75, 0.7, 0.3)
                .space(0.7),
            // Sequence.
            // A synced square on the bow: it pulls in time with the host.
            Sound::new("Pulsing bow", Sequence, MODAL)
                .accent(4.0)
                .strike(0.5, mallet(0.4), 0.4)
                .bow(0.4, 0.5)
                .contour(0.6)
                .resonator(0.4, 0.55, 0.6, 0.3)
                .space(0.7)
                .synced(1, mxm_tempo::Division::Sixteenth)
                .set(LFO_IDS[0][1], Square.to_index() as f32)
                .route(Lfo1, Bow, 0.4),
            // A sample-and-hold steps the Geometry: every note a new shape.
            Sound::new("Stepped shapes", Sequence, MODAL)
                .accent(5.0)
                .strike(0.8, mallet(0.5), 0.55)
                .resonator(0.4, 0.55, 0.4, 0.3)
                .space(0.65)
                .synced(2, mxm_tempo::Division::Eighth)
                .set(LFO_IDS[1][1], SampleAndHold.to_index() as f32)
                .touch()
                .route(Lfo2, Geometry, 0.3),
            // An envelope cycling as an LFO, retriggered by the note, sweeping Position.
            Sound::new("Cycling position", Sequence, STRING)
                .accent(-4.0)
                .strike(0.8, plectrum(0.4), 0.5)
                .resonator(0.25, 0.55, 0.5, 0.3)
                .space(0.6)
                .env(2, 0.05, 0.08, 0.5, 0.1)
                .cycling(2)
                .touch()
                .route(Envelope2, Position, 0.3),
            // Fx.
            Sound::new("Particle rain", Fx, MODAL)
                .accent(2.0)
                .strike(0.8, particles(0.6), 0.6)
                .resonator(0.6, 0.6, 0.5, 0.3)
                .space(0.95),
            Sound::new("Wind tunnel", Fx, MODAL)
                .strike(0.0, mallet(0.5), 0.5)
                .blow(0.05, 0.9, 0.8)
                .contour(0.6)
                .resonator(0.7, 0.55, 0.6, 0.35)
                .space(0.98),
            Sound::new("Space gong", Fx, MODAL)
                .accent(8.5)
                .strike(0.85, mallet(0.4), 0.3)
                .resonator(0.95, 0.4, 0.65, 0.3)
                .space(1.0),
            // A slot on Model picks each note's model: the softer the note, the further it moves
            // from the modal resonator to the string and the strings.
            Sound::new("Model by touch", Fx, MODAL)
                .accent(6.0)
                .strike(0.8, plectrum(0.2), 0.55)
                .resonator(0.3, 0.55, 0.45, 0.3)
                .space(0.7)
                .route(Velocity, Model, -0.7),
            // Follow: a lag on the contour, so the brightness lingers after each swell.
            Sound::new("Lingering glow", Fx, MODAL)
                .accent(13.0)
                .strike(0.0, mallet(0.5), 0.5)
                .bow(0.6, 0.5)
                .contour(0.5)
                .resonator(0.5, 0.4, 0.65, 0.3)
                .space(0.85)
                .env(2, 0.5, 0.3, 0.7, 1.5)
                .env_mode(2, Follow)
                .route(Contour, Envelope2Input, 1.0)
                .route(Envelope2, Brightness, 0.4),
            // An envelope cycling while the note is held: the bow breathes, and so does the room.
            Sound::new("Breathing metal", Fx, MODAL)
                .strike(0.0, mallet(0.5), 0.5)
                .bow(0.35, 0.5)
                .contour(0.6)
                .resonator(0.8, 0.5, 0.65, 0.3)
                .space(0.9)
                .env(1, 0.6, 0.6, 0.0, 0.6)
                .cycling(1)
                .route(Envelope1, Bow, 0.4)
                .route(Envelope1, Space, 0.1),
            Sound::new("Ghost voice", Fx, DARK)
                .strike(1.0, 0.45, 0.45)
                .blow(1.0, 0.45, 0.45)
                .contour(0.65)
                .resonator(0.4, 0.65, 0.55, 0.5)
                .space(0.85)
                .lfo(1, 0.3, SmoothRandom)
                .retriggered(1)
                .route(Lfo1, Flow, 0.2)
                .route(Lfo1, Pan, 0.4),
            Sound::new("Damped thuds", Fx, MODAL)
                .accent(14.0)
                .strike(0.9, hits(0.95), 0.4)
                .resonator(0.6, 0.45, 0.35, 0.3)
                .space(0.75)
                .touch()
                .route(Velocity, Damping, 0.3),
        ];
        let mut sounds = sounds;
        let rank = |c: Category| Category::ALL.iter().position(|&a| a == c);
        sounds[1..].sort_by_key(|s| rank(s.category));
        sounds.into_iter().map(Sound::designed).collect()
    }

    /// The file a sound is written to.
    fn file_name(name: &str) -> String {
        format!("{}.json", name.to_lowercase().replace(' ', "-"))
    }

    /// A design as the preset it is: Init, with each override converted to its parameter's
    /// normalised value and every text the plugin's own, read in the preset's state.
    fn generated(params: &ModalSynthParams, (name, category, overrides): &Design) -> Preset {
        let map = params.param_map();
        let mut preset = Preset::init(params);
        preset.name = (*name).to_owned();
        preset.category = *category;
        for (id, plain) in overrides {
            let (_, ptr, _) = map
                .iter()
                .find(|(candidate, ..)| candidate == id)
                .unwrap_or_else(|| panic!("{name:?} names unknown parameter `{id}`"));
            // SAFETY: `params` owns the parameter and outlives the call.
            let normalised = unsafe { ptr.preview_normalized(*plain) };
            preset
                .params
                .get_mut(*id)
                .unwrap_or_else(|| panic!("{id} is not in Init"))
                .v = normalised;
        }
        mxm_preset::refresh_texts(params, &mut preset);
        preset
    }

    fn shipped() -> Vec<(String, String)> {
        ModalSynthParams::default().factory_files()
    }

    fn model_of(overrides: &[(&str, f32)]) -> usize {
        overrides
            .iter()
            .find(|(id, _)| *id == "model")
            .map_or(INIT_MODEL, |(_, v)| *v as i32) as usize
    }

    #[test]
    #[ignore = "writes the factory preset files"]
    fn write_the_factory_presets() {
        let params = ModalSynthParams::default();
        let folder = factory_folder();
        std::fs::create_dir_all(&folder).unwrap();
        let mut order = String::new();
        for design in design() {
            let file = file_name(design.0);
            std::fs::write(folder.join(&file), generated(&params, &design).to_json()).unwrap();
            order.push_str(&file);
            order.push('\n');
        }
        std::fs::write(folder.join(mxm_preset::ORDER_FILE), order).unwrap();
    }

    /// **The shipped files are the design**, in its order: a design changed without regenerating
    /// fails here.
    #[test]
    fn the_shipped_files_are_the_design() {
        let params = ModalSynthParams::default();
        let files = shipped();
        let designs = design();
        assert_eq!(
            files
                .iter()
                .map(|(file, _)| file.clone())
                .collect::<Vec<_>>(),
            designs.iter().map(|d| file_name(d.0)).collect::<Vec<_>>(),
            "regenerate: cargo test -p mxm-modal-synth --lib write_the_factory_presets -- --ignored"
        );
        for ((file, json), design) in files.iter().zip(&designs) {
            assert_eq!(
                Preset::parse(json, crate::CLAP_ID).unwrap(),
                generated(&params, design),
                "regenerate {file}"
            );
        }
    }

    /// **Every stored `text` is this plugin's own formatting of the `v` beside it**
    /// (`mxm_preset::stale_factory_texts`).
    #[test]
    fn every_factory_text_is_this_plugins_formatting_of_its_value() {
        let stale = mxm_preset::stale_factory_texts(&ModalSynthParams::default());
        assert!(stale.is_empty(), "{}", stale.join("\n"));
    }

    /// Every file is this plugin's, names every parameter, is categorised and is a sound of its
    /// own.
    #[test]
    fn every_file_is_complete_distinct_categorised_and_owned() {
        let params = ModalSynthParams::default();
        let mut seen = Vec::new();
        assert!(!shipped().is_empty(), "no factory files were read");
        for (file, json) in shipped() {
            let preset =
                Preset::parse(&json, crate::CLAP_ID).unwrap_or_else(|e| panic!("{file}: {e}"));
            assert_ne!(preset.category, Category::Uncategorised, "{file}");
            let (_, problems) = preset.resolve(&params);
            assert!(problems.is_empty(), "{file}: {problems:?}");
            assert_eq!(preset.params.len(), params.all_parameters().len(), "{file}");
            assert!(!seen.contains(&preset.params), "{file} repeats a sound");
            seen.push(preset.params);
        }
    }

    /// **Every file loads as a host loads it** (`mxm_preset::clap::load_file`, the CLAP preset
    /// loader's path), and marks itself loaded by its name.
    #[test]
    fn every_file_loads_through_the_hosts_loader() {
        let params = ModalSynthParams::default();
        let host = mxm_plugin_test::NoHost;
        let setter = ParamSetter::new(&host);
        for (file, json) in shipped() {
            let path = factory_folder().join(&file);
            assert_eq!(
                mxm_preset::clap::load_file(&params, &path, &setter),
                Ok(()),
                "{file}"
            );
            let name = Preset::parse(&json, crate::CLAP_ID).unwrap().name;
            assert_eq!(mxm_preset::loaded(&params).name(), Some(name.as_str()));
        }
    }

    /// **The browser's order is a first impression, then `Category::ALL`'s.**
    #[test]
    fn the_browser_order_is_a_first_impression_then_the_categories() {
        let rank = |c: Category| Category::ALL.iter().position(|&a| a == c);
        let ranks: Vec<_> = design().iter().skip(1).map(|(_, c, _)| rank(*c)).collect();
        assert!(ranks.windows(2).all(|w| w[0] <= w[1]), "{ranks:?}");
    }

    /// `order.txt` names every file once, and nothing else.
    #[test]
    fn the_order_names_every_file_once() {
        let folder = factory_folder();
        let order = std::fs::read_to_string(folder.join(mxm_preset::ORDER_FILE)).unwrap();
        let mut listed: Vec<String> = order.lines().map(str::to_owned).collect();
        let mut files: Vec<String> = std::fs::read_dir(&folder)
            .unwrap()
            .flatten()
            .filter_map(|e| e.file_name().into_string().ok())
            .filter(|name| name.ends_with(".json"))
            .collect();
        listed.sort();
        files.sort();
        assert_eq!(listed, files);
    }

    /// **Fifty sounds is the floor** (mxm-kit's `docs/plugin-conventions.md`, *A factory set is
    /// content*).
    #[test]
    fn the_set_is_at_least_fifty_sounds() {
        assert!(design().len() >= 50, "{} sounds", design().len());
    }

    /// **Every model has a sound**: the browser opens on each part of the instrument.
    #[test]
    fn every_model_has_a_sound() {
        let mut models = [false; MODEL_NAMES.len()];
        for (_, _, overrides) in design() {
            models[model_of(&overrides)] = true;
        }
        let missing: Vec<&str> = MODEL_NAMES
            .iter()
            .zip(models)
            .filter(|(_, has)| !has)
            .map(|(name, _)| *name)
            .collect();
        assert!(missing.is_empty(), "no sound plays {missing:?}");
    }

    /// **The names are the set's own**: sentence case, short, and naming neither the module, its
    /// hidden voice, its maker, the function generator it takes after nor its maker (root
    /// `AGENTS.md`, *The names*).
    #[test]
    fn every_name_is_its_own_short_and_in_sentence_case() {
        for (name, ..) in design() {
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
            assert!(name.len() <= 20, "{name:?} is long");
            let mut chars = name.chars();
            assert!(chars.next().is_some_and(char::is_uppercase), "{name:?}");
            // A later word starts in lower case, unless it is an initialism (FM).
            let title_cased = |word: &str| {
                word.chars().next().is_some_and(char::is_uppercase)
                    && word.chars().any(char::is_lowercase)
            };
            assert!(
                !name.split(' ').skip(1).any(title_cased),
                "{name:?} is not sentence case"
            );
        }
    }

    /// **Init is every parameter's default, and the browser's first row**: the generated Init and
    /// the CLAP defaults are one set of numbers.
    #[test]
    fn init_is_every_parameter_default_and_factory_begins_with_it() {
        let params = ModalSynthParams::default();
        let init = Preset::init(&params);
        assert_eq!(init.params.len(), params.all_parameters().len());
        for (id, parameter) in params.all_parameters() {
            assert_eq!(init.params[id].v, parameter.default_normalised(), "{id}");
        }
        let all = mxm_preset::factory(&params);
        assert_eq!(all[0].name, INIT_NAME);
        assert_eq!(all.len(), 1 + shipped().len());
    }

    #[test]
    fn factory_content_does_not_move_volume() {
        let params = ModalSynthParams::default();
        let default = params.volume.default_normalised();
        for (file, json) in shipped() {
            let preset = Preset::parse(&json, crate::CLAP_ID).unwrap();
            assert_eq!(preset.params["volume"].v, default, "{file}");
        }
    }

    #[test]
    fn user_files_are_namespaced_to_the_permanent_id() {
        if let Some(root) = mxm_preset::user_root(crate::CLAP_ID) {
            assert!(root.to_string_lossy().contains(crate::CLAP_ID));
        }
    }

    use mxm_params::ErasedParam as _;
}
