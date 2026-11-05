# NOTES.md — mxm-modal-synth

The reasoning behind `AGENTS.md`, and the owner's rulings. The plan, with the facts about Elements
it was designed from, is ops' `plans/plan-elements.md`; Pi reviewed it in three rounds (ops'
`plans/review-elements-2026-11-05*.md`).

## The owner's rulings

- **2026-11-05: The instrument.** *"It could be great to do Elements the exact same way that we did
  plaits. Make a plan for that. Same modules and all."* Elements is a monophonic Eurorack module
  with one voice and no modulation of its own a player reaches; everything else comes in on a
  jack. So, as MXM Thanks EG did with Plaits, the plugin is the module's voice with the case around
  it: envelopes, LFOs and the performance sources, eight voices.
- **2026-11-05: A free crate too.** *"And a free rust crate too"*: the port is
  `mxm-audio/mxm-modal-voice` (MIT, as the code it is ported from), bit identical to Elements' C++
  and held there by 106 reference cases; this repository takes it by its `main`, locked. Private
  until the owner says otherwise.
- **2026-11-05: The names.** *"name is fine"*, then *"i approved the default name"*: the instrument
  is **MXM Modal Synth** and the crate **mxm-modal-voice**, named by kind, as Elements is a modal
  synthesis voice. Emilie Gillet's *Guidelines for derivative works* ask that "Mutable Instruments"
  not be used on derivative works and recommend not keeping a module's name, so "Elements" names
  the original in `NOTICE.md` and the README only.
- **2026-11-05: The recordings.** *"they are released with the projects so we considder them MIT as
  well."* The nine strike recordings and the noise ship as Elements has them.
- **2026-11-05: No audio input now**, as the conventions have an instrument; the module's inputs
  stay silent, and one may come later as an auxiliary port.
- **2026-11-05: Copies, not shared code.** *"Option C. This synthe and thanks Eg might change later,
  so the shared slots are mainly by accident."*, and of the envelopes and LFOs, *"yes, copy them
  too"*. The route slots, the envelopes, the LFOs, the plugin's shell and the editor's shape are
  Thanks EG's, copied, so each instrument can change its own. Only the rate converter moved into
  mxm-kit's `mxm-dsp`, shared with Thanks EG (the owner, 2026-11-05: "move it into the kit"; and
  "Rate converter is more correct", the drum machines' Resample being another thing).

## The editor

Thanks EG's editor with this voice's cards: mxm-mono-pr1's shape for a synth's cards, the app bar
and the preset row; mxm-drum-machine's for route slots as rows on the card of the control they
move. No new widget.

- **The module's panel, in cards.** Elements puts Contour and the three exciters' levels in one
  row of sliders, each exciter's timbres under it, and the resonator's knobs with Space in another.
  The cards follow: Exciter (Contour, Bow, Blow, Strike, with Strength's routes, the accent of all
  three), Bow and blow (Bow timbre, Flow, Blow timbre), Strike (Mallet, Timbre), Resonator (with
  Space). Model, Coarse and Fine are one card, the model a switch of four.
- **Why the cards are short.** The first build had a card per exciter and one for Space: fourteen
  cards, and at the quarter-4K budget the pager put Voice and the envelopes on one page and the rest
  on another. Folding the exciters into one card made a card of ten target lines, taller than a
  1280 × 800 page. With at most five lines a card, the second row (LFO 3 and the five sound cards)
  sits under the envelopes' first and all twelve open on one page, as Thanks EG's do. Pan and
  Amplitude, which have no knob, sit on Voice, where how loud each voice sounds and where belongs.
- **Which free slot a choice takes, and the full line**: Thanks EG's (its `NOTES.md`).
- **`‹ modulate ›` from the keyboard** (the owner, 2026-11-05: "Fix it in the kit now", on Pi's
  review): the picker is not a parameter, so the cursor could not reach it, here, in the drum
  machine or in Thanks EG, and a first route needed the mouse. It is now a stop with a key of its
  own (`route_rows::modulate_key`), and the kit's selectors open their list on MENU, ↑ ↓ and Enter
  picking (`a_route_is_added_from_the_keyboard`).

## The factory set

**Sixty sounds, each one idea pushed to its end** (the owner, 2026-11-05, on hearing the first set:
*"They sound the same. Be more creative. go wild."*). The first set kept every control near its
middle (Geometry 0.2–0.6, Position 0.2–0.4, Space 0.5–0.95) in one register, so it sounded alike;
this one goes to the module's ends, read off its source (`research:` the Elements page):

- **The room**: Space carried past 1.75 by an envelope freezes the reverb while a key is held
  (*Frozen bells*, *Freeze bloom*, *Frozen stab*); Space at nothing puts the raw exciter on the left
  (*Split breath*).
- **The exciters at their ends**: the plectrum's double strike at its top (*Flam tom*), particles
  re-exciting a held string (*Dust harp*, *Tanpura*), the recorded hits (*Prepared piano*,
  *Kalimba*, *Glass shards*), the bow just over its threshold (*Scraped metal*), the tube
  (*Didgeridoo*, *Duck call*).
- **The resonators at their ends**: the string's bridge buzzing at Geometry 0 (*Sitar*, *Buzz
  bass*, *Rattlesnake*), a plate (*Tin plate keys*, *Thunder sheet*), a bell, the pickup at the
  bridge (*Clav string*, *Sul ponticello*) or the middle (*Pan flute*).
- **What the instrument adds**: pitch drops on an envelope (*Deep kick*, *Laser*), chords that change
  by themselves (*Chord cycler*, *Chord stepper*), a model chosen per note (*Model roulette*),
  random pitch (*Pitch rain*), a sample-and-hold babbling (*Computer babble*), a comb sweeping
  (*Underwater*), pan and pitch swinging (*Doppler*), cycling breath (*Ratchet breath*); and the dark
  FM voice as piano, acid and growling bass, screaming lead, theremin, horn, swarm and nebula.

Every register from two octaves down to two up. **Approved by the owner** in MXM Player
(2026-11-05: *"That is MUCH better"*, then *"the sounds are approved"*).

**Levelled in the sound, never by Volume**, to the set's ±5 LU window (`tests/factory_levels.rs`):

- **A struck sound by its accent**, Envelope 3 held for the note into Strength. The accent returns
  to rest 0.3 s after the release, which a strike never hears; the strike lands while the
  envelope is still rising, so a large accent reaches the module's top sooner rather than going
  past it. A Bass sound sits no lower than an octave down: two octaves down, the phrase's notes
  fall under 25 Hz, below what the loudness measure weighs.
- **A bowed or blown sound by its levels, never cut by the accent**: a cut that ends 0.3 s after the
  release would swell the exciter's release by as much. Held chords of these sum into the part's
  one soft limit (the DSP's `NOTES.md`), so a −10 dB accent moved a pad by about 4 LU; the levels
  were found by sweeping them. The bow on the modal resonator speaks between 0.3 and 0.5, and the
  tube sounds' level comes from their timbres, Brightness and Damping, since above two thirds of
  Blow the noise holds at its top while the tube comes in.
- **The dark FM voice by its oscillators and its filter**: Strength only adds to its envelope, whose
  VCA is open once it sustains, and Geometry is its filter's mode, louder towards low-pass.

## Open points, for the owner (E5)

- **The hidden voice's dry drone ends with its note**, 10 ms after its gated outputs fall silent; on
  the module it drones while the voice runs. A recorded deviation (the DSP's `NOTES.md`).
- **A string with Damping at its top never decays**; the instrument fades a released note out 30 s
  after its key went up. A guard, not a design.
- **One soft limit for all the voices**, as the module has for its one: chords of sustained sounds
  are flattened by it. Kept, because one voice must be the module's part to the bit; a per-voice
  headroom would change that.
- **"Env n input"** names the Follow mode's target as the plan and the DSP do, though the
  conventions say a target is never called *Input*.
- **Copies the kit could hold**: the time reading. The route slots stay copies by ruling.
