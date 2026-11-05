//! The cards and every parameter's binding.
//!
//! Each card is a `mxm_ui::tree` (`plans/plan-layout-tree.md`): [`card`] describes its body once,
//! and that one description is both measured — its floor and its height — and drawn, leaf by leaf,
//! through the bindings below ([`paint`]). Nothing is typed and nothing is drawn to learn a size.
//!
//! **The cards, in the conventions' category order** (`docs/plugin-conventions.md`, *Editor
//! contract*): Voice (Performance); the three envelopes and the three LFOs (Modulators); the
//! model and its tuning, then the exciters as the module's panel lays them out, Contour and the
//! three levels over each one's timbres (Generators); the resonator with Space (Tone). **Each
//! target's routes are rows on the card of the control it moves** ([`super::route_rows`]); Volume,
//! the instrument's output level, is the app bar's.

use std::collections::HashMap;

use egui::Ui;
use mxm_modal_synth_dsp::Target;
use mxm_ui::control::{Size, Wave};
use mxm_ui::paging::Category;
use mxm_ui::space::SPACE_3;
use mxm_ui::theme::Tokens;
use mxm_ui::tree::{self, Height, Kind, Node, leaf, pad, stack};
use nice_plug::prelude::ParamSetter;

use super::binding::{self, Bound, ErasedParam, StepLaw};
use super::route_rows;
use crate::params::{ENVELOPE_IDS, LFO_IDS, LFO_SYNC, ModalSynthParams, VOICE_IDS};
use crate::routes::ROUTE_IDS;

/// Every knob is Standard: each always shows its reading, a synced rate's division among them
/// (`docs/plugin-conventions.md`, *A syncable rate or time shows its reading*).
const STANDARD: Size = Size::Standard;

/// One card of the editor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Card {
    /// How the voices take notes: Mode, Glide, Bend range; and how loud each sounds and where,
    /// the routes of Amplitude and Pan.
    Voice,
    /// Envelope `n`, from 0: its seven, and the routes of its six targets.
    Envelope(usize),
    /// LFO `n`, from 0: its five, and its Rate's routes.
    Lfo(usize),
    /// Model, Coarse and Fine, with the routes of Pitch and Model.
    Model,
    /// Contour, the envelope of the bow and the blow, and the three exciters' levels, with their
    /// routes and Strength's, the accent of all three.
    Exciter,
    /// The bow's timbre, the blow's Flow and timbre, with their routes.
    BowAndBlow,
    /// The strike's Mallet and timbre, with their routes.
    Strike,
    /// Geometry, Brightness, Damping, Position and Space, with their routes.
    Resonator,
}

/// The cards in paging order: the category order, then the signal's.
pub const CARDS: [Card; 12] = [
    Card::Voice,
    Card::Envelope(0),
    Card::Envelope(1),
    Card::Envelope(2),
    Card::Lfo(0),
    Card::Lfo(1),
    Card::Lfo(2),
    Card::Model,
    Card::Exciter,
    Card::BowAndBlow,
    Card::Strike,
    Card::Resonator,
];

/// Each envelope's targets, in the order its card draws their routes: the times, then the gates.
pub const ENVELOPE_TARGETS: [[Target; 6]; 3] = [
    [
        Target::Envelope1Rise,
        Target::Envelope1Fall,
        Target::Envelope1Time,
        Target::Envelope1Cycle,
        Target::Envelope1Trigger,
        Target::Envelope1Input,
    ],
    [
        Target::Envelope2Rise,
        Target::Envelope2Fall,
        Target::Envelope2Time,
        Target::Envelope2Cycle,
        Target::Envelope2Trigger,
        Target::Envelope2Input,
    ],
    [
        Target::Envelope3Rise,
        Target::Envelope3Fall,
        Target::Envelope3Time,
        Target::Envelope3Cycle,
        Target::Envelope3Trigger,
        Target::Envelope3Input,
    ],
];

/// Each LFO's one target, its Rate.
pub const LFO_TARGETS: [Target; 3] = [Target::Lfo1Rate, Target::Lfo2Rate, Target::Lfo3Rate];

impl Card {
    pub const fn title(self) -> &'static str {
        match self {
            Self::Voice => "Voice",
            Self::Envelope(0) => "Envelope 1",
            Self::Envelope(1) => "Envelope 2",
            Self::Envelope(_) => "Envelope 3",
            Self::Lfo(0) => "LFO 1",
            Self::Lfo(1) => "LFO 2",
            Self::Lfo(_) => "LFO 3",
            Self::Model => "Model",
            Self::Exciter => "Exciter",
            Self::BowAndBlow => "Bow and blow",
            Self::Strike => "Strike",
            Self::Resonator => "Resonator",
        }
    }

    /// Its primary category (design system §3.4).
    pub const fn category(self) -> Category {
        match self {
            Self::Voice => Category::Performance,
            Self::Envelope(_) | Self::Lfo(_) => Category::Modulators,
            Self::Model | Self::Exciter | Self::BowAndBlow | Self::Strike => Category::Generators,
            Self::Resonator => Category::Tone,
        }
    }

    /// The name of a run of like cards, for a page that splits a category.
    pub const fn kind(self) -> &'static str {
        match self {
            Self::Voice => "Voice",
            Self::Envelope(_) => "Envelope",
            Self::Lfo(_) => "LFO",
            Self::Model => "Model",
            Self::Exciter | Self::BowAndBlow | Self::Strike => "Exciter",
            Self::Resonator => "Resonator",
        }
    }

    /// **The musician controls the card owns**, by permanent id. Route slots are not listed: a row
    /// is drawn because its slot is in use, so at Init none is ([`Card::targets`] is where they go).
    pub fn parameters(self) -> Vec<&'static str> {
        match self {
            Self::Voice => vec!["mode", "glide", "bend_range"],
            Self::Envelope(n) => ENVELOPE_IDS[n].to_vec(),
            Self::Lfo(n) => LFO_IDS[n].to_vec(),
            Self::Model => vec!["model", "coarse", "fine"],
            Self::Exciter => vec!["contour", "bow", "blow", "strike"],
            Self::BowAndBlow => vec!["bow_timbre", "flow", "blow_timbre"],
            Self::Strike => vec!["mallet", "strike_timbre"],
            Self::Resonator => vec!["geometry", "brightness", "damping", "position", "space"],
        }
    }

    /// **The targets whose routes are rows on this card**: each target on exactly one card, the
    /// card of the control it moves.
    pub fn targets(self) -> &'static [Target] {
        match self {
            Self::Voice => &[Target::Amplitude, Target::Pan],
            Self::Envelope(n) => &ENVELOPE_TARGETS[n],
            Self::Lfo(n) => std::slice::from_ref(&LFO_TARGETS[n]),
            Self::Model => &[Target::Pitch, Target::Model],
            Self::Exciter => &[
                Target::Contour,
                Target::Bow,
                Target::Blow,
                Target::Strike,
                Target::Strength,
            ],
            Self::BowAndBlow => &[Target::BowTimbre, Target::Flow, Target::BlowTimbre],
            Self::Strike => &[Target::Mallet, Target::StrikeTimbre],
            Self::Resonator => &[
                Target::Geometry,
                Target::Brightness,
                Target::Damping,
                Target::Position,
                Target::Space,
            ],
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The cards, as trees.
// ---------------------------------------------------------------------------------------------

/// What a leaf of this editor's cards draws, by the permanent id of what it edits. Hashed by what
/// it names, which is also what keeps its widget ids stable when a route appears above it.
#[derive(Clone, Debug, Hash)]
pub enum Leaf {
    Knob(&'static str),
    /// One of an envelope's A, D, S and R, in the collection's fader row.
    Fader(&'static str),
    /// A stepped switch as a segmented control.
    Switch(&'static str),
    /// An on/off switch under its name.
    Toggle(&'static str),
    /// An LFO's tempo sync, the quarter note beside its Rate.
    Sync(&'static str),
    /// An LFO's shapes, drawn.
    Shape(&'static str),
    /// A target's routes, by its index in `Target::ALL`.
    Routes(usize),
}

/// The LFO shapes, in `LfoShapeChoice`'s order, drawn rather than named (design system §7.3).
pub const LFO_SHAPES: [(Wave, &str); 7] = [
    (Wave::Sine, "Sine"),
    (Wave::Triangle, "Triangle"),
    (Wave::RampUp, "Saw up"),
    (Wave::RampDown, "Saw down"),
    (Wave::Square, "Square"),
    (Wave::Random, "Sample and hold"),
    (Wave::SmoothRandom, "Smooth random"),
];

/// What each LFO shape does, in [`LFO_SHAPES`]' order: a sentence for each cell (§7.3).
const LFO_DETAILS: [&str; 7] = [
    "A smooth wobble.",
    "Rises and falls in straight lines.",
    "Rises, then snaps back down.",
    "Falls, then snaps back up.",
    "Jumps between two values, like a trill.",
    "A new random value on every cycle, held until the next.",
    "Glides from one random value to the next.",
];

/// A stepped switch's cells, **labelled by the parameter itself**: each its option's own formatted
/// value, so a cell reads what the host's automation list reads.
fn switch_options(p: &ModalSynthParams, id: &'static str) -> Vec<String> {
    let param = binding_for(id, p).param;
    let last = param
        .steps()
        .unwrap_or_else(|| unreachable!("{id} is not a switch"));
    (0..=last)
        .map(|option| param.format(option as f32 / last as f32))
        .collect()
}

/// A knob, in a column never narrower than its widest reading — a synced Rate's divisions
/// included, so its column holds the reading whichever way its sync is.
fn knob(ui: &Ui, p: &ModalSynthParams, id: &'static str) -> Node<Leaf> {
    let bound = binding_for(id, p);
    let widest = if lfo_of_rate(id).is_some() {
        binding::synced_widest(bound.param, LFO_SYNC.span)
    } else {
        mxm_ui::control::widest_value(|n| bound.param.format(n as f32))
    };
    let column = if STANDARD.value_always_visible() {
        let font = mxm_ui::typography::value_style(ui.style()).resolve(ui.style());
        ui.painter()
            .layout_no_wrap(widest.clone(), font, egui::Color32::PLACEHOLDER)
            .size()
            .x
    } else {
        0.0
    };
    leaf(
        Leaf::Knob(id),
        Kind::Knob {
            name: bound.painted().to_owned(),
            widest,
            size: STANDARD,
            column,
        },
    )
}

/// The collection's knob row (`mxm_ui::tree::knob_row`).
fn knobs(ui: &Ui, p: &ModalSynthParams, ids: &[&'static str]) -> Node<Leaf> {
    tree::knob_row(
        ui,
        ids.iter().map(|id| (STANDARD, knob(ui, p, id))).collect(),
    )
}

/// An envelope's A, D, S and R as the collection's fader row (`mxm_ui::tree::fader_row`).
fn faders(ui: &Ui, p: &ModalSynthParams, ids: &[&'static str]) -> Node<Leaf> {
    tree::fader_row(
        ui,
        ids.iter()
            .map(|&id| {
                let bound = binding_for(id, p);
                tree::fader(
                    Leaf::Fader(id),
                    bound.painted(),
                    mxm_ui::control::widest_value(|n| bound.param.format(n as f32)),
                )
            })
            .collect(),
    )
}

fn switch(p: &ModalSynthParams, id: &'static str) -> Node<Leaf> {
    leaf(
        Leaf::Switch(id),
        Kind::Segmented {
            label: binding_for(id, p).painted().to_owned(),
            options: switch_options(p, id),
            beside: None,
        },
    )
}

fn toggle(p: &ModalSynthParams, id: &'static str) -> Node<Leaf> {
    leaf(
        Leaf::Toggle(id),
        Kind::Toggle {
            label: binding_for(id, p).painted().to_owned(),
        },
    )
}

/// A target's routes. A composite with a rule of its own — its floor is every source a row could
/// show — so it states its size (`route_rows::size`) and fills the card's width.
fn routes(ui: &Ui, p: &ModalSynthParams, target: Target) -> Node<Leaf> {
    let size = route_rows::size(ui, &p.routes, target);
    leaf(
        Leaf::Routes(route_rows::index_of(target)),
        Kind::Custom {
            min_width: size.x,
            height: Height::Fixed(size.y),
            fills: true,
        },
    )
}

/// Every route stack of `card`, `SPACE_3` below what precedes them.
fn stacks(ui: &Ui, p: &ModalSynthParams, card: Card) -> Vec<Node<Leaf>> {
    card.targets()
        .iter()
        .enumerate()
        .map(|(n, target)| {
            let stack = routes(ui, p, *target);
            if n == 0 { pad(SPACE_3, stack) } else { stack }
        })
        .collect()
}

/// A card's body, as a tree, from the parameters.
pub fn card(ui: &Ui, card: Card, p: &ModalSynthParams) -> Node<Leaf> {
    let gap = ui.spacing().item_spacing.x;
    let mut body = match card {
        Card::Voice => vec![switch(p, "mode"), knobs(ui, p, &["glide", "bend_range"])],
        // A, D, S and R as faders, the convention's (the owner, 2026-09-25: *"ADSRs are typically
        // sliders"*), then the curve, what drives it and Cycle.
        Card::Envelope(n) => {
            let ids = ENVELOPE_IDS[n];
            vec![
                faders(ui, p, &ids[..4]),
                knobs(ui, p, &[ids[4]]),
                switch(p, ids[6]),
                toggle(p, ids[5]),
            ]
        }
        // Rate, its quarter note straight after it and then Fade, one flat row (the conventions'
        // *Tempo sync*); then the shapes and Retrigger.
        Card::Lfo(n) => {
            let ids = LFO_IDS[n];
            vec![
                tree::row_gap(
                    gap,
                    vec![
                        knobs(ui, p, &[ids[0]]),
                        tree::switch_beside_knob(
                            STANDARD,
                            leaf(Leaf::Sync(ids[2]), Kind::SyncToggle),
                        ),
                        knobs(ui, p, &[ids[4]]),
                    ],
                ),
                leaf(
                    Leaf::Shape(ids[1]),
                    Kind::Waves {
                        label: Some(binding_for(ids[1], p).painted().to_owned()),
                        count: LFO_SHAPES.len(),
                        marks: Vec::new(),
                        beside: None,
                    },
                ),
                toggle(p, ids[3]),
            ]
        }
        Card::Model => vec![switch(p, "model"), knobs(ui, p, &["coarse", "fine"])],
        Card::Exciter => vec![knobs(ui, p, &["contour", "bow", "blow", "strike"])],
        Card::BowAndBlow => vec![knobs(ui, p, &["bow_timbre", "flow", "blow_timbre"])],
        Card::Strike => vec![knobs(ui, p, &["mallet", "strike_timbre"])],
        Card::Resonator => vec![knobs(
            ui,
            p,
            &["geometry", "brightness", "damping", "position", "space"],
        )],
    };
    body.extend(stacks(ui, p, card));
    stack(body)
}

/// Everything a leaf draws with: the parameters and their host, the tempo in force and the text
/// buffers.
pub struct Live<'a, 'b> {
    pub params: &'a ModalSynthParams,
    /// The host tempo in force: a synced Rate reads its division with one and its hertz without.
    pub tempo: Option<f64>,
    pub setter: &'a ParamSetter<'b>,
    pub entries: &'a mut HashMap<&'static str, Option<String>>,
}

/// Draws one leaf, in the `Ui` the tree bounded to `rect`, through the bindings — so the controls,
/// their gestures and their names are the collection's.
pub fn paint(ui: &mut Ui, tokens: &Tokens, leaf: &Leaf, rect: egui::Rect, live: &mut Live<'_, '_>) {
    let p = live.params;
    let setter = live.setter;
    match *leaf {
        Leaf::Knob(id) => {
            let bound = binding_for(id, p);
            match synced_reading(p, id, live.tempo) {
                Some(reading) => bound.knob_with_reading(
                    ui,
                    tokens,
                    setter,
                    STANDARD,
                    rect.width(),
                    live.entries,
                    reading,
                ),
                None => bound.knob(ui, tokens, setter, STANDARD, rect.width(), live.entries),
            }
        }
        Leaf::Fader(id) => {
            let bound = binding_for(id, p);
            bound.slider_vertical(
                ui,
                tokens,
                setter,
                live.entries,
                bound.painted(),
                rect.width(),
                mxm_ui::control::FADER_HEIGHT,
            );
        }
        // Each cell as wide as the longest option and no wider (design system §7.3).
        Leaf::Switch(id) => {
            let bound = binding_for(id, p);
            let labels = switch_options(p, id);
            let options: Vec<&str> = labels.iter().map(String::as_str).collect();
            binding::segmented_named(
                ui,
                tokens,
                bound.id,
                bound.param,
                bound.panel.as_deref(),
                &options,
                bound.details,
                setter,
                0.0,
            );
        }
        Leaf::Toggle(id) => {
            let bound = binding_for(id, p);
            binding::toggle_labelled(
                ui,
                tokens,
                bound.id,
                bound.param,
                bound.painted(),
                bound.description,
                setter,
                0.0,
            );
        }
        Leaf::Sync(id) => binding::sync_picture(ui, tokens, id, param_of(id, p), setter),
        Leaf::Shape(id) => {
            let bound = binding_for(id, p);
            binding::segmented_waves_named(
                ui,
                tokens,
                bound.id,
                bound.param,
                bound.panel.as_deref(),
                &LFO_SHAPES,
                None,
                bound.details,
                setter,
            );
        }
        Leaf::Routes(target) => route_rows::stack(
            ui,
            tokens,
            &p.routes,
            Target::ALL[target],
            setter,
            live.entries,
        ),
    }
}

/// The LFO whose Rate `id` is, from 0.
fn lfo_of_rate(id: &str) -> Option<usize> {
    LFO_IDS.iter().position(|ids| ids[0] == id)
}

/// A synced Rate's reading, the division it plays, while its sync is on and a tempo is in force.
fn synced_reading(p: &ModalSynthParams, id: &str, tempo: Option<f64>) -> Option<&'static str> {
    use nice_plug::prelude::Param as _;
    let lfo = p.lfos()[lfo_of_rate(id)?];
    if !lfo.sync.value() {
        return None;
    }
    LFO_SYNC
        .shown(
            lfo.rate.unmodulated_normalized_value(),
            tempo,
            f64::from(lfo.rate.preview_plain(0.0)),
            f64::from(lfo.rate.preview_plain(1.0)),
        )
        .map(mxm_tempo::Division::label)
}

// ---------------------------------------------------------------------------------------------
// The bindings: every parameter's control, sentence, painted name and step law.
// ---------------------------------------------------------------------------------------------

/// Where an id is among the parameters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Place {
    Voice,
    /// Envelope `n`'s `k`th, in `ENVELOPE_IDS`' order.
    Envelope(usize, usize),
    /// LFO `n`'s `k`th, in `LFO_IDS`' order.
    Lfo(usize, usize),
    /// Route slot `r`'s `k`th: source, target, amount.
    Route(usize, usize),
}

fn place(id: &str) -> Place {
    let find = |lists: &[&[&str]]| {
        lists
            .iter()
            .enumerate()
            .find_map(|(n, ids)| ids.iter().position(|i| *i == id).map(|k| (n, k)))
    };
    let envelopes: Vec<&[&str]> = ENVELOPE_IDS.iter().map(|ids| &ids[..]).collect();
    let lfos: Vec<&[&str]> = LFO_IDS.iter().map(|ids| &ids[..]).collect();
    let routes: Vec<&[&str]> = ROUTE_IDS.iter().map(|ids| &ids[..]).collect();
    if VOICE_IDS.contains(&id) {
        Place::Voice
    } else if let Some((n, k)) = find(&envelopes) {
        Place::Envelope(n, k)
    } else if let Some((n, k)) = find(&lfos) {
        Place::Lfo(n, k)
    } else if let Some((r, k)) = find(&routes) {
        Place::Route(r, k)
    } else {
        unreachable!("{id} is not a parameter")
    }
}

/// The parameter `id` names.
pub fn param_of<'a>(id: &str, p: &'a ModalSynthParams) -> &'a dyn ErasedParam {
    match place(id) {
        Place::Voice => match id {
            "model" => &p.model,
            "coarse" => &p.coarse,
            "fine" => &p.fine,
            "contour" => &p.contour,
            "bow" => &p.bow,
            "bow_timbre" => &p.bow_timbre,
            "blow" => &p.blow,
            "flow" => &p.flow,
            "blow_timbre" => &p.blow_timbre,
            "strike" => &p.strike,
            "mallet" => &p.mallet,
            "strike_timbre" => &p.strike_timbre,
            "geometry" => &p.geometry,
            "brightness" => &p.brightness,
            "damping" => &p.damping,
            "position" => &p.position,
            "space" => &p.space,
            "mode" => &p.mode,
            "glide" => &p.glide,
            "bend_range" => &p.bend_range,
            _ => &p.volume,
        },
        Place::Envelope(n, k) => {
            let e = p.envelopes()[n];
            let params: [&dyn ErasedParam; 7] = [
                &e.attack,
                &e.decay,
                &e.sustain,
                &e.release,
                &e.response,
                &e.cycle,
                &e.mode,
            ];
            params[k]
        }
        Place::Lfo(n, k) => {
            let l = p.lfos()[n];
            let params: [&dyn ErasedParam; 5] = [&l.rate, &l.shape, &l.sync, &l.retrigger, &l.fade];
            params[k]
        }
        Place::Route(r, k) => {
            let route = p.routes.all()[r];
            let params: [&dyn ErasedParam; 3] = [&route.source, &route.target, &route.amount];
            params[k]
        }
    }
}

/// What a control tells the player on hover: what it does to the sound (design system §7.6).
fn description_of(id: &str) -> &'static str {
    match place(id) {
        Place::Voice => match id {
            "model" => "Chooses what the exciters play into: a resonator, or the dark FM voice.",
            "coarse" => "Transposes the voice in semitones, up to three octaves either way.",
            "fine" => "Fine tuning, up to two semitones either way.",
            "contour" => {
                "The shape of the bow's and the blow's envelope: from a short pluck through a swell to a held note."
            }
            "bow" => "How hard the bow presses on the resonator.",
            "bow_timbre" => "The bow's grain, from smooth to scratchy.",
            "blow" => {
                "How much breath blows into the resonator; turned high, a reed tube joins in."
            }
            "flow" => "How the breath flows, from a steady stream to gusts and sputters.",
            "blow_timbre" => "The breath's colour, from dark to hissing.",
            "strike" => {
                "How hard the resonator is struck; turned high, the strike itself is heard too."
            }
            "mallet" => {
                "What strikes it: recorded hits, then a mallet, a plectrum and a shower of particles."
            }
            "strike_timbre" => "The strike's brightness, from a soft thud to a hard click.",
            "geometry" => {
                "The resonator's shape: from a stretched string through bars and plates to bells; for strings, the chord."
            }
            "brightness" => "How bright the resonator rings, from muffled to glassy.",
            "damping" => "How long it rings, from a short thump to a ring that barely fades.",
            "position" => "Where it is struck and heard, from the edge to the middle.",
            "space" => {
                "From the dry exciter, through a widening stereo image, into a reverb that grows at the top."
            }
            "mode" => "How the voices take notes.",
            "glide" => "Glide time between notes; zero turns glide off.",
            "bend_range" => "The pitch-bend range, in semitones.",
            _ => "The instrument's output level.",
        },
        Place::Envelope(_, k) => match k {
            0 => "Time to rise to full level.",
            1 => "Time to fall from full level to the sustain level.",
            2 => "The level held for as long as the note is held.",
            3 => "Time to fall back to rest once the note ends.",
            4 => {
                "The curve of every stage: logarithmic below the centre, straight at it, exponential above."
            }
            5 => {
                "Repeats the rise and fall while it is on, so the envelope runs as a wave of its own."
            }
            _ => "What starts the envelope.",
        },
        Place::Lfo(_, k) => match k {
            0 => "How fast this LFO runs; with Sync on, a note length.",
            1 => "This LFO's wave.",
            2 => binding::SYNC_DESCRIPTION,
            3 => {
                "Starts this LFO again with each note, for each voice apart; off, one LFO runs for every voice."
            }
            _ => "How long this LFO takes to fade in after each note; zero is at once.",
        },
        Place::Route(_, k) => match k {
            0 => "Where this route's movement comes from.",
            1 => "What this route moves.",
            _ => "How far this route moves its target. Below the centre it moves it the other way.",
        },
    }
}

/// What each option of a stepped control does, one sentence per cell in the parameter's own order
/// (design system §7.3). Empty for everything drawn as a knob, fader, slider or toggle.
fn details_of(id: &str) -> &'static [&'static str] {
    match place(id) {
        Place::Voice if id == "model" => &[
            "Modes of a bar, a plate or a bell, struck, bowed or blown.",
            "One string, plucked, bowed or blown.",
            "Five strings tuned to a chord, the chord chosen by Geometry.",
            "A dark pair of FM voices that the exciters' controls shape.",
        ],
        Place::Voice if id == "mode" => &[
            "Up to eight notes at once.",
            "One note at a time; each new note starts the sound again.",
            "One note at a time; a note played over a held one carries on without starting again.",
        ],
        Place::Envelope(_, 6) => &[
            "Each note starts it, and letting go of the key releases it.",
            "Its Trigger starts it; it holds while the trigger is high and releases when it falls.",
            "It follows its Input, rising at the attack time and falling at the release time.",
        ],
        Place::Lfo(_, 1) => &LFO_DETAILS,
        _ => &[],
    }
}

/// What a control paints where its card already says the rest (design system §7.1); `None` paints
/// the parameter's own name.
fn panel_of(id: &str) -> Option<&'static str> {
    match place(id) {
        Place::Voice => match id {
            "mode" => Some("Mode"),
            "strike_timbre" => Some("Timbre"),
            _ => None,
        },
        Place::Envelope(_, k) => Some(["A", "D", "S", "R", "Response", "Cycle", "Mode"][k]),
        Place::Lfo(_, k) => Some(["Rate", "Shape", "Sync", "Retrigger", "Fade"][k]),
        Place::Route(..) => None,
    }
}

/// How the keyboard steps it (the owner's scope, 2026-09-23): the semitone-stepped Coarse and
/// Bend range by `Semitones`; everything else its own step.
fn law_of(id: &str) -> StepLaw {
    match id {
        "coarse" | "bend_range" => StepLaw::Semitones,
        _ => StepLaw::Own,
    }
}

/// Whether its rest is the centre: a signed amount (a Response, a route's depth) or a tune.
fn bipolar(id: &str) -> bool {
    matches!(id, "coarse" | "fine")
        || matches!(place(id), Place::Envelope(_, 4) | Place::Route(_, 2))
}

/// **Every parameter's binding**: its control, its sentence, what it paints, its step law.
pub fn binding_for<'a>(id: &'static str, p: &'a ModalSynthParams) -> Bound<'a> {
    let mut bound = Bound::new(id, param_of(id, p), description_of(id))
        .details(details_of(id))
        .law(law_of(id));
    if let Some(panel) = panel_of(id) {
        bound = bound.labelled(panel);
    }
    if bipolar(id) { bound.bipolar() } else { bound }
}

/// Every parameter's binding, in declaration order: the developer Parameters list.
pub fn all_bindings(p: &ModalSynthParams) -> Vec<Bound<'_>> {
    p.all_parameters()
        .into_iter()
        .map(|(id, _)| binding_for(id, p))
        .collect()
}
