//! Hostile activations and callback shapes through the direct CLAP host. Run against a debug
//! bundle as well as release: the workspace enables nice-plug's debug allocation assertion, so an
//! audio-thread allocation aborts the callback instead of hiding.

use mxm_modal_synth_host_tests::{PLUGIN, bundle};
use mxm_player::events::input::Payload;
use mxm_player_harness::harness::Harness;

fn note_on(key: u8) -> Payload {
    Payload::NoteOn {
        channel: 0,
        key,
        velocity: 0.8,
    }
}

fn note_off(key: u8) -> Payload {
    Payload::NoteOff {
        channel: 0,
        key,
        velocity: 0.0,
    }
}

/// Eight voices at full level, before Volume: the bound a sample stays within.
const BOUND: f32 = 8.0;

#[test]
fn hostile_sample_rates_and_every_block_size_stay_finite_and_bounded() {
    let Some(bundle) = bundle() else {
        return;
    };
    // A rate the resampler does not serve is refused, never played at the wrong speed.
    for rate in [1_000.0, 7_999.0, 2_000_000.0] {
        assert!(
            Harness::with_configuration(&bundle, PLUGIN, 1, rate, 8192).is_err(),
            "{rate} Hz is refused"
        );
    }
    for rate in [
        8_000.0, 11_025.0, 44_100.0, 48_000.0, 96_000.0, 192_000.0, 768_000.0,
    ] {
        let mut host = Harness::with_configuration(&bundle, PLUGIN, 1, rate, 8192).expect("hosts");
        for key in [0, 60, 127] {
            assert!(host.push(0, note_on(key)));
        }
        for frames in [1usize, 2, 11, 12, 13, 63, 64, 65, 255, 1024, 4097, 8192] {
            let audio = host.render(frames);
            assert!(
                audio.iter().all(|s| s.is_finite() && s.abs() <= BOUND),
                "invalid output at {rate} Hz / {frames} frames"
            );
        }
        for key in [0, 60, 127] {
            assert!(host.push(0, note_off(key)));
        }
        for _ in 0..16 {
            host.render(8192);
        }
        assert!(host.render(64).iter().all(|s| s.is_finite()));
        host.shutdown();
    }
}

#[test]
fn more_notes_than_voices_steal_and_stay_finite() {
    let Some(bundle) = bundle() else {
        return;
    };
    let mut host = Harness::new(&bundle, PLUGIN, 1).expect("hosts");
    for key in 36..60 {
        assert!(host.push(0, note_on(key)));
        let audio = host.render(256);
        assert!(audio.iter().all(|s| s.is_finite() && s.abs() <= BOUND));
    }
    for key in 36..60 {
        assert!(host.push(0, note_off(key)));
    }
    assert!(host.render(4096).iter().all(|s| s.is_finite()));
    host.shutdown();
}
