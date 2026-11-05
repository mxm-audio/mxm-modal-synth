//! The cost of eight voices of each model, ops' `plans/plan-elements.md` §5.1 measure: eight held
//! notes, every exciter playing, at 32 kHz and resampled to 44.1, a 256-sample render at a time,
//! as a share of one core (a second of audio's render time over a second). The bar is a quarter of
//! a core for the heaviest.
//!
//! ```text
//! cargo run --release -p mxm-modal-synth-dsp --example cpu
//! ```

use mxm_modal_synth_dsp::{Instrument, Settings, Slot, Source, Target};
use std::time::Instant;

fn main() {
    let seconds = 4;
    let mut rows = Vec::new();
    for (label, rate) in [("32 kHz", 32_000.0f32), ("44.1 kHz", 44_100.0)] {
        for model in 0..4 {
            let mut settings = Settings {
                model,
                bow: 0.6,
                blow: 0.9,
                strike: 0.8,
                space: 1.0,
                ..Settings::default()
            };
            // A busy patch: an envelope and an LFO on the voice's controls.
            settings.slots[0] = Slot {
                source: Some(Source::Envelope1),
                target: Some(Target::Brightness),
                amount: 0.3,
            };
            settings.slots[1] = Slot {
                source: Some(Source::Lfo1),
                target: Some(Target::Geometry),
                amount: 0.2,
            };
            let mut instrument = Instrument::new();
            instrument.set_sample_rate(rate);
            for k in 0..8u8 {
                instrument.note_on(48 + 3 * k, 0.8, None);
            }
            let (mut left, mut right) = ([0.0f32; 256], [0.0f32; 256]);
            let blocks = (rate as usize * seconds).div_ceil(256);
            // Warm up, then measure.
            for _ in 0..blocks / 8 {
                instrument.render(&settings, &mut left, &mut right);
            }
            let start = Instant::now();
            for _ in 0..blocks {
                instrument.render(&settings, &mut left, &mut right);
            }
            let share = start.elapsed().as_secs_f64() / seconds as f64;
            assert_eq!(instrument.telemetry().voices_sounding, 8);
            rows.push((label, model, share));
        }
    }
    for (label, model, share) in &rows {
        println!(
            "{label:>8}  model {model}  {:>5.1} % of a core",
            share * 100.0
        );
    }
    let (label, model, share) = rows
        .iter()
        .max_by(|a, b| a.2.total_cmp(&b.2))
        .copied()
        .unwrap();
    println!(
        "heaviest: model {model} at {label}, {:.1} % of a core (bar: 25 %)",
        share * 100.0
    );
}
