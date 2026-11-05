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
        ENVELOPE_IDS, INIT_MODEL, LFO_IDS, LFO_SYNC, LfoShapeChoice, MODEL_NAMES, VoiceMode,
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
        /// value only reaches it sooner while the envelope rises, which is when a strike lands.
        /// The accent returns to rest within 0.3 s of the release: a strike is over by then and a sustained exciter only fades the sooner, but
        /// one cut by the accent would swell, so a bowed or blown sound is never cut by it (its
        /// levels and its contour set it; `designed` holds that).
        fn accent(self, db: f32) -> Self {
            self.env(3, 0.0005, 0.1, 1.0, 0.3)
                .route(Envelope3, Strength, db / 30.0)
        }

        /// Coarse tuning, in semitones.
        fn coarse(self, semitones: f32) -> Self {
            self.set("coarse", semitones)
        }

        /// One voice, every note retriggered.
        fn mono(self) -> Self {
            self.voice(VoiceMode::Mono)
        }

        /// One voice, a note played over a held one gliding to it in `glide` seconds.
        fn legato(self, glide: f32) -> Self {
            self.voice(VoiceMode::Legato).set("glide", glide)
        }

        /// **A pitch drop**: Envelope 1 starts the note `semitones` up and falls back in
        /// `seconds`, a slot for each octave of it (a slot on Pitch reaches 12).
        fn drop(self, semitones: f32, seconds: f32) -> Self {
            let mut sound = self.env(1, 0.0005, seconds, 0.0, seconds);
            let mut left = semitones;
            while left > 0.0 {
                let step = left.min(12.0);
                sound = sound.route(Envelope1, Pitch, step / 12.0);
                left -= step;
            }
            sound
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
    use LfoShapeChoice::{SampleAndHold, SmoothRandom, Square, Triangle};
    use Source::{
        Envelope1, Envelope2, Envelope3, Key, Lfo1, Lfo2, Lfo3, Pressure, Random, Velocity,
    };
    use Target::{
        Blow, BlowTimbre, Bow, BowTimbre, Brightness, Flow, Geometry, Model, Pan, Pitch, Position,
        Space, Strength, StrikeTimbre,
    };

    /// The models, as `params::MODEL_NAMES` numbers them.
    const MODAL: usize = 0;
    const STRING: usize = 1;
    const STRINGS: usize = 2;
    const DARK: usize = 3;

    /// **The factory sounds, readable**: a first impression, then by category, sorted into
    /// `Category::ALL`'s order for the browser (stable, so each category keeps the order written
    /// here; `the_browser_order_is_a_first_impression_then_the_categories`). **Each is one idea
    /// pushed to its end** (the owner, 2026-11-05: "They sound the same. Be more creative. go
    /// wild."): the controls at their extremes, the reverb frozen past Space's 1.75, the bridge
    /// buzzing, the plectrum's flam, particles re-exciting a held string, chords that change by
    /// themselves, a model chosen per note, pitch drops, and every register; every model is
    /// there, and each sound's comment says what it does. Each one's loudness is held by `tests/factory_levels.rs`, and is set
    /// in the sound, never by Volume.
    pub(super) fn design() -> Vec<Design> {
        let sounds = vec![
            // The first impression: bells whose room freezes while they are held, an envelope
            // carrying Space past 1.75, where the reverb stops taking input and rings on.
            Sound::new("Frozen bells", Keys, MODAL)
                .accent(6.0)
                .strike(0.8, mallet(0.9), 0.8)
                .resonator(0.92, 0.8, 0.6, 0.15)
                .space(0.9)
                .env(1, 1.2, 0.1, 1.0, 1.5)
                .route(Envelope1, Space, 0.45)
                .touch(),
            // Keys.
            Sound::new("Gamelan", Keys, MODAL)
                .strike(0.8, mallet(1.0), 0.9)
                .resonator(0.78, 0.7, 0.6, 0.33)
                .space(0.6)
                .route(Random, Pitch, 0.01)
                .touch(),
            // Key into Geometry: every key a different preparation.
            Sound::new("Prepared piano", Keys, MODAL)
                .accent(-4.0)
                .strike(0.85, hits(0.2), 0.6)
                .resonator(0.32, 0.55, 0.55, 0.07)
                .space(0.3)
                .route(Key, Geometry, 0.3)
                .route(Random, Position, 0.1)
                .touch(),
            Sound::new("Tin music box", Keys, MODAL)
                .strike(0.75, mallet(1.0), 1.0)
                .resonator(0.66, 1.0, 0.5, 0.5)
                .coarse(24.0)
                .space(0.8)
                .route(Random, Pitch, 0.02)
                .touch(),
            // The pickup at the bridge, and velocity on the brightness: a funky clavinet.
            Sound::new("Clav string", Keys, STRING)
                .accent(15.0)
                .strike(1.0, plectrum(0.05), 0.9)
                .resonator(0.15, 0.85, 0.4, 0.08)
                .space(0.12)
                .route(Velocity, Brightness, 0.3)
                .touch(),
            // Velocity into the FM depth: soft notes round, hard ones glassy.
            Sound::new("Dark FM piano", Keys, DARK)
                .strike(0.4, 0.5, 0.35)
                .blow(0.3, 0.25, 0.2)
                .bow(0.0, 0.1)
                .contour(0.2)
                .resonator(0.15, 0.45, 0.6, 0.3)
                .space(0.4)
                .route(Velocity, StrikeTimbre, 0.4),
            Sound::new("Tin plate keys", Keys, MODAL)
                .strike(0.8, mallet(0.7), 0.7)
                .resonator(0.02, 0.8, 0.5, 0.25)
                .space(0.55)
                .touch(),
            // Percussion.
            // A pitch dropping two octaves in 80 ms, and nothing bright: a kick.
            Sound::new("Deep kick", Percussion, MODAL)
                .accent(15.0)
                .strike(0.9, mallet(0.0), 0.0)
                .resonator(0.4, 0.05, 0.2, 0.5)
                .coarse(-24.0)
                .space(0.1)
                .drop(24.0, 0.08)
                .touch(),
            // Particles on a plate: a rattling burst, a snare of sorts.
            Sound::new("Clang snare", Percussion, MODAL)
                .accent(15.0)
                .strike(0.9, particles(0.9), 0.9)
                .resonator(0.05, 0.9, 0.22, 0.25)
                .space(0.3)
                .touch(),
            Sound::new("Tin hat", Percussion, MODAL)
                .accent(15.0)
                .strike(1.0, particles(1.0), 1.0)
                .resonator(1.0, 1.0, 0.15, 0.2)
                .coarse(12.0)
                .space(0.2)
                .touch(),
            Sound::new("Log drum", Percussion, MODAL)
                .accent(8.0)
                .strike(0.9, mallet(0.2), 0.3)
                .resonator(0.25, 0.3, 0.35, 0.3)
                .coarse(-12.0)
                .space(0.35)
                .drop(2.0, 0.05)
                .touch(),
            // The plectrum at its top strikes twice, 130 ms apart: a flam.
            Sound::new("Flam tom", Percussion, MODAL)
                .accent(10.0)
                .strike(0.9, plectrum(1.0), 0.4)
                .resonator(0.45, 0.4, 0.3, 0.3)
                .coarse(-12.0)
                .space(0.35)
                .touch(),
            Sound::new("Rattlesnake", Percussion, STRING)
                .accent(15.0)
                .strike(1.0, particles(0.5), 0.8)
                .resonator(0.0, 0.9, 0.45, 0.1)
                .space(0.3)
                .touch(),
            // Random into Position and Geometry: no two shards alike.
            Sound::new("Glass shards", Percussion, MODAL)
                .accent(4.0)
                .strike(0.85, hits(0.0), 0.9)
                .resonator(0.95, 1.0, 0.3, 0.3)
                .coarse(12.0)
                .space(0.7)
                .route(Random, Position, 0.5)
                .route(Random, Geometry, 0.1)
                .touch(),
            // Pluck.
            // Geometry at its floor curves the bridge: the string buzzes against it.
            Sound::new("Sitar", Pluck, STRING)
                .accent(5.0)
                .strike(0.8, plectrum(0.15), 0.7)
                .resonator(0.01, 0.8, 0.65, 0.08)
                .space(0.4)
                .touch(),
            Sound::new("Koto", Pluck, STRING)
                .accent(4.0)
                .strike(0.8, plectrum(0.3), 0.75)
                .resonator(0.3, 0.6, 0.6, 0.15)
                .space(0.5)
                .lfo(1, 5.5, Triangle)
                .fade(1, 0.4)
                .route(Lfo1, Pitch, 0.03)
                .touch(),
            // A major seventh from every key.
            Sound::new("Chord harp", Pluck, STRINGS)
                .accent(2.0)
                .strike(0.8, plectrum(0.4), 0.6)
                .resonator(chord(9), 0.7, 0.7, 0.3)
                .space(0.75)
                .touch(),
            Sound::new("Kalimba", Pluck, MODAL)
                .strike(0.85, hits(0.6), 0.6)
                .resonator(0.18, 0.5, 0.5, 0.15)
                .space(0.45)
                .touch(),
            // A pitch that droops three semitones as it rings: boing.
            Sound::new("Rubber band", Pluck, STRING)
                .accent(2.0)
                .strike(0.85, plectrum(0.6), 0.5)
                .resonator(0.6, 0.2, 0.4, 0.5)
                .space(0.3)
                .drop(3.0, 0.12)
                .touch(),
            // Particles keep exciting the string while the key is held: shimmering dust.
            Sound::new("Dust harp", Pluck, STRING)
                .strike(0.7, particles(0.3), 0.7)
                .resonator(0.4, 0.7, 0.75, 0.25)
                .space(0.85)
                .touch(),
            // Bass.
            Sound::new("Buzz bass", Bass, STRING)
                .accent(15.0)
                .strike(1.0, plectrum(0.2), 0.6)
                .resonator(0.0, 0.6, 0.5, 0.1)
                .coarse(-12.0)
                .space(0.15)
                .mono()
                .touch(),
            // An envelope opening the filter on each note, the feedback biting: acid.
            Sound::new("Acid FM bass", Bass, DARK)
                .strike(0.0, 0.25, 0.5)
                .blow(0.35, 0.5, 0.6)
                .bow(0.0, 0.5)
                .contour(0.7)
                .resonator(0.1, 0.2, 0.3, 0.3)
                .coarse(-12.0)
                .space(0.1)
                .legato(0.05)
                .env(2, 0.0005, 0.25, 0.0, 0.1)
                .route(Envelope2, Brightness, 0.5),
            Sound::new("Wooden sub", Bass, MODAL)
                .accent(10.0)
                .strike(0.9, mallet(0.1), 0.2)
                .resonator(0.35, 0.1, 0.4, 0.4)
                .coarse(-24.0)
                .space(0.05)
                .mono()
                .touch(),
            Sound::new("Slap bass", Bass, STRING)
                .accent(15.0)
                .strike(1.0, plectrum(0.25), 0.7)
                .resonator(0.2, 0.9, 0.5, 0.1)
                .coarse(-12.0)
                .space(0.1)
                .mono()
                .route(Velocity, Brightness, 0.3)
                .touch(),
            Sound::new("Bowed contrabass", Bass, STRING)
                .accent(15.0)
                .strike(0.0, mallet(0.5), 0.5)
                .bow(0.9, 0.3)
                .contour(0.5)
                .resonator(0.25, 0.4, 0.45, 0.3)
                .coarse(-12.0)
                .space(0.2)
                .mono(),
            // Feedback at its top and a synced LFO on the filter's mode: a growl that wobbles.
            Sound::new("Growl bass", Bass, DARK)
                .strike(0.8, 0.25, 0.5)
                .blow(0.8, 0.25, 0.4)
                .bow(0.0, 0.95)
                .contour(0.7)
                .resonator(0.3, 0.5, 0.3, 0.3)
                .coarse(-24.0)
                .space(0.1)
                .mono()
                .synced(1, mxm_tempo::Division::Eighth)
                .route(Lfo1, Geometry, 0.3),
            // Lead.
            Sound::new("Screaming FM", Lead, DARK)
                .strike(0.25, 0.5, 0.8)
                .blow(0.25, 0.5, 0.8)
                .bow(0.0, 1.0)
                .contour(0.7)
                .resonator(0.0, 0.8, 0.4, 0.3)
                .space(0.35)
                .legato(0.12)
                .vibrato(0.03)
                .route(Pressure, BowTimbre, -0.3),
            // A long glide and a wide vibrato on the plainest oscillator.
            Sound::new("Theremin", Lead, DARK)
                .strike(0.0, 0.0, 0.0)
                .blow(0.32, 0.0, 0.0)
                .bow(0.0, 0.0)
                .contour(0.75)
                .resonator(0.1, 0.9, 0.4, 0.3)
                .space(0.45)
                .legato(0.4)
                .lfo(1, 5.5, Triangle)
                .fade(1, 0.6)
                .route(Lfo1, Pitch, 0.04),
            // The pickup at the middle hears odd modes only: hollow, like a pipe.
            Sound::new("Pan flute", Lead, MODAL)
                .accent(5.0)
                .strike(0.0, mallet(0.5), 0.5)
                .blow(0.66, 0.5, 0.3)
                .contour(0.75)
                .resonator(0.45, 0.4, 0.5, 0.5)
                .space(0.5)
                .mono()
                .vibrato(0.015),
            Sound::new("Duck call", Lead, MODAL)
                .strike(0.0, mallet(0.5), 0.5)
                .blow(0.72, 0.4, 0.25)
                .contour(0.8)
                .resonator(0.55, 0.4, 0.4, 0.05)
                .space(0.25)
                .mono()
                .drop(2.0, 0.15),
            Sound::new("Glass harmonica", Lead, MODAL)
                .strike(0.0, mallet(0.5), 0.5)
                .bow(0.5, 0.7)
                .contour(0.65)
                .resonator(0.88, 0.85, 0.7, 0.3)
                .space(0.7)
                .legato(0.08)
                .vibrato(0.02),
            // Brass.
            Sound::new("Tube fanfare", Brass, MODAL)
                .strike(0.0, mallet(0.5), 0.5)
                .blow(0.85, 0.6, 0.55)
                .contour(0.8)
                .resonator(0.15, 0.5, 0.4, 0.3)
                .space(0.45)
                .env(2, 0.04, 0.3, 0.6, 0.2)
                .route(Envelope2, Brightness, 0.3),
            // A long glide on a dark tube: a slide trombone.
            Sound::new("Slide trombone", Brass, MODAL)
                .strike(0.0, mallet(0.5), 0.5)
                .blow(0.85, 0.5, 0.3)
                .contour(0.75)
                .resonator(0.2, 0.35, 0.4, 0.3)
                .space(0.3)
                .legato(0.15),
            // An envelope on the FM depth: the bite at the start of a horn's note.
            Sound::new("FM horn", Brass, DARK)
                .strike(0.0, 0.25, 0.3)
                .blow(0.3, 0.25, 0.3)
                .bow(0.0, 0.2)
                .contour(0.75)
                .resonator(0.1, 0.55, 0.4, 0.3)
                .space(0.35)
                .env(2, 0.03, 0.25, 0.3, 0.2)
                .route(Envelope2, BlowTimbre, 0.4),
            // Pad.
            // Noise blown into a metal resonator, the room freezing as the chord swells.
            Sound::new("Freeze bloom", Pad, MODAL)
                .strike(0.0, mallet(0.5), 0.5)
                .blow(0.08, 0.9, 0.6)
                .contour(0.6)
                .resonator(0.7, 0.6, 0.6, 0.3)
                .space(0.9)
                .env(1, 2.0, 0.1, 1.0, 1.5)
                .route(Envelope1, Space, 0.5),
            // A slow LFO on Geometry: the strings' chord changes by itself.
            Sound::new("Chord cycler", Pad, STRINGS)
                .accent(10.0)
                .strike(0.0, mallet(0.5), 0.5)
                .bow(0.6, 0.5)
                .contour(0.6)
                .resonator(chord(5), 0.6, 0.75, 0.3)
                .space(0.9)
                .lfo(1, 0.25, Triangle)
                .route(Lfo1, Geometry, 0.5),
            // Position and Geometry wandering together: vowels.
            Sound::new("Breathing choir", Pad, MODAL)
                .strike(0.0, mallet(0.5), 0.5)
                .blow(0.18, 0.2, 0.4)
                .contour(0.6)
                .resonator(0.55, 0.5, 0.7, 0.5)
                .space(0.95)
                .lfo(2, 0.15, SmoothRandom)
                .route(Lfo2, Position, 0.4)
                .route(Lfo2, Geometry, 0.1),
            // A wobbling pitch, as worn tape: wow and flutter.
            Sound::new("Tape chords", Pad, STRINGS)
                .accent(15.0)
                .strike(0.0, mallet(0.5), 0.5)
                .bow(0.8, 0.4)
                .contour(0.6)
                .resonator(chord(6), 0.35, 0.7, 0.3)
                .space(0.85)
                .lfo(1, 0.8, SmoothRandom)
                .route(Lfo1, Pitch, 0.02),
            // The second oscillator detuned and the voice spinning: a swarm.
            Sound::new("Dark FM swarm", Pad, DARK)
                .strike(0.2, 0.5, 0.4)
                .blow(0.2, 0.5, 0.3)
                .bow(0.8, 0.3)
                .contour(0.6)
                .resonator(0.2, 0.55, 0.5, 1.0)
                .space(0.95),
            // Space at nothing: the raw breath on the left, the resonator on the right.
            Sound::new("Split breath", Pad, MODAL)
                .strike(0.0, mallet(0.5), 0.5)
                .blow(0.17, 0.6, 0.6)
                .contour(0.6)
                .resonator(0.4, 0.5, 0.7, 0.3)
                .space(0.0),
            // Strings.
            // The bow just over its threshold, rough on a bell: scraped metal.
            Sound::new("Scraped metal", Strings, MODAL)
                .strike(0.0, mallet(0.5), 0.5)
                .bow(0.45, 1.0)
                .contour(0.6)
                .resonator(0.97, 0.6, 0.7, 0.3)
                .space(0.8),
            // Played at the bridge: thin and glassy.
            Sound::new("Sul ponticello", Strings, STRING)
                .strike(0.0, mallet(0.5), 0.5)
                .bow(0.35, 0.9)
                .contour(0.65)
                .resonator(0.25, 0.95, 0.55, 0.02)
                .space(0.6)
                .vibrato(0.025),
            Sound::new("Minor ninths", Strings, STRINGS)
                .accent(9.0)
                .strike(0.0, mallet(0.5), 0.5)
                .bow(0.7, 0.5)
                .contour(0.65)
                .resonator(chord(3), 0.55, 0.7, 0.3)
                .space(0.85),
            // A synced square on the bow: tremolo bowing in time.
            Sound::new("Tremolo cello", Strings, STRING)
                .strike(0.0, mallet(0.5), 0.5)
                .bow(0.7, 0.45)
                .contour(0.6)
                .resonator(0.25, 0.5, 0.55, 0.3)
                .space(0.55)
                .synced(1, mxm_tempo::Division::Sixteenth)
                .set(LFO_IDS[0][1], Square.to_index() as f32)
                .route(Lfo1, Bow, 0.5),
            // Drone.
            // A tube an octave down, its brightness and timbre wandering: a didgeridoo.
            Sound::new("Didgeridoo", Drone, MODAL)
                .strike(0.0, mallet(0.5), 0.5)
                .blow(0.8, 0.3, 0.4)
                .contour(0.7)
                .resonator(0.5, 0.25, 0.5, 0.12)
                .coarse(-12.0)
                .space(0.4)
                .lfo(3, 0.7, SmoothRandom)
                .route(Lfo3, BlowTimbre, 0.3)
                .route(Lfo3, Brightness, 0.15),
            Sound::new("Singing bowl", Drone, MODAL)
                .strike(0.4, mallet(0.3), 0.3)
                .bow(0.55, 0.4)
                .contour(0.6)
                .resonator(0.8, 0.4, 0.8, 0.3)
                .space(0.9)
                .lfo(2, 0.3, Triangle)
                .route(Lfo2, Position, 0.3),
            // Particles plucking the strings over and over: a tanpura on fifths.
            Sound::new("Tanpura", Drone, STRINGS)
                .accent(6.0)
                .strike(0.8, particles(0.4), 0.7)
                .resonator(chord(5), 0.8, 0.65, 0.3)
                .space(0.8),
            // The FM ratio drifting very slowly under a spinning voice.
            Sound::new("FM nebula", Drone, DARK)
                .strike(0.8, 0.4, 0.5)
                .blow(0.8, 0.35, 0.5)
                .bow(0.6, 0.6)
                .contour(0.6)
                .resonator(0.3, 0.6, 0.5, 0.8)
                .space(0.95)
                .lfo(3, 0.05, SmoothRandom)
                .route(Lfo3, Flow, 0.2),
            Sound::new("Wind organ", Drone, MODAL)
                .strike(0.0, mallet(0.5), 0.5)
                .blow(0.12, 1.0, 0.7)
                .contour(0.6)
                .resonator(0.5, 0.6, 0.65, 0.5)
                .coarse(-12.0)
                .space(0.95),
            // Sequence.
            // A sample-and-hold on Geometry, in time: every note a different chord.
            Sound::new("Chord stepper", Sequence, STRINGS)
                .accent(3.0)
                .strike(0.8, plectrum(0.4), 0.6)
                .resonator(chord(5), 0.6, 0.55, 0.3)
                .space(0.6)
                .synced(2, mxm_tempo::Division::Eighth)
                .set(LFO_IDS[1][1], SampleAndHold.to_index() as f32)
                .route(Lfo2, Geometry, 0.5)
                .touch(),
            // Random into Model: every note a different instrument.
            Sound::new("Model roulette", Sequence, STRING)
                .strike(0.8, plectrum(0.3), 0.6)
                .resonator(0.35, 0.6, 0.45, 0.3)
                .space(0.5)
                .route(Random, Model, 0.67)
                .touch(),
            // An envelope cycling at 33 Hz on the breath: each note a ratchet of puffs.
            Sound::new("Ratchet breath", Sequence, MODAL)
                .strike(0.0, mallet(0.5), 0.5)
                .blow(0.15, 0.5, 0.6)
                .contour(0.8)
                .resonator(0.6, 0.6, 0.6, 0.3)
                .space(0.6)
                .env(1, 0.01, 0.02, 0.0, 0.02)
                .cycling(1)
                .route(Envelope1, Blow, 0.5),
            // Random into Pitch: each note lands somewhere in an octave either side.
            Sound::new("Pitch rain", Sequence, MODAL)
                .strike(0.8, mallet(0.8), 0.7)
                .resonator(0.6, 0.7, 0.45, 0.3)
                .space(0.7)
                .route(Random, Pitch, 1.0)
                .route(Random, Geometry, 0.2)
                .touch(),
            // Fx.
            // Two octaves down in 0.4 s on a fed-back FM voice: a laser.
            Sound::new("Laser", Fx, DARK)
                .strike(0.25, 0.5, 0.7)
                .blow(0.25, 0.5, 0.6)
                .bow(0.0, 0.7)
                .contour(0.75)
                .resonator(0.05, 0.85, 0.4, 0.3)
                .space(0.5)
                .drop(24.0, 0.4),
            // A sample-and-hold on Pitch at 12 Hz: the voice babbles while the key is held.
            Sound::new("Computer babble", Fx, DARK)
                .strike(0.0, 0.5, 0.3)
                .blow(0.27, 0.5, 0.3)
                .bow(0.0, 0.2)
                .contour(0.75)
                .resonator(0.05, 0.7, 0.4, 0.3)
                .space(0.4)
                .lfo(1, 12.0, SampleAndHold)
                .route(Lfo1, Pitch, 1.0),
            // A plate bowed two octaves down: thunder.
            Sound::new("Thunder sheet", Fx, MODAL)
                .strike(0.0, mallet(0.5), 0.5)
                .bow(0.6, 0.2)
                .contour(0.6)
                .resonator(0.0, 0.25, 0.8, 0.3)
                .coarse(-24.0)
                .space(0.85)
                .lfo(2, 0.5, SmoothRandom)
                .route(Lfo2, Bow, 0.3),
            // One strike, and the room frozen on it while the key is held.
            Sound::new("Frozen stab", Fx, MODAL)
                .accent(9.0)
                .strike(0.85, mallet(0.6), 0.7)
                .resonator(0.6, 0.6, 0.5, 0.3)
                .space(0.95)
                .env(1, 0.05, 0.1, 1.0, 1.0)
                .route(Envelope1, Space, 0.45)
                .touch(),
            // The pickup sweeping fast through the modes: a comb filter underwater.
            Sound::new("Underwater", Fx, MODAL)
                .strike(0.0, mallet(0.5), 0.5)
                .blow(0.4, 0.5, 0.4)
                .contour(0.6)
                .resonator(0.4, 0.2, 0.7, 0.3)
                .space(0.7)
                .lfo(1, 7.0, Triangle)
                .route(Lfo1, Position, 0.5),
            // Pan swinging side to side, the pitch bending with it: a doppler fly-by.
            Sound::new("Doppler", Fx, STRING)
                .accent(3.0)
                .strike(0.0, mallet(0.5), 0.5)
                .bow(0.7, 0.5)
                .contour(0.6)
                .resonator(0.3, 0.6, 0.6, 0.3)
                .space(0.5)
                .lfo(2, 0.4, Triangle)
                .route(Lfo2, Pan, 1.0)
                .route(Lfo2, Pitch, 0.04),
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
