# MXM Modal Synth

A polyphonic modal synthesizer, CLAP and VST®3: bars, plates, strings and tubes, struck, bowed and
blown, with envelopes that are function generators too, LFOs and sixteen modulation slots. Its
voice is ported to Rust from Emilie Gillet's code for Mutable Instruments Elements, which she
published under the MIT licence. She has no part in this product and does not endorse it.

Eight voices, each a whole modal voice: a bow, a blow and a strike exciting a modal resonator, a
string or a set of strings tuned to a chord, and a dark FM voice beside them; one room for all.
Per voice three envelopes, each also a function generator (cycling, gated, or following another
signal), and three LFOs; sixteen route slots from nineteen sources to forty targets; sixty
factory sounds. The voice is held bit for bit to the code it was ported from; the port is its own
crate, [mxm-modal-voice](https://github.com/mxm-audio/mxm-modal-voice) (MIT).
`cargo xtask bundle mxm-modal-synth --release` builds and installs the CLAP and VST3.

Pre-alpha: nothing is released yet. The sounds and the parameters are settled; the parameters are
frozen, so a saved project opens in every later version.

## Licence

GPL-3.0-or-later (`LICENSE`), with the MIT-licensed code it is ported from named in `NOTICE.md`.
The MXM name and logo are not covered by the licence: see `TRADEMARKS.md`. VST is a registered
trademark of Steinberg Media Technologies GmbH.
