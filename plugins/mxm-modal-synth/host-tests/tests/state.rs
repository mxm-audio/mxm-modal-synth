//! The parameters through CLAP state, as a host saves and opens a project: the freeze's two checks
//! (E5, ops' `plans/plan-elements.md` §7). Every parameter round trips, and the state saved at the
//! freeze opens as it was saved, every parameter of `frozen-parameters.txt` at its value.

use mxm_modal_synth_host_tests::{PLUGIN, bundle, frozen_ids, param_id, scratch};
use mxm_player::params::ParamSnapshot;
use mxm_player::session::Session;
use std::path::PathBuf;

/// The state saved at the freeze, as nice-plug writes a CLAP state, and each parameter's value in
/// it as the host read it then: `<id> <value>`, a stepped parameter's value its step, a continuous
/// one's its travel.
const FROZEN_STATE: &str = "frozen-2026-11-05.clapstate";
const FROZEN_VALUES: &str = "frozen-2026-11-05.values";

/// Where the player's `dumpstate` writes and `loadstate` reads, in its session's folder.
const STATE_DUMP: &str = "preset.clapstate";

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn loaded(name: &str) -> Option<Session> {
    let bundle = bundle()?;
    let search = bundle.parent().unwrap().to_path_buf();
    let mut session = Session::new(scratch(name), vec![search]);
    session.load(bundle, PLUGIN);
    // A write before the loaded plugin's first blocks is lost.
    session.advance_blocks(2).unwrap();
    Some(session)
}

/// The plugin's own values. `PlayerState` corrects readback to the player's remembered patch, so it
/// cannot show what a state load restored.
fn values(session: &mut Session) -> Vec<ParamSnapshot> {
    session.app().engine_mut().read_params().params
}

fn find<'a>(values: &'a [ParamSnapshot], id: &str) -> &'a ParamSnapshot {
    values
        .iter()
        .find(|param| param.id == param_id(id))
        .unwrap_or_else(|| panic!("{id} is not among the host's parameters"))
}

fn run(session: &mut Session, command: &str) {
    let reply = session.app().run_cli_command(command);
    assert!(reply.contains("\"ok\""), "{command}: {reply}");
}

/// A value away from `param`'s default, the `i`th parameter's own: a stepped one some steps on,
/// a continuous one somewhere along its travel.
fn moved(param: &ParamSnapshot, i: usize) -> f64 {
    if param.is_stepped {
        let steps = (param.max - param.min) as usize + 1;
        let from = (param.default - param.min) as usize;
        param.min + ((from + 1 + i % (steps - 1)) % steps) as f64
    } else {
        let at = (0.137 + i as f64 * 0.618_034).fract();
        let at = if (at - param.default).abs() < 0.05 {
            (at + 0.5).fract()
        } else {
            at
        };
        param.min + at * (param.max - param.min)
    }
}

/// **Every frozen parameter moved off its default**, one at a time as a host's automation writes
/// them, and its value as the plugin then reports it.
fn move_every_parameter(session: &mut Session) -> Vec<(String, f64)> {
    let before = values(session);
    let ids = frozen_ids();
    assert_eq!(
        before.len(),
        ids.len(),
        "the host sees every frozen parameter and no other"
    );
    for (i, id) in ids.iter().enumerate() {
        let value = moved(find(&before, id), i);
        session.app().set_parameter(param_id(id), value);
        session.advance_blocks(1).unwrap();
    }
    session.advance_blocks(2).unwrap();
    let after = values(session);
    let still: Vec<&String> = ids
        .iter()
        .filter(|id| {
            let param = find(&after, id);
            param.value == param.default
        })
        .collect();
    assert!(still.is_empty(), "these did not move: {still:?}");
    ids.into_iter()
        .map(|id| {
            let value = find(&after, &id).value;
            (id, value)
        })
        .collect()
}

/// **Every parameter round trips through CLAP state**: each moved, the state saved, each sent back
/// to its default, the state opened again; every one is where it was saved.
#[test]
fn every_parameter_round_trips_through_clap_state() {
    let Some(mut session) = loaded("modal-state-round-trip") else {
        return;
    };
    let saved = move_every_parameter(&mut session);
    run(&mut session, "dumpstate");

    for (id, _) in &saved {
        session.app().reset_parameter(param_id(id));
    }
    session.advance_blocks(2).unwrap();
    let reset = values(&mut session);
    for (id, value) in &saved {
        let param = find(&reset, id);
        assert_eq!(param.value, param.default, "{id} went back to its default");
        assert_ne!(param.value, *value);
    }

    run(&mut session, "loadstate");
    let opened = values(&mut session);
    for (id, value) in &saved {
        assert_eq!(find(&opened, id).value, *value, "{id} round trips");
    }
}

/// **The state saved at the freeze opens as it was saved**: every parameter frozen then at its
/// value, through today's plugin. A saved state stores plain values and a list's ids, and the host
/// reads a continuous parameter's travel and a list's index: so this fails when an id, a range, a
/// skew or a list's order moves. Values agree within rounding, as each platform's maths library
/// rounds a skew its own way.
#[test]
fn the_state_saved_at_the_freeze_opens_as_it_was_saved() {
    let Some(mut session) = loaded("modal-frozen-state") else {
        return;
    };
    std::fs::copy(fixture(FROZEN_STATE), session.dir().join(STATE_DUMP)).unwrap();
    run(&mut session, "loadstate");
    let opened = values(&mut session);

    let recorded = std::fs::read_to_string(fixture(FROZEN_VALUES)).unwrap();
    let recorded: Vec<(&str, f64)> = recorded
        .lines()
        .map(|line| {
            let (id, value) = line.split_once(' ').expect("<id> <value>");
            (id, value.parse().expect("a value"))
        })
        .collect();
    let ids: Vec<&str> = recorded.iter().map(|(id, _)| *id).collect();
    assert_eq!(ids, frozen_ids(), "the state holds every frozen parameter");
    for (id, value) in recorded {
        let now = find(&opened, id).value;
        assert!(
            (now - value).abs() <= 1e-6,
            "{id} was saved at {value} and opens at {now}"
        );
    }
}

/// Writes the freeze's state and its values: once, at the freeze. Never again; the files are what
/// [`the_state_saved_at_the_freeze_opens_as_it_was_saved`] holds today's plugin to.
#[test]
#[ignore = "written once, at the freeze (2026-11-05)"]
fn write_the_frozen_state() {
    let (state, values_file) = (fixture(FROZEN_STATE), fixture(FROZEN_VALUES));
    assert!(
        !state.exists() && !values_file.exists(),
        "the freeze's state is written once, never again"
    );
    let mut session = loaded("modal-write-frozen-state").expect("the release bundle");
    let saved = move_every_parameter(&mut session);
    run(&mut session, "dumpstate");
    std::fs::create_dir_all(state.parent().unwrap()).unwrap();
    std::fs::copy(session.dir().join(STATE_DUMP), &state).unwrap();
    let lines: String = saved
        .iter()
        .map(|(id, value)| format!("{id} {value:?}\n"))
        .collect();
    std::fs::write(values_file, lines).unwrap();
}
