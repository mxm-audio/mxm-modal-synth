//! What a player hears, through the real host: every model sounds, and a released voice ends in
//! exact silence.

use mxm_modal_synth_host_tests::{PLUGIN, bundle, param_id};
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

/// The loudest sample of `seconds` of audio at 48 kHz.
fn peak_over(host: &mut Harness, seconds: f32) -> f32 {
    let mut peak = 0.0f32;
    for _ in 0..(seconds * 48_000.0 / 512.0) as usize {
        let audio = host.render(512);
        peak = audio.iter().fold(peak, |p, s| p.max(s.abs()));
    }
    peak
}

#[test]
fn every_model_sounds() {
    let Some(bundle) = bundle() else {
        return;
    };
    // The modal resonator, the string, the strings and the dark FM voice.
    for model in 0..4 {
        let mut host = Harness::new(&bundle, PLUGIN, 1).expect("hosts");
        // Model is a stepped parameter, so CLAP carries its plain value.
        assert!(host.push(
            0,
            Payload::ParamValue {
                param_id: param_id("model"),
                value: f64::from(model),
            }
        ));
        host.render(64);
        assert!(host.push(0, note_on(48)));
        let peak = peak_over(&mut host, 0.5);
        assert!(
            peak > 1e-3,
            "model {model} is silent through the player (peak {peak})"
        );
        assert!(host.push(0, note_off(48)));
        host.shutdown();
    }
}

#[test]
fn a_released_voice_ends_in_exact_silence() {
    let Some(bundle) = bundle() else {
        return;
    };
    let mut host = Harness::new(&bundle, PLUGIN, 1).expect("hosts");
    assert!(host.push(0, note_on(60)));
    assert!(peak_over(&mut host, 0.3) > 1e-3);
    assert!(host.push(0, note_off(60)));
    peak_over(&mut host, 10.0);
    let tail = host.render(4096);
    assert!(
        tail.iter().all(|s| *s == 0.0),
        "an idle voice and the room's tail must end in exact zero"
    );
    host.shutdown();
}
