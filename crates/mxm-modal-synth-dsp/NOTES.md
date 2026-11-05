# NOTES.md — crates/mxm-modal-synth-dsp

The reasons behind [`AGENTS.md`](AGENTS.md) and what was measured (E3, 2026-11-05).

## The cost (the plan's §5.1)

`examples/cpu.rs`, release build, eight held notes of one model with every exciter playing, an
envelope and an LFO routed, 256-sample renders, on the owner's Windows machine: the heaviest is
**the modal resonator, 12.2 % of a core at 32 kHz** (8.2 % at 44.1 kHz with the rate converter, the
spread between runs larger than the converter's cost); the hidden voice 10.0 %, the strings 8.3 %,
the string 6.0 %. Under the plan's quarter of a core, so eight voices stand.

## One voice is the module's part

The module has one voice and one part; the instrument has eight voices and one part. The part is
written again here (`part.rs`) from `Part::Process` rather than run eight times, because the module
mixes its voice into one soft limit and one reverb, and eight reverbs would be eight rooms. Each
voice is mixed by its own Space, so a voice keeps the width it was played with; the reverb follows
the newest voice's Space, as the module's follows its one patch. A group with no voice adds nothing
and a gain of one is `x × 1`, so one voice sounding is the module's part to the bit, which
`one_voice_is_the_part_exactly` holds for all four models.

**What one soft limit means for chords**: the module's levels are set for one voice at full scale,
so four held notes of a bowed or blown sound sum well into the limit, which then flattens them.
That is the module's sound four times over, and the instrument keeps it; the factory set balances
its sustained sounds by their exciters' levels for it (the plugin's `NOTES.md`).

## The gate a block after the controls

The module reads its CVs a block before they reach the part, and its gate a block after that, so a
strike always lands with the pitch and strength of its own note. A voice here sets its controls in
one block and raises the gate in the next (`the_gate_is_a_block_after_the_controls`), so a stolen
voice's first strike is its new note's.

## The model, latched at the note

On the module the resonator's model is chosen by a hidden gesture and holds for a performance; it
is never a CV. Here it is a parameter latched at each untied note, so a change never swaps a
ringing resonator underneath its own sound, and a slot on Model picks each note's (the factory's
*Model by touch*).

## Idling

A resonator's tail crosses zero, so a block or two of silence comes mid-ring; a voice is idle only
after 128 ms below 10⁻⁶ (−120 dB) on every output the mix uses, longer than half a period of the
lowest note. Its envelopes a slot reads must be at rest too, so a Follow envelope holding a level
keeps it, and one at rest frees it.

## The hidden voice's drone: a recorded deviation

The module's hidden voice puts its oscillators, ungated, on the raw output, which the part mixes in
below Space 0.1; on the module it drones for as long as the voice runs. A released note here ends
once the voice's gated outputs have been silent for the idle time, the drone fading over 10 ms
(`the_hidden_voices_dry_drone_ends_with_its_note`). Without it, any note at low Space on model 3
would never end. While the key is held the drone sounds, as on the module.

## The guard

A string with Damping at its top crossfades to an infinite decay and never ends on its own. A
released note still sounding 30 s after its key went up, its envelopes at rest, is faded over
10 ms and stopped (`a_sound_that_never_ends_is_ended_thirty_seconds_after_its_release`): a guard,
not a design.

## Choices copied with the envelopes and LFOs

From MXM Thanks EG, unchanged (its `NOTES.md` has the reasons): the Response curve
`(e^{6r·x} − 1)/(e^{6r} − 1)`; Rise and Fall linear in seconds; smooth random by a raised cosine;
exponential Glide, in Poly from the last note played; Pan as a balance, unity at the centre; notes
waiting for the next render. The LFO's top rate is a fifth of the block rate, now 400 Hz at the
module's 2 kHz blocks.
