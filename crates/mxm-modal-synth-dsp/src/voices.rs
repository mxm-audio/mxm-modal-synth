//! The voices and how they take notes.
//!
//! **Poly**: a new note takes an idle voice, else the released one sounding longest, else the
//! held one sounding longest. With Glide set, each new note glides from the last note played.
//! **Mono** and **Legato** play voice 0 from a stack of held keys, last-note priority: Mono
//! retriggers on every new key and glides when Glide is set; Legato glides to a key played over a
//! held one without a new trigger, and a new key after every key is up starts afresh. In both, a
//! released key returns to the one held before it, without a trigger.
//!
//! An event that names a note id acts on the voice playing that id; one with no id, or on a voice
//! that has none, on the voices playing its key ([`Voice::answers`]).

use mxm_modulation::standard;

use crate::lfo::Rng;
use crate::voice::Voice;
use crate::{Mode, Settings, VOICES};

/// Keys a Mono or Legato voice remembers, newest last.
const HELD: usize = 16;

#[derive(Clone, Copy, Debug, Default)]
struct Held {
    note: u8,
    id: Option<i32>,
}

pub(crate) struct Voices {
    pub voices: [Voice; VOICES],
    held: [Held; HELD],
    held_len: usize,
    /// Note-ons so far: a voice's `started` is its place in them.
    count: u64,
    rng: Rng,
    /// The last key played, for Poly's glide.
    last_key: Option<f32>,
}

const SEED: u32 = 0x6d2b_79f5;

impl Voices {
    pub(crate) fn new() -> Self {
        Self {
            voices: std::array::from_fn(Voice::new),
            held: [Held::default(); HELD],
            held_len: 0,
            count: 0,
            rng: Rng::new(SEED),
            last_key: None,
        }
    }

    /// Every voice and every state as made; allocates nothing.
    pub(crate) fn clear(&mut self) {
        for v in &mut self.voices {
            v.clear();
        }
        self.held_len = 0;
        self.count = 0;
        self.rng = Rng::new(SEED);
        self.last_key = None;
    }

    pub(crate) fn note_on(&mut self, s: &Settings, note: u8, velocity: f32, id: Option<i32>) {
        let velocity = if velocity.is_finite() { velocity } else { 1.0 };
        match s.mode {
            Mode::Poly => {
                let v = self.pick();
                let from = if s.glide_s > 0.0 { self.last_key } else { None };
                self.last_key = Some(f32::from(note));
                self.start(s, v, note, velocity, id, from);
            }
            Mode::Mono | Mode::Legato => {
                if let Some(i) = self.find_held(note, None) {
                    self.remove_held(i);
                }
                if self.held_len == HELD {
                    self.remove_held(0);
                }
                let tied = s.mode == Mode::Legato && self.held_len > 0 && self.voices[0].gate;
                self.held[self.held_len] = Held { note, id };
                self.held_len += 1;
                if tied {
                    self.voices[0].tie(s, note, id);
                } else {
                    let voice = &self.voices[0];
                    let from = (s.mode == Mode::Mono && s.glide_s > 0.0 && voice.active)
                        .then(|| voice.pitch());
                    self.start(s, 0, note, velocity, id, from);
                }
            }
        }
    }

    fn start(
        &mut self,
        s: &Settings,
        v: usize,
        note: u8,
        velocity: f32,
        id: Option<i32>,
        glide_from: Option<f32>,
    ) {
        self.count += 1;
        let random = standard::random(self.rng.unit());
        self.voices[v].start(
            s,
            note,
            id,
            standard::velocity(velocity),
            random,
            self.count,
            glide_from,
        );
    }

    /// Poly's choice: an idle voice (the one used longest ago), else the released one sounding
    /// longest, else the held one sounding longest.
    fn pick(&self) -> usize {
        let oldest = |want: &dyn Fn(&Voice) -> bool| {
            (0..VOICES)
                .filter(|&i| want(&self.voices[i]))
                .min_by_key(|&i| self.voices[i].started)
        };
        oldest(&|v| !v.active)
            .or_else(|| oldest(&|v| !v.gate))
            .or_else(|| oldest(&|_| true))
            .unwrap_or(0)
    }

    pub(crate) fn note_off(&mut self, s: &Settings, note: u8, id: Option<i32>) {
        match s.mode {
            Mode::Poly => {
                for v in &mut self.voices {
                    if v.gate && v.answers(note, id) {
                        v.release();
                    }
                }
            }
            Mode::Mono | Mode::Legato => {
                if let Some(i) = self.find_held(note, id) {
                    let top = i + 1 == self.held_len;
                    self.remove_held(i);
                    if top {
                        self.follow_stack(s);
                    }
                }
            }
        }
    }

    /// A note off for an id alone: the voice playing it, or nothing.
    pub(crate) fn note_off_id(&mut self, s: &Settings, id: i32) {
        match s.mode {
            Mode::Poly => {
                for v in &mut self.voices {
                    if v.gate && v.id == Some(id) {
                        v.release();
                    }
                }
            }
            Mode::Mono | Mode::Legato => {
                if let Some(i) = self.held[..self.held_len]
                    .iter()
                    .position(|h| h.id == Some(id))
                {
                    let top = i + 1 == self.held_len;
                    self.remove_held(i);
                    if top {
                        self.follow_stack(s);
                    }
                }
            }
        }
    }

    /// After the newest key went up: back to the key held before it, or the gate off.
    fn follow_stack(&mut self, s: &Settings) {
        let voice = &mut self.voices[0];
        if self.held_len > 0 {
            let h = self.held[self.held_len - 1];
            if voice.gate {
                voice.tie(s, h.note, h.id);
            }
        } else {
            voice.release();
        }
    }

    /// Every voice's gate off, as every held key's note off.
    pub(crate) fn release_all(&mut self) {
        for v in &mut self.voices {
            v.release();
        }
        self.held_len = 0;
    }

    /// `None`: every note. A choked voice is silent at once.
    pub(crate) fn choke(&mut self, note: Option<u8>) {
        match note {
            None => {
                for v in &mut self.voices {
                    v.choke();
                }
                self.held_len = 0;
            }
            Some(n) => {
                for v in &mut self.voices {
                    if v.active && v.note == n {
                        v.choke();
                    }
                }
                if let Some(i) = self.find_held(n, None) {
                    self.remove_held(i);
                }
                if !self.voices[0].active {
                    self.held_len = 0;
                }
            }
        }
    }

    pub(crate) fn choke_id(&mut self, id: i32) {
        for v in &mut self.voices {
            if v.active && v.id == Some(id) {
                v.choke();
            }
        }
        if let Some(i) = self.held[..self.held_len]
            .iter()
            .position(|h| h.id == Some(id))
        {
            self.remove_held(i);
        }
        if !self.voices[0].active {
            self.held_len = 0;
        }
    }

    /// A per-note pitch expression; a non-finite one is dropped.
    pub(crate) fn set_tuning(&mut self, note: u8, id: Option<i32>, semitones: f32) {
        if !semitones.is_finite() {
            return;
        }
        for v in &mut self.voices {
            if v.active && v.answers(note, id) {
                v.tuning = semitones;
            }
        }
    }

    pub(crate) fn set_tuning_id(&mut self, id: i32, semitones: f32) {
        if !semitones.is_finite() {
            return;
        }
        for v in &mut self.voices {
            if v.active && v.id == Some(id) {
                v.tuning = semitones;
            }
        }
    }

    fn find_held(&self, note: u8, id: Option<i32>) -> Option<usize> {
        let held = &self.held[..self.held_len];
        // Newest first, so a key pressed twice lets go of its latest press.
        held.iter()
            .rposition(|h| matches!((id, h.id), (Some(a), Some(b)) if a == b))
            .or_else(|| {
                held.iter().rposition(|h| match (id, h.id) {
                    (Some(_), Some(_)) => false,
                    _ => h.note == note,
                })
            })
    }

    fn remove_held(&mut self, i: usize) {
        self.held.copy_within(i + 1..self.held_len, i);
        self.held_len -= 1;
    }

    #[cfg(test)]
    pub(crate) fn held(&self) -> Vec<u8> {
        self.held[..self.held_len].iter().map(|h| h.note).collect()
    }
}
