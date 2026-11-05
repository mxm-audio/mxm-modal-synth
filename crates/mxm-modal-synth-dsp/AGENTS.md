# AGENTS.md — crates/mxm-modal-synth-dsp

Parent: [`../../AGENTS.md`](../../AGENTS.md)

# Purpose

The instrument around the ported voice: eight voices, each an Elements voice
(`mxm_modal_voice::Voice`) with its hidden second voice beside it (`OminousVoice`), three envelopes
and three LFOs; the sixteen route slots on the collection's routing (`mxm-modulation`); the part
they share (each voice's mix, one soft limit, one reverb); and the resampler from 32 kHz to the
host's rate. Framework-free, no file IO; nothing allocated after `Instrument::new` and
`set_sample_rate`. The design is ops' `plans/plan-elements.md` §3 and §5. **The envelopes, the
LFOs and the slots are MXM Thanks EG's, copied** (the owner, 2026-11-05: Option C), so each
instrument can change its own; the MATHS behaviour the envelopes take is
`research:modulation/make-noise-maths.md`. The reasons and the measurements are in
[NOTES.md](NOTES.md).

# Ownership

`src/lib.rs` holds **the interface with the plugin** (`Settings`, `Slot`, `Source`, `Target`, the
modes and shapes, `reach`, `Target::offer`, `Instrument`'s methods): a change to it is a change to
`plugins/mxm-modal-synth` in the same commit. Behind it: `voice.rs` (one voice: the patch, the
gate, the model, idling), `voices.rs` (allocation, the held keys, note ids), `part.rs` (the mix,
the soft limit, the reverb, the panic), `envelope.rs`, `lfo.rs`, `routing.rs` (indices, laws,
reaches, the grid), `resampler.rs`, `settings.rs` (the kept settings). Tests: `src/tests.rs`,
`src/conformance.rs`, `tests/realtime.rs`; `examples/cpu.rs` the cost.

# Local Contracts

## The voices

- **Eight in Poly**: a new note takes an idle voice, else the released one sounding longest, else
  the held one sounding longest. **Mono** and **Legato** use voice 0, last-note priority; Mono
  retriggers on every new key; Legato glides to a key played over a held one without a new gate;
  a released key returns to the one held before it, without one. A change of mode releases every
  voice. Glide (time to within 1 %, exponential): in Poly from the last note played, in Mono from
  where the voice is, in Legato on a tie only.
- **Note events wait for the next `render`** (a fixed queue of 256), which takes in its settings
  first: an event acts at the next engine block either way, and so under the settings that block
  plays. An event naming a note id acts on the voice playing it; one without, or on a voice
  without one, on its key. A non-finite velocity is 1; a non-finite tuning is dropped; a new note
  clears the tuning.
- **A voice is the module with the case around it.** Per engine block of 16 samples at 32 kHz:
  the routing is evaluated (below) and each control, with its slots, constrained as the module's
  `cv_scaler.cc` does (0.9995; Strike timbre 0.995; Space 2; a pot without a CV, Contour and the
  three levels, 65 535/65 536); Flow, Mallet, Geometry, Position and Space go through its one-poles
  (0.05, 0.05, 0.05, 0.01, 0.01 a block). Strength is 0.5 (the accent's 0 dB) plus its slots,
  kept to 0…1. The note is `key + Coarse + Fine + bend × range + tuning + Pitch`, to a frequency as
  `Part::Process` does. The patch's fields no knob sets are `Part::Init`'s (plan D10); no seed.
- **The gate reaches the voice a block after its controls**, as the module reads its CVs a block
  before its gate, so a strike lands on its own pitch and strength; a retrigger drops it for one
  block. **The model is latched at each untied note** (0 modal, 1 string, 2 strings, 3 the hidden
  voice); a slot on Model picks each note's. The voice's inputs (strike and blow in) are silent.
- **An idle voice is not rendered** and adds exact zero. Idle when its key is up and its gate has
  fallen, the envelopes a slot reads at rest, and every output the mix uses below 10⁻⁶ for 256
  blocks (128 ms, longer than half a period of the lowest note). **The hidden voice's dry drone**
  (its oscillators, heard raw at Space under 0.1, ungated on the module) fades over 10 ms once its
  gated outputs have been silent that long: a recorded deviation, NOTES.md. **A released note
  always ends**: one still sounding 30 s after its key went up (a string with Damping at its top
  never decays) fades over 10 ms and stops. The next note starts an idle voice as `init` leaves
  it; `reset` clears every voice and the part.
- Velocity (`v − 1`, how far below the hardest note) and Random (`2u − 1`) are drawn per untied
  note; Key is the glided note on a
  60-semitone unit, middle C 0; Wheel and Pressure 0 to 1, Bend the lever −1 to 1
  (`mxm_modulation::standard`). **Contour** is the exciter's envelope (the hidden voice's, on
  model 3), published after the render, so every reader reads it a block late.

## The part

`Part::Process` for many voices: each voice's three outputs mixed to stereo by its own Space as
`Part` mixes its one, through its gains (Amplitude `×0…×2`, Pan a balance with the centre unity in
both channels, ramped across the block); **the resonator voices summed into one soft limit, the
hidden voices added after it** (the module leaves its hidden voice unlimited); **one reverb**,
following the newest voice's Space; `SoftConvert` to the module's frames, read as `frame / 32 768`
times Volume, the module's right channel (main) on the right. A group with no voice in it adds
nothing, so one voice of either kind is `Part` to the bit (`one_voice_is_the_part_exactly`).
`Part`'s metering and panic stand: a mix past 200 resets every resonator the next block. The
reverb's tail sounds until 1 000 blocks of silent frames (0.5 s, past its longest delay).

## The envelopes, MATHS' functions in an ADSR

As the plan's §5.4 and MXM Thanks EG's: Attack, Decay, Sustain, Release, one Response curve (−1 log
to +1 exp, MATHS' Vari-Response), Cycle, and Mode (Note, Gate, Follow). Once per block (2 kHz).

- **One slope core**: a ramp 0…1 at the segment's rate, output `curve(ramp)`; the curve is
  `(e^{6r·x} − 1)/(e^{6r} − 1)` (its mirror for log), defined against the level as the manual
  defines it, so a retrigger continues from the level it is at. A time is the whole 0-to-1 travel;
  the curve does not change it.
- **Times**: each from 0.5 ms to 30 s on its parameter; the Time target reaches ×/÷ 16 (4 octaves,
  exponential, positive shortens, as MATHS' Both); Rise and Fall reach their whole range, linear
  in seconds (an amount of 1 adds 30 s), positive slows, as MATHS' Rise and Fall CV; then clamped
  to the range, and Time after.
- **Cycling**, attack and decay repeat and Sustain is unused. In Note mode it starts at note on and
  releases at note off from where it is; in Gate mode it cycles while Cycle is on or the Cycle
  target is high, with or without a trigger; in Follow mode Cycle does nothing.
- **Retrigger, as MATHS**: a rising Trigger during the decay, sustain or release restarts the
  attack from where it is; one during the attack is ignored. In Note mode an attack the Trigger
  started completes whatever the note does. A new note on a sounding voice restarts the attack
  from where it is (Legato's tied notes don't). In Gate mode the Trigger is the gate: falling,
  even in the attack, releases.
- **Follow**: the output moves toward its Input (clamped 0…1) at the attack rate rising and the
  release rate falling (lag, portamento, attack–sustain–release).
- **The gates**: End of rise high from the end of the attack until rest or the next attack (a
  followed level reached counts as an end of rise); End of cycle its complement. A gate target is
  high while the slots aimed at it sum above one half.
- Output 0 to 1 (MATHS' 10 V); cycling, it turns at 0.8 (the manual's 8 V).

## The LFOs

Rate from 0.01 Hz to 50 Hz, or resolved from the tempo by the plugin (kept to a fifth of the block
rate, 400 Hz); the Rate target reaches ±4 octaves (the standard's). Shapes: sine, triangle and
square from phase 0 as the sine, saw up from −1, saw down from +1, sample-and-hold, and smooth
random (a raised-cosine glide to a new random point each cycle). **Retrigger on**: each voice's own
LFO restarts at phase 0 with each untied note; **off**: one LFO for all voices, free-running, whose
Rate target reads only the shared sources (Wheel, Pressure, Bend, the shared LFOs). Fade: the
depth rises linearly from 0 over the fade time after each note. Output −1 to 1.

## The routing

- **The slots fill `mxm-modulation`'s grid** (`SourceFrame`, `Compacted`), rebuilt when a slot
  changes, never per block. A slot is in use when its source and target are both set; two on one
  pair add; an amount is kept to ±1. **A pair the standard refuses is not filled**
  (`Target::offer`: a gesture into a Cycle or Trigger gate).
- **Each block, in order**: the performance sources, the LFOs, envelopes 1, 2, 3, then the voice's
  targets, the render, and the Contour source. A source read before it is computed this block reads
  last block's value, so any loop (an envelope triggering itself, a ring) is deterministic and one
  block, 0.5 ms, late. Every source is published every block, so nothing owes a `clear`.
- **Reaches** (amount ±1, [`reach`]): Pitch ±12 st; Model ±3 models (one slot reaches every one
  from any); Space ±2 (the module's CV range, past the freeze); Amplitude `×0…×2` (the standard's
  law); a Rate ±4 octaves; envelope times as above; every other target its whole range (a control,
  Strength, Pan, a gate, an Input). Key routes take the standard's per-octave reach.

## The resampler

MXM Thanks EG's, its source the voices' 32 kHz: a fixed polyphase Kaiser-windowed sinc to the host's
rate, passband to 14.4 kHz (0.45 of 32 kHz; the module stops at 16), stopband from the lower Nyquist
at least 100 dB down, history cleared on `reset`; host rates 44.1 to 192 kHz (any rate works).
**Its latency is fixed**: 100 host samples at 44.1 kHz, 108 at 48, 216 at 96, 432 at 192. **At
32 kHz bypassed, with no latency.** It is this crate's copy: **moving it into mxm-kit, as the plan
asks, needs the owner's word to push the kit's public `main`**, and waits for it.

## Realtime

No allocation, lock or panic in `render`, the note calls or `reset`, on any input; a NaN in a
setting is ignored (the last good value stays), never propagated. `#![forbid(unsafe_code)]` in the
library; `tests/realtime.rs` counts allocations with its own allocator.

# Work Guidance

- The voice is held bit for bit (mxm-modal-voice); this crate is not, and is tested as the
  collection's instruments are. Its one link to the voice's reference is
  `one_voice_is_the_part_exactly`, for all four models.
- The voice's additions for a host (`envelope`, `random_mut`, `clear`, on the voice and the hidden
  voice) are listed in mxm-modal-voice's `AGENTS.md`; `init`, `set_resonator_model` and `panic` are
  the C++'s. Add one there only when nothing here can know it.

# Verification

```bash
cargo test -p mxm-modal-synth-dsp
cargo clippy -p mxm-modal-synth-dsp --all-targets -- -D warnings
cargo run --release -p mxm-modal-synth-dsp --example cpu    # eight voices of each model
```

The tests owed, all there: one voice equals `Part`, all four models; the gate a block after the
controls; silence is exact zero once everything has ended, at 32 and 44.1 kHz; a sound that never
ends is ended 30 s after its release; the hidden voice's drone ends with its note; no NaN or
infinity across every model, extreme settings and every slot at ±1; each envelope mode, the
retrigger rule, Cycle and the gates; the LFO shapes, Retrigger and Fade; voice allocation and
stealing, Mono and Legato, note ids; each target's reach; Contour is the exciter's envelope; the
resampler's passband, stopband and latency; the standard's three conformance checks, each falsified
once; no allocation; the CPU measure (the plan's §5.1: under a quarter of a core; the figure is in
NOTES.md).

# Child DOX Index

None.
