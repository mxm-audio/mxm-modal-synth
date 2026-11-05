//! The part the voices share, written from Elements' `Part::Process` (`mxm_modal_voice::Part`,
//! which keeps the C++'s one voice and one patch for the reference cases): each voice mixed to
//! stereo by its own Space, as `Part` mixes its one; **one soft limit and one reverb for all**, as
//! `Part` has; the module's frames, `SoftConvert`.
//!
//! The soft limit takes the voices playing a resonator; the hidden voices, which `Part` leaves
//! unlimited (`if (!easter_egg_)`), are added after it. A group with no voice in it adds nothing,
//! so one voice sounding, of either kind, is `Part` to the bit, signed zeros too
//! (`one_voice_is_the_part_exactly`). The reverb follows the newest voice's Space, as `Part`'s
//! follows its one patch; with every voice silent it holds. The metering and the panic are
//! `Part`'s: when the mix's level passes 200 the next block resets every resonator and plays
//! the inputs, which here are silent, the voices and the reverb standing still for it.

use mxm_modal_voice::fx::reverb::Reverb;
use mxm_modal_voice::stmlib::dsp::dsp::{soft_convert, soft_limit};

use crate::VOICES;
use crate::voice::{BLOCK, Output, REVERB_DIFFUSION, REVERB_LP, Shared, Voice};

/// Blocks of silent frames after which the reverb's tail is over: 0.5 s, longer than its longest
/// delay (6 312 samples, 0.2 s), so memory still ringing shows in the frames before then.
const TAIL_BLOCKS: u32 = 1_000;

/// `Part::Process`'s dry gain from Space: the exciter's raw signal into the left, 1 up to 0.05,
/// falling to 0 at 0.1.
pub(crate) fn raw_gain(space: f32) -> f32 {
    let space = if space >= 1.0 { 1.0 } else { space };
    if space <= 0.05 {
        1.0
    } else if space <= 0.1 {
        2.0 - space * 20.0
    } else {
        0.0
    }
}

/// One group's mix, main (the module's right) and aux (its left).
struct Group {
    any: bool,
    main: [f32; BLOCK],
    aux: [f32; BLOCK],
}

impl Group {
    fn new() -> Self {
        Self {
            any: false,
            main: [0.0; BLOCK],
            aux: [0.0; BLOCK],
        }
    }

    /// `Part::Process`'s mixdown of a voice, by its own Space, through its gains (at unity, the
    /// same bits: `x × 1` is `x`).
    fn add(&mut self, o: &Output) {
        self.any = true;
        let raw_gain = raw_gain(o.space);
        let mut space = if o.space >= 1.0 { 1.0 } else { o.space };
        space = if space >= 0.1 { space - 0.1 } else { 0.0 };
        let spread = if space <= 0.7 { space } else { 0.7 };
        for j in 0..BLOCK {
            let t = (j + 1) as f32 / BLOCK as f32;
            let left = o.from[0] + (o.to[0] - o.from[0]) * t;
            let right = o.from[1] + (o.to[1] - o.from[1]) * t;
            let side = o.sides[j] * spread;
            let r = o.center[j] - side;
            let l = o.center[j] + side;
            self.main[j] += r * right;
            self.aux[j] += (l + (o.raw[j] - l) * raw_gain) * left;
        }
    }
}

pub(crate) struct Part {
    reverb: Box<Reverb>,
    panic: bool,
    resonator_level: f32,
    /// The newest voice's Space, which the reverb follows; held while every voice is silent.
    space: f32,
    /// Blocks since the last frame that was not zero.
    quiet: u32,
    /// The last block's two outputs, before the frames: for the test that holds it to `Part`.
    #[cfg(test)]
    pub(crate) last: ([f32; BLOCK], [f32; BLOCK]),
}

impl Part {
    pub(crate) fn new() -> Self {
        let mut reverb = Box::<Reverb>::default();
        reverb.init();
        Self {
            reverb,
            panic: false,
            resonator_level: 0.0,
            space: crate::Settings::default().space,
            quiet: TAIL_BLOCKS,
            #[cfg(test)]
            last: ([0.0; BLOCK], [0.0; BLOCK]),
        }
    }

    /// As `new` made it; allocates nothing.
    pub(crate) fn clear(&mut self) {
        self.reverb.clear();
        self.reverb.init();
        self.panic = false;
        self.resonator_level = 0.0;
        self.space = crate::Settings::default().space;
        self.quiet = TAIL_BLOCKS;
        #[cfg(test)]
        {
            self.last = ([0.0; BLOCK], [0.0; BLOCK]);
        }
    }

    /// Whether the frames have not been silent for half a second.
    pub(crate) fn ringing(&self) -> bool {
        self.quiet < TAIL_BLOCKS
    }

    /// One block: every active voice rendered and mixed, the frames, left and right.
    pub(crate) fn process(
        &mut self,
        shared: &Shared,
        voices: &mut [Voice; VOICES],
    ) -> [[i16; 2]; BLOCK] {
        let mut frames = [[0i16; 2]; BLOCK];
        if self.panic {
            // `Part::Process`'s bypass: every resonator reset, the level cleared, the inputs
            // (silent) played.
            for v in voices.iter_mut() {
                v.panic();
            }
            self.resonator_level = 0.0;
            self.panic = false;
            self.count(&frames);
            #[cfg(test)]
            {
                self.last = ([0.0; BLOCK], [0.0; BLOCK]);
            }
            return frames;
        }

        let mut elements = Group::new();
        let mut hidden = Group::new();
        let mut newest: Option<(u64, f32)> = None;
        for v in voices.iter_mut() {
            if !v.active {
                continue;
            }
            v.process(shared);
            let o = &v.output;
            if o.hidden {
                hidden.add(o);
            } else {
                elements.add(o);
            }
            if newest.is_none_or(|(started, _)| v.started > started) {
                newest = Some((v.started, o.space));
            }
        }
        if let Some((_, space)) = newest {
            self.space = space;
        }

        // Pre-clipping, the resonators' voices only.
        if elements.any {
            for i in 0..BLOCK {
                elements.main[i] = soft_limit(elements.main[i]);
                elements.aux[i] = soft_limit(elements.aux[i]);
            }
        }
        let (mut main, mut aux) = match (elements.any, hidden.any) {
            (true, true) => {
                let mut main = elements.main;
                let mut aux = elements.aux;
                for i in 0..BLOCK {
                    main[i] += hidden.main[i];
                    aux[i] += hidden.aux[i];
                }
                (main, aux)
            }
            (true, false) => (elements.main, elements.aux),
            (false, true) => (hidden.main, hidden.aux),
            (false, false) => ([0.0; BLOCK], [0.0; BLOCK]),
        };

        // Metering, and the panic it arms.
        let mut resonator_level = self.resonator_level;
        for &m in &main {
            let error = m * m - resonator_level;
            resonator_level += error * if error > 0.0 { 0.05 } else { 0.0005 };
        }
        self.resonator_level = resonator_level;
        if resonator_level >= 200.0 {
            self.panic = true;
        }

        // The reverb, from the newest voice's Space, as `Part` sets it from its patch's.
        let mut space = if self.space >= 1.0 { 1.0 } else { self.space };
        space = if space >= 0.1 { space - 0.1 } else { 0.0 };
        let reverb_amount = if space >= 0.5 {
            1.0 * (space - 0.5)
        } else {
            0.0
        };
        let reverb_time = 0.35 + 1.2 * reverb_amount;
        self.reverb.set_amount(reverb_amount);
        self.reverb.set_diffusion(REVERB_DIFFUSION);
        if self.space >= 1.75 {
            self.reverb.set_time(1.0);
            self.reverb.set_input_gain(0.0);
            self.reverb.set_lp(1.0);
        } else {
            self.reverb.set_time(reverb_time);
            self.reverb.set_input_gain(0.2);
            self.reverb.set_lp(REVERB_LP);
        }
        self.reverb.process(&mut main, &mut aux, BLOCK);

        // `elements.cc`'s output stage: left the aux, right the main.
        for (frame, (&m, &a)) in frames.iter_mut().zip(main.iter().zip(&aux)) {
            *frame = [soft_convert(a), soft_convert(m)];
        }
        self.count(&frames);
        #[cfg(test)]
        {
            self.last = (main, aux);
        }
        frames
    }

    fn count(&mut self, frames: &[[i16; 2]; BLOCK]) {
        if frames.iter().all(|f| f[0] == 0 && f[1] == 0) {
            self.quiet = self.quiet.saturating_add(1);
        } else {
            self.quiet = 0;
        }
    }
}
