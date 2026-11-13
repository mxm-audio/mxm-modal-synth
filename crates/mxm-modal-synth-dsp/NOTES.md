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
its sustained sounds by their exciters' levels for it (the plugin's `NOTES.md`). Kept by the owner
(2026-11-05).

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
would never end. While the key is held the drone sounds, as on the module. Kept by the owner
(2026-11-05).

## The bowed loops' limit: a recorded deviation

Elements' modal resonator runs its eight bowed modes whether or not the bow plays, and above 7.37 kHz
(0.2304 of its 32 kHz) the bow table's feedback lifts a bowed mode's loop past a gain of one: struck,
the mode grows instead of decaying, as it does in the C++ (mxm-modal-voice's `NOTES.md` has the
loop). The owner's song found it (2026-11-13: a toy piano on *Tin plate keys* whistling on at
10.4 kHz from D7). Every voice turns on the port's limit, which holds each bowed loop to the C++'s
own gain of 0.99 (the owner: *"Yes do it that way"*).

Measured on the factory sounds (single notes, C0 to C9 in quarter tones, against the C++): without
it, 27 of the 30 on the modal resonator run away at some pitch, from E3 up in most, from F♯2 in
Tin hat; with it, none. Below C3, 98 % of the notes are bit for bit the C++'s; from C3 to B4, 77 %;
above C6 almost every note differs, but by 50 to 90 dB under the sound. What can be heard: on a
few high notes (Clang snare and Glass shards at B6, Kalimba at C♯6, Tin hat at B8), a mode near
7.5 to 8 kHz the C++ leaves ringing for seconds dies with the rest, 26 to 30 dB under the sound.
`factory_levels.rs`'s `no_factory_sound_rings_on_where_elements_runs_away` plays each of the 22
factory sounds that run away at a whole key at one such key, and fails at every one with the limit
off.

The song's toy piano, rendered again (141 s): the 10.4 kHz line, which made the track 16 dB
louder in all, is gone; below 7 kHz, where the runaway was quiet, the track is the same to 45 to 70 dB.
Where it was loud it had held voices and driven the shared soft limit, so the music under it
differs there too.

## The guard

A string with Damping at its top crossfades to an infinite decay and never ends on its own. A
released note still sounding 30 s after its key went up, its envelopes at rest, is faded over
10 ms and stopped (`a_sound_that_never_ends_is_ended_thirty_seconds_after_its_release`): a guard,
not a design, kept by the owner (2026-11-05).

## Choices copied with the envelopes and LFOs

From MXM Thanks EG, unchanged (its `NOTES.md` has the reasons): the Response curve
`(e^{6r·x} − 1)/(e^{6r} − 1)`; Rise and Fall linear in seconds; smooth random by a raised cosine;
exponential Glide, in Poly from the last note played; Pan as a balance, unity at the centre; notes
waiting for the next render. The LFO's top rate is a fifth of the block rate, now 400 Hz at the
module's 2 kHz blocks.
