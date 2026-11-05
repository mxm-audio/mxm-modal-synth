# AGENTS.md — plugins/mxm-modal-synth

Parent: [`../../AGENTS.md`](../../AGENTS.md)

# Purpose

The nice-plug shell, CLAP and VST3, over the instrument in `crates/mxm-modal-synth-dsp` (its
`src/lib.rs` is the interface): the 105 parameters, events, the realtime callback, state, the
preset system's wiring, the control map and the editor. Ops' `plans/plan-elements.md` §5 is the
design; the shell, the slots and the editor's shape are MXM Thanks EG's, copied (the owner,
2026-11-05).

# Ownership

`Cargo.toml`, `control-map.json`, `frozen-parameters.txt` (the freeze), `presets/`, `tests/` (the
factory set's levels), `host-tests/` (the slow tier) and `src/`: `lib.rs` (identity, the `Plugin` impl, events, `process`), `params.rs`
(the voice's 21, the envelopes' and LFOs' nested parameters, the `Settings` fill), `routes.rs`
(the sixteen slots and their lists), `preset.rs` (`mxm_preset::Instrument` and the factory design),
`telemetry.rs` (the atomics the editor reads), and the editor: `editor.rs` (the window, the app
bar, paging, the developer Parameters list), `editor/cards.rs` (the cards as `mxm_ui::tree`s and
every parameter's binding), `editor/route_rows.rs` (the slots as rows), `editor/binding.rs`
(`mxm_params::binding`, re-exported) and `editor/proof.rs` (its tests). The DSP is the crate's.

# Local Contracts

## Identity and layouts

- `CLAP_ID` `dk.mxm.mxm-modal-synth` from `plugin_name!`; `VST3_CLASS_ID` its hash, written out.
- Stereo, then mono (the two channels' average); no input of any kind (the owner, 2026-11-05).

## The 105 parameters

- **The ids** (`params::VOICE_IDS`, `ENVELOPE_IDS`, `LFO_IDS`, `routes::ROUTE_IDS`): the voice's
  `model coarse fine contour bow bow_timbre blow flow blow_timbre strike mallet strike_timbre
  geometry brightness damping position space mode glide bend_range volume`;
  `env<n>_{attack,decay,sustain,release,response,cycle,mode}` and
  `lfo<n>_{rate,shape,sync,retrigger,fade}`, `n` 1–3, nested by `id_prefix`;
  `route<r>_{source,target,amount}`, `r` 1–16. **Frozen since E5** (2026-11-05): every id, kind,
  range and default, and every list's values in order (Model's models, `params::MODEL_NAMES`; the
  voice and envelope modes, the LFO shapes, the slots' sources and targets), as
  `frozen-parameters.txt` holds them (`the_parameters_are_the_frozen_tables`). A line never
  changes, no id is removed or reused, and **no list grows**: a preset stores a value as its index
  over the list's length. A new parameter is appended. The host tests' `state.rs` takes every
  parameter through CLAP state and opens the state saved at the freeze (`tests/fixtures/`, written
  once). [NOTES.md § The freeze](../../NOTES.md#the-freeze-e5-2026-11-05)
- **The slots' lists are the DSP's**: `Off`, then `Source::ALL` (19) or `Target::ALL` (40) in
  order. `routes::SourceChoice` and `TargetChoice` carry the DSP's value and implement nice-plug's
  `Enum` by hand, their names from an exhaustive match, so a DSP addition fails to compile until
  named.
- **Names are the instrument's own**, sentence case: never "Elements", the hidden voice's name,
  "Mutable Instruments", "MATHS", "Make Noise" or "Plaits" (`no_name_is_the_originals`). The
  models are *Modal*, *String*, *Strings* and *Dark FM*. The Follow mode's target reads *Env n
  follow* (the owner, 2026-11-05), its id `env<n>_input` as frozen.
- **Readings**: `mxm_params::text` where it has one; Model reads its name, Coarse and Fine signed
  semitones (Coarse ±36, Fine ±2, the module's), Volume (stored linear) decibels, a Rate hertz (a
  typed division picks its position), times by the kit's
  `text::tenths_of_milliseconds_then_seconds`.

## Init, smoothing, sync

- **Init**: every amount at zero (`params::AMOUNT_IDS`: Glide, each Fade, every route amount),
  every slot `Off`, no sync; the voice where the module powers on (`Part::Init`): the modal
  resonator struck at 0.8, Contour 1, Geometry 0.2, Brightness 0.5, Damping 0.25, Position 0.3,
  Space 0.5, the timbres at 0.5; Volume −3 dB. Init is the CLAP defaults, generated, with no file.
- **Smoothed**: what adds to or multiplies the sound (Fine, the fourteen sound controls, Volume,
  each Sustain, every route amount). Not: Model, Coarse, every time, rate, mode, shape and switch.
- **The patch, once per process call** (`ModalSynthParams::settings`), each LFO's Rate resolved
  there through `params::LFO_SYNC` at the call's tempo, free without one; **the smoothed signals
  step once per internal block** (`ModalSynthParams::advance`). The tempo in force is published in
  `telemetry.tempo` and forgotten at `activate`.

## Route slots

- **mxm-drum-machine's scheme**: a slot is in use when neither end is `Off`; switching its source off
  keeps its target and amount; two on one pair add (the DSP's). The amount is −1…+1 of its
  target's reach, read as a percentage: the drums' **recorded deviation** from `routing_checks`'
  pair-by-pair readings, which need a parameter per pair.
- **The slots stay here**, a copy of Thanks EG's, and out of the kit: each instrument may change
  its own (the owner, 2026-11-05: *"This synthe and thanks Eg might change later, so the shared
  slots are mainly by accident"*).

## Events and the callback

- NoteOn with velocity and note id (velocity 0 is a NoteOff); NoteOff and Choke by key, or, key-less,
  by note id, a wildcard being every note; `MidiPitchBend` (−1…1); CC 1 the wheel; channel
  pressure; `PolyTuning` by key or, key-less, by note id (non-finite, or a wildcard, dropped);
  CC 120 chokes every note, CC 123 lets every held key go. Per-note pressure and the other
  expressions fall through. The instrument's side is `note_ids_pick_their_voice`.
- **Split at every event and capped at `MAX_BLOCK`**; events at the end, and in an empty buffer,
  apply before the next sample. Nothing allocates in `process` or `reset`.
- **The latency is `Instrument::latency_samples()`, reported in `activate`**, after
  `set_sample_rate`; fixed for the activation (none at 32 kHz). **`activate` refuses a rate outside
  `HOST_RATES`** (8 kHz to 1.536 MHz), which the rate converter would play at the wrong speed. A
  served rate is taken to the whole hertz: a fractional one plays at most 0.5 Hz off, 62 ppm at
  8 kHz and 11 at 44.1 (0.1 cent at most), inside an interface's own clock tolerance; refusing it
  would leave the plugin unloaded in such a host.
- **`KeepAlive` while `Instrument::is_sounding()`, else `Normal`.** No `Tail(n)`: a resonator, a
  cycling envelope on an exciter or the reverb's tail sounds with no key held, so no tail length is
  honest.
- **Flush-to-zero** through nice-plug's `unsafe_flush_denormals`, as the voice's reference was
  rendered: a recorded deviation from the kit's *flush denormals in the DSP itself*, since a flush
  in the ported code would change its bits.

## Presets, map and telemetry

- `preset.rs` implements `mxm_preset::Instrument`; `CLAP_PRESETS` and `preset_loader` let a host
  list and load the files. The design is `preset::tests::design`, overrides on Init in each
  parameter's own units; the ignored `write_the_factory_presets` writes the files and `order.txt`,
  and `the_shipped_files_are_the_design` fails when they differ. At least fifty (60), each with its
  category, every model with one (`every_model_has_a_sound`), names short, sentence case and the
  set's own; the order a first impression, then `Category::ALL`'s (sorted in `design`). **The owner
  approved them (2026-11-05)**: a change to a sound is the owner's to hear.
- **The set's levels are `tests/factory_levels.rs`**: each file loaded as a host loads it, played
  as its category is (at 120 bpm), its BS.1770 momentary loudness at its loudest within `WINDOW_LU`
  of the set's median, no sample past full scale, and silent within `ENDS_WITHIN` of its key going
  up. **Balanced in the sound, never by Volume** (`factory_content_does_not_move_volume`): a struck
  sound by its accent (`Sound::accent`, Envelope 3 into Strength); a bowed or blown one by its
  levels and timbres, never cut by the accent, which would swell on release (`designed` holds it);
  the dark FM voice by its oscillators' levels and its filter's mode (Geometry).
- `control-map.json` claims only matching roles (its `_comment` says why each; held by
  `control_map::the_map_holds_to_the_standard_and_names_real_parameters`). No route amount is
  claimed: a slot moves with its routing.
- `telemetry::Telemetry` (shared through `MxmModalSynth::telemetry`): the peak meter, voices
  sounding, the tempo, and the developer channel's requests (CC 116–119 with `MXM_DEV_CC` set).

## Editor ([NOTES.md § The editor](../../NOTES.md#the-editor))

- **Twelve cards in the conventions' category order** (`cards::CARDS`): Voice, with the routes of
  Amplitude and Pan (Performance); Envelope 1–3 and LFO 1–3 (Modulators); Model (the four models,
  Coarse, Fine), Exciter (Contour and the three levels, with Strength's routes), Bow and blow (their
  timbres and Flow), Strike (Mallet, Timbre) (Generators); Resonator, with Space (Tone): the
  module's panel, a row of levels over each exciter's timbres. **Volume is the app bar's**. Every
  control is the collection's, through `mxm_params::binding`; a painted name drops what its card
  says, the parameter's name stays the host's.
- **A route slot is a row on the card of its target** (`Card::targets`: each of the 40 on one card,
  the card of what it moves), the drum machine's shape: the target's line offers `‹ modulate ›`; a
  choice takes a free slot in one bracketed gesture, one this target had first, else
  `Routes::first_free`. **A slot not in use is on no card.** With all sixteen in use every line
  reads `route_rows::FULL`, disabled, and offers nothing. **`‹ modulate ›` is a cursor stop** under
  a key of its own (`route_rows::modulate_key`, `mxm_ui::navigation::at`): MENU opens its list, ↑ ↓
  and Enter pick; the cursor's coverage lists it while it offers a source.
- **It opens on one page with every card**: `REFERENCE` (1821 × 1080) is the budget hugged
  (`the_opening_size_is_the_budget_hugged`), ten points wider than that check's figure
  (`the_window_opens_on_one_page_with_every_card`). A card after the LFOs holds at most five
  target lines, so the second row stays under the first, the envelopes' six. `MINIMUM` is the app bar's last compact step.
- Step laws: Coarse and Bend range `Semitones`, the rest their own. Every knob is Standard.
- CC 118 asks for an expander this editor has not got: taken, and nothing changes. CC 119's 127 is
  the developer Parameters list, every parameter by its full name.

# Work Guidance

- Change `params.rs`, `routes.rs` or the design, then regenerate:
  `cargo test -p mxm-modal-synth --lib write_the_factory_presets -- --ignored`.
- The level test is the one here that depends on sound; `-- --nocapture` prints each sound's
  loudness, peak and ring.
- **Designing a sound, the gotchas**: chords of bowed or blown sounds sum into the one soft limit,
  where a cut barely moves the loudness until it is well under it; the bow on the modal resonator
  has a threshold (silent at 0.3, full near 0.5); Blow above two thirds adds the tube with the
  noise held at its top, so a tube sound's level is its timbres', Brightness and Damping; the
  measure reads a sound at its loudest 400 ms, the envelope's peak, so Contour's sustain hardly
  moves it. The dark FM voice ignores Strength once its envelope sustains, and its Geometry is its
  filter's mode, low-pass below a half.
- `cargo xtask bundle mxm-modal-synth --release` makes the `.clap` and `.vst3` and installs them;
  `MXM_CLAP_DIR` and `MXM_VST3_DIR` send a debug bundle elsewhere.

# Verification

```bash
cargo test -p mxm-modal-synth     # with tests/factory_levels.rs, the set's loudness
cargo clippy -p mxm-modal-synth --all-targets -- -D warnings
cargo xtask bundle mxm-modal-synth --release
clap-validator validate target/bundled/mxm-modal-synth.clap
# Every page, light and dark, at Init and with slots in use, for review:
# target/layout-tree/mxm-modal-synth/<MXM_PICTURES tag>/{init,routed}/
MXM_PICTURES=after cargo test -p mxm-modal-synth --lib tree_pictures -- --ignored
```

`clap-validator` 0.4.1 fails `preset-discovery-crawl`, `-descriptor-consistency` and `-load` once the
factory folder is installed: the nice-plug fork's provider declares the folder as a Windows path
(`C:\…`), and the validator takes only one starting with `/` (every MXM plugin with installed
presets, 2026-11-04). The other tests pass.

# Child DOX Index

None.
