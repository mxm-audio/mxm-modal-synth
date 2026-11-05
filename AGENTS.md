# AGENTS.md — mxm-modal-synth

DOX rail for this repository: what it is, the rules every part keeps, and the Child DOX Index.
The reasoning and the owner's rulings are in [NOTES.md](NOTES.md). The plan it is built from is
ops' `plans/plan-elements.md`, with its stages (E0 to E5) and their checks. The rules every MXM
plugin keeps are mxm-kit's `docs/plugin-conventions.md`; nothing here weakens them.

# Purpose

**MXM Modal Synth**: Elements' modal synthesis voice as a polyphonic MXM instrument, CLAP and VST3,
ported to Rust from Emilie Gillet's MIT code, "the exact same way that we did plaits" (the owner,
2026-11-05): MXM Thanks EG's envelopes, LFOs, route slots, shell and editor around it. Eight
voices, each a whole Elements voice (bow, blow and strike into a modal resonator, a string or
strings, and the hidden dark FM voice); one part, one soft limit and one reverb for all; three
envelopes that are function generators after Make Noise's MATHS too; three LFOs; sixteen route
slots; 105 parameters.

Pre-alpha (2026-11-05): the research and the harness (E1), the port with its 106 cases bit for
bit (E2), the instrument (E3) and the plugin with its editor, factory sounds and control map (E4)
are built, and pass their checks, the host tests included, and `clap-validator` but for its
three known Windows preset-path failures (below). The owner approved the sounds (E5,
2026-11-05). **Owed**: a run in Bitwig (E4), and E5's freeze, with the open points in `NOTES.md`. Private until it is ready; after version 1.0 of the
MXM collection.

# Ownership

Root owns `AGENTS.md`, `NOTES.md`, `README.md`, `NOTICE.md`, `LICENSE`, `TRADEMARKS.md`,
`CONTRIBUTING.md`, `Cargo.toml`, `Cargo.lock`, `bundler.toml`, `.cargo/config.toml` (the `xtask`
alias), `.gitignore` and `.gitattributes`, and `xtask/` (the collection's shared `mxm-xtask`:
`bundle`), and `docs/briefs/mxm-modal-synth.md` (the design system's §14 brief).
The port is mxm-modal-voice, its own repository (below). `crates/mxm-modal-synth-dsp` (the
instrument: voices, the part, envelopes, LFOs, the slots' routing, the rate converter) and
`plugins/mxm-modal-synth` (the shell and editor, with `host-tests/`, the slow tier) have their own
`AGENTS.md`.

The reference harness, the C++ it builds and the research page are in the private research
repository (`research:sources/mutable-instruments-elements/`, its harness in `derived/harness/`;
`research:physical-modelling/mutable-instruments-elements.md`), never here nor in
mxm-modal-voice, which commits only the harness's cases and its outputs' SHA-256s.

# Local Contracts

## The voice: mxm-modal-voice

- **The port is its own repository**, `mxm-audio/mxm-modal-voice` (`libs/mxm-modal-voice` in the
  workspace), MIT, private until the owner says otherwise, taken by its `main`, locked. It holds
  Elements' part bit for bit to the C++ at 32 kHz, in blocks of 16, and its `AGENTS.md` has the
  porting rules. A change to the voice is made there, keeping its 106 cases bit for bit, then
  relocked here.
- **Rust only**: no C++ in the plugin and no FFI.
- **Flush-to-zero is on**: the plugin takes nice-plug's `unsafe_flush_denormals`, as the reference
  was rendered, so what ships is the reference's bits.
- **Elements' data ships as Elements has it**: the nine strike recordings, the noise and the tables,
  MIT as its code (the owner, 2026-11-05).
- **The voice reads no files**, and its audio inputs stay silent (no input now, the owner's ruling).

## The names

"Elements", its hidden voice's name, "Mutable Instruments", "MATHS" and "Make Noise" appear in
`NOTICE.md`, the README, this file, `NOTES.md` and the research, never in the product: not in a
parameter, a model's or a control's name, a preset or the editor (Emilie Gillet's *Guidelines for
derivative works*; ops' `notes/synths.md`: maker and model names are research inputs, never
product names).

## The instrument

- **The host holds what the panel can show at once**: the voice's 21 parameters, the envelopes' 21,
  the LFOs' 15 and sixteen route slots `route<r>_{source,target,amount}`, 48; **105**. A source or
  target is a value of a slot, never a parameter of its own. The plan's §5 has the lists.
- **Every voice runs at 32 kHz**, as the module, and one rate converter (mxm-kit's
  `mxm_dsp::rate_converter`) takes the sum to the host's rate with a fixed, reported latency; at
  32 kHz nothing is converted.
- **One voice is the module's part to the bit** (`one_voice_is_the_part_exactly`, all four models):
  the instrument's part is `Part::Process` for many voices, the resonators into one soft limit, the
  hidden voices after it, one reverb.
- **Thanks EG's modules are copies** (the owner, 2026-11-05): the envelopes, the LFOs, the route
  slots, the shell and the editor's shape; each instrument changes its own.
- The collection's contract, as every MXM instrument (the plan's §5.7).

# Work Guidance

- Build and test on Windows only, as far as `python ws.py reach` says a change goes; the other
  platforms in the owner's batch (the workspace's `AGENTS.md`).
- Voice work in mxm-modal-voice, built here first through `python ws.py link mxm-modal-voice`; kit
  work (the rate converter's) through `python ws.py link kit`, unlinked before any push; pushing
  mxm-kit's public `main` needs the owner's word.

# Verification

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test                 # the fast tier: the instrument and the plugin
cargo xtask bundle mxm-modal-synth --release
cargo test -p mxm-modal-synth-host-tests      # the slow tier: the release bundle in MXM Player
clap-validator validate target/bundled/mxm-modal-synth.clap
```

`clap-validator` 0.4.1 on Windows fails `preset-discovery-crawl`, `-descriptor-consistency` and
`-load` on every MXM plugin with installed presets: the preset provider gives a `C:\…` path and
the validator takes only `/…` (its `src/plugin/preset_discovery/indexer.rs:178`, on `master` too),
a Unix-only check no absolute Windows path can pass (2026-11-04, the board's message 436). Every
other test passes.

The reference's 106 cases and their digests are mxm-modal-voice's, in its `tests/`, and its
`cargo test` checks them; here the instrument's tests check its link to the voice.

# Child DOX Index

| Doc | Scope |
|---|---|
| [`crates/mxm-modal-synth-dsp/AGENTS.md`](crates/mxm-modal-synth-dsp/AGENTS.md) | The instrument: the interface with the plugin, the voices, the part, the envelopes with MATHS' functions, the LFOs, the routing, the rate converter |
| [`plugins/mxm-modal-synth/AGENTS.md`](plugins/mxm-modal-synth/AGENTS.md) | The shell: the 105 parameters and the slots' lists, events and the callback, Init and the presets, the control map, and the editor: its twelve cards, the slots as rows on their targets' cards |
