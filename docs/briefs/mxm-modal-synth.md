# mxm-modal-synth — UI design brief

Required by mxm-kit's [`MXM_DESIGN_SYSTEM.md`](https://github.com/mxm-audio/mxm-kit/blob/main/docs/MXM_DESIGN_SYSTEM.md)
§14. The instrument plays the modal synthesis voice of a published Eurorack module, ported to Rust
from its maker's MIT code and held bit for bit to it (`mxm-modal-voice`), with envelopes, LFOs and
route slots around it (`crates/mxm-modal-synth-dsp`). The design is ops' `plans/plan-elements.md`
§5; the cards follow the module's panel, a row of levels over each exciter's timbres.

**Brief status: written after the editor's first build, 2026-11-05**, from the plan's §5 and MXM
Thanks EG's editor, whose shape it copies. Automated tree, layout, cursor and picture checks pass;
the owner's listening, a real DAW, native-window fit and design-system §15 QA remain the release
gates.

## Product boundary

- **The voice is the module's**: what the exciters and the resonators do is the module's, as it has
  them. The instrument adds what the module took from its case: eight voices, three envelopes that
  are function generators too, three LFOs and sixteen route slots.
- **The module's room is in**: its reverb, which Space reaches, is the voice's, one for all voices
  (the conventions' *An instrument ships the effects its original had*).
- **No maker's or module's name** anywhere in the editor; the models carry the instrument's names
  (`params::MODEL_NAMES`: Modal, String, Strings, Dark FM).

## Established warts to preserve

The module's, ported and kept: its 16-bit output, the resonators' one soft limit (held chords of
bowed or blown sounds flatten in it), a string that never decays with Damping at its top (ended by
the instrument 30 s after release, a guard), and the hidden voice's ungated drone at low Space
while its note is held. See `research:physical-modelling/mutable-instruments-elements.md`.

## 1. Primary sound-design task

Choose what sounds (the model), excite it (strike, bow, blow, and Contour for the last two), then
shape the resonator (Geometry, Brightness, Damping, Position) and its room (Space); then make it
move with an envelope or LFO.

## 2. The parameters users reach for most

Model, the three exciters' levels, Mallet, Geometry, Brightness, Damping, Position, Space.

## 3. Signal flow that must be visible

Exciters (Contour and levels; each one's timbres) → the resonator (Model, Coarse, Fine; Geometry,
Brightness, Damping, Position) → Space → Volume in the app bar. Modulators reach any of these
through route slots, drawn as rows on the card of the control they move.

## 4. Play view

The single page at the opening size: every card shown; nothing is behind a disclosure.

## 5. Advanced controls and disclosure

None hidden: the route slots appear as rows only when in use, and a card's *modulate* line adds
one. When all sixteen are in use, every card's line says so.

## 6. Categories, cards and groups

In the collection's category order: **Performance** (Voice: Mode, Glide, Bend range, with Amplitude
and Pan's routes); **Modulators** (Envelope 1–3, LFO 1–3); **Generators** (Model, Exciter, Bow and
blow, Strike); **Tone** (Resonator, with Space). No preferred groups, as Thanks EG's.

## 7. Identity accent

The shared collection accent (dark `#4CC9D8`, light `#247F91`); no accent of the module's.

## 8. Live visualizations

The meter in the app bar; each LFO's shape drawn in its picker. A drawn exciter envelope and live
modulator levels would need the DSP to expose them: later.

## 9. What is removed from the source's panel, and why

The hidden gestures that choose the resonator's model and wake the hidden voice (Model is a switch
of four), the CV and audio inputs (route slots; no audio input now), the gate input (the keyboard),
the FM attenuverter (a slot on Pitch), the octave of Coarse beyond three either way.

## 10. Quarter-4K fit, smaller sizes and 200% zoom

Measured by the editor's tests, not guessed: it opens at 1821 × 1080 logical points with all
twelve cards on one page; the minimum is 446 × 360, set by the app bar. Every page passes the tree
and layout checks in both themes. **No native-window, DPI or 200% run on a real display yet**: §15
responsive QA remains a manual gate.

## Init patch

The module's power-on patch: the modal resonator, struck at 0.8 with Mallet and Strike timbre at
the middle, Contour 1, bow and blow off; Geometry 0.2, Brightness 0.5, Damping 0.25, Position 0.3,
Space 0.5. Poly, no glide, bend ±2 semitones; each envelope 5 ms / 300 ms / 70 % / 300 ms, linear,
Note mode, not cycling; the LFOs at 5, 1 and 0.2 Hz, sine, free-running, no fade; no route slot in
use; Volume −3 dB. Every amount at zero, one set of numbers with the CLAP defaults.
