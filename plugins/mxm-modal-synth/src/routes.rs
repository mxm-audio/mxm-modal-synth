//! Sixteen route slots, each a source, a target and an amount (`route<r>_{source,target,amount}`):
//! mxm-drum-machine's scheme (the owner, 2026-09-30: the host holds what the panel can show at
//! once), in place of a presence and an amount for every one of the DSP's 19 sources × 40 targets.
//!
//! **The lists are the DSP's.** A source is `Off` or one of `mxm_modal_synth_dsp::Source::ALL`, a
//! target `Off` or one of `Target::ALL`, in their order: [`SourceChoice`] and [`TargetChoice`]
//! carry the DSP's value itself, and their names come from an exhaustive match, so a source or
//! target added to the DSP fails to compile here until it is named. The order is what a saved state
//! and a preset store (a parameter's index); it is fixed at the gate (E5).
//!
//! **A slot is in use when neither its source nor its target is `Off`**; switching its source off
//! keeps its target and amount, and two slots on one source and target add (the DSP's rules). The
//! amount is a signed fraction of its target's reach, one parameter for every target, so the host
//! reads it as a percentage: the drums' **recorded deviation** from `routing_checks`' pair-by-pair
//! readings (`plugins/mxm-modal-synth/AGENTS.md`).

use mxm_modal_synth_dsp::{SLOTS, Slot, Source, Target};
use mxm_params::text;
use nice_plug::prelude::*;

/// A route's amount: −1…+1 of its target's reach, smoothed as the drums' are.
pub const AMOUNT_SMOOTHING_MS: f32 = 15.0;

/// The name a card shows for a source: short, because the longest `<target> from <source>` sets a
/// card's floor.
const fn source_name(source: Source) -> &'static str {
    match source {
        Source::Envelope1 => "Env 1",
        Source::Envelope2 => "Env 2",
        Source::Envelope3 => "Env 3",
        Source::Envelope1Rise => "Env 1 rise end",
        Source::Envelope1Cycle => "Env 1 cycle end",
        Source::Envelope2Rise => "Env 2 rise end",
        Source::Envelope2Cycle => "Env 2 cycle end",
        Source::Envelope3Rise => "Env 3 rise end",
        Source::Envelope3Cycle => "Env 3 cycle end",
        Source::Lfo1 => "LFO 1",
        Source::Lfo2 => "LFO 2",
        Source::Lfo3 => "LFO 3",
        Source::Contour => "Contour",
        Source::Key => "Key",
        Source::Velocity => "Velocity",
        Source::Wheel => "Wheel",
        Source::Pressure => "Pressure",
        Source::Bend => "Bend",
        Source::Random => "Random",
    }
}

/// A source's stable id, which a saved state stores.
const fn source_id(source: Source) -> &'static str {
    match source {
        Source::Envelope1 => "env1",
        Source::Envelope2 => "env2",
        Source::Envelope3 => "env3",
        Source::Envelope1Rise => "env1_rise",
        Source::Envelope1Cycle => "env1_cycle",
        Source::Envelope2Rise => "env2_rise",
        Source::Envelope2Cycle => "env2_cycle",
        Source::Envelope3Rise => "env3_rise",
        Source::Envelope3Cycle => "env3_cycle",
        Source::Lfo1 => "lfo1",
        Source::Lfo2 => "lfo2",
        Source::Lfo3 => "lfo3",
        Source::Contour => "contour",
        Source::Key => "key",
        Source::Velocity => "velocity",
        Source::Wheel => "wheel",
        Source::Pressure => "pressure",
        Source::Bend => "bend",
        Source::Random => "random",
    }
}

/// What a card and the host call a target: the DSP's names, in sentence case.
const fn target_name(target: Target) -> &'static str {
    match target {
        Target::Pitch => "Pitch",
        Target::Strength => "Strength",
        Target::Contour => "Contour",
        Target::Bow => "Bow",
        Target::BowTimbre => "Bow timbre",
        Target::Blow => "Blow",
        Target::Flow => "Flow",
        Target::BlowTimbre => "Blow timbre",
        Target::Strike => "Strike",
        Target::Mallet => "Mallet",
        Target::StrikeTimbre => "Strike timbre",
        Target::Geometry => "Geometry",
        Target::Brightness => "Brightness",
        Target::Damping => "Damping",
        Target::Position => "Position",
        Target::Space => "Space",
        Target::Model => "Model",
        Target::Amplitude => "Amplitude",
        Target::Pan => "Pan",
        Target::Lfo1Rate => "LFO 1 rate",
        Target::Lfo2Rate => "LFO 2 rate",
        Target::Lfo3Rate => "LFO 3 rate",
        Target::Envelope1Rise => "Env 1 rise",
        Target::Envelope1Fall => "Env 1 fall",
        Target::Envelope1Time => "Env 1 time",
        Target::Envelope1Cycle => "Env 1 cycle",
        Target::Envelope1Trigger => "Env 1 trigger",
        Target::Envelope1Input => "Env 1 input",
        Target::Envelope2Rise => "Env 2 rise",
        Target::Envelope2Fall => "Env 2 fall",
        Target::Envelope2Time => "Env 2 time",
        Target::Envelope2Cycle => "Env 2 cycle",
        Target::Envelope2Trigger => "Env 2 trigger",
        Target::Envelope2Input => "Env 2 input",
        Target::Envelope3Rise => "Env 3 rise",
        Target::Envelope3Fall => "Env 3 fall",
        Target::Envelope3Time => "Env 3 time",
        Target::Envelope3Cycle => "Env 3 cycle",
        Target::Envelope3Trigger => "Env 3 trigger",
        Target::Envelope3Input => "Env 3 input",
    }
}

/// A target's stable id, which a saved state stores.
pub(crate) const fn target_id(target: Target) -> &'static str {
    match target {
        Target::Pitch => "pitch",
        Target::Strength => "strength",
        Target::Contour => "contour",
        Target::Bow => "bow",
        Target::BowTimbre => "bow_timbre",
        Target::Blow => "blow",
        Target::Flow => "flow",
        Target::BlowTimbre => "blow_timbre",
        Target::Strike => "strike",
        Target::Mallet => "mallet",
        Target::StrikeTimbre => "strike_timbre",
        Target::Geometry => "geometry",
        Target::Brightness => "brightness",
        Target::Damping => "damping",
        Target::Position => "position",
        Target::Space => "space",
        Target::Model => "model",
        Target::Amplitude => "amplitude",
        Target::Pan => "pan",
        Target::Lfo1Rate => "lfo1_rate",
        Target::Lfo2Rate => "lfo2_rate",
        Target::Lfo3Rate => "lfo3_rate",
        Target::Envelope1Rise => "env1_rise",
        Target::Envelope1Fall => "env1_fall",
        Target::Envelope1Time => "env1_time",
        Target::Envelope1Cycle => "env1_cycle",
        Target::Envelope1Trigger => "env1_trigger",
        Target::Envelope1Input => "env1_input",
        Target::Envelope2Rise => "env2_rise",
        Target::Envelope2Fall => "env2_fall",
        Target::Envelope2Time => "env2_time",
        Target::Envelope2Cycle => "env2_cycle",
        Target::Envelope2Trigger => "env2_trigger",
        Target::Envelope2Input => "env2_input",
        Target::Envelope3Rise => "env3_rise",
        Target::Envelope3Fall => "env3_fall",
        Target::Envelope3Time => "env3_time",
        Target::Envelope3Cycle => "env3_cycle",
        Target::Envelope3Trigger => "env3_trigger",
        Target::Envelope3Input => "env3_input",
    }
}

/// Values a source parameter holds: `Off`, then every source.
pub const SOURCE_COUNT: usize = Source::ALL.len() + 1;
/// Values a target parameter holds: `Off`, then every target.
pub const TARGET_COUNT: usize = Target::ALL.len() + 1;

/// `Off`, then `each` of `all`, in order.
macro_rules! listed {
    ($count:expr, $all:expr, $each:ident, $off:literal) => {{
        let mut out = [$off; $count];
        let mut i = 0;
        while i < $all.len() {
            out[i + 1] = $each($all[i]);
            i += 1;
        }
        out
    }};
}

static SOURCE_NAMES: [&str; SOURCE_COUNT] = listed!(SOURCE_COUNT, Source::ALL, source_name, "Off");
static SOURCE_IDS: [&str; SOURCE_COUNT] = listed!(SOURCE_COUNT, Source::ALL, source_id, "off");
static TARGET_NAMES: [&str; TARGET_COUNT] = listed!(TARGET_COUNT, Target::ALL, target_name, "Off");
static TARGET_IDS: [&str; TARGET_COUNT] = listed!(TARGET_COUNT, Target::ALL, target_id, "off");

/// A route's source: `None` is `Off`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SourceChoice(pub Option<Source>);

impl Enum for SourceChoice {
    fn variants() -> &'static [&'static str] {
        &SOURCE_NAMES
    }

    fn ids() -> Option<&'static [&'static str]> {
        Some(&SOURCE_IDS)
    }

    fn to_index(self) -> usize {
        self.0
            .and_then(|source| Source::ALL.iter().position(|&s| s == source))
            .map_or(0, |i| i + 1)
    }

    fn from_index(index: usize) -> Self {
        Self(
            index
                .checked_sub(1)
                .and_then(|i| Source::ALL.get(i).copied()),
        )
    }
}

impl SourceChoice {
    /// What a card calls it.
    #[must_use]
    pub fn name(self) -> &'static str {
        SOURCE_NAMES[self.to_index()]
    }
}

/// A route's target: `None` is `Off`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TargetChoice(pub Option<Target>);

impl Enum for TargetChoice {
    fn variants() -> &'static [&'static str] {
        &TARGET_NAMES
    }

    fn ids() -> Option<&'static [&'static str]> {
        Some(&TARGET_IDS)
    }

    fn to_index(self) -> usize {
        self.0
            .and_then(|target| Target::ALL.iter().position(|&t| t == target))
            .map_or(0, |i| i + 1)
    }

    fn from_index(index: usize) -> Self {
        Self(
            index
                .checked_sub(1)
                .and_then(|i| Target::ALL.get(i).copied()),
        )
    }
}

impl TargetChoice {
    /// What a card and the host call it.
    #[must_use]
    pub fn name(self) -> &'static str {
        TARGET_NAMES[self.to_index()]
    }
}

/// One route slot: a source, a target and an amount.
#[derive(Params)]
pub struct RouteParams {
    #[id = "source"]
    pub source: EnumParam<SourceChoice>,
    #[id = "target"]
    pub target: EnumParam<TargetChoice>,
    #[id = "amount"]
    pub amount: FloatParam,
}

impl RouteParams {
    fn new(r: usize) -> Self {
        Self {
            source: EnumParam::new(format!("Route {r} source"), SourceChoice(None)),
            target: EnumParam::new(format!("Route {r} target"), TargetChoice(None)),
            amount: FloatParam::new(
                format!("Route {r} amount"),
                0.0,
                FloatRange::Linear {
                    min: -1.0,
                    max: 1.0,
                },
            )
            .with_smoother(SmoothingStyle::Linear(AMOUNT_SMOOTHING_MS))
            .with_value_to_string(text::percent())
            .with_string_to_value(text::parse_percent()),
        }
    }

    /// Whether the slot is in use: a source and a target, neither `Off`. One that is not
    /// contributes nothing and keeps its amount.
    #[must_use]
    pub fn in_use(&self) -> bool {
        self.source.value().0.is_some() && self.target.value().0.is_some()
    }

    /// The slot as the DSP takes it, its amount as the patch sets it.
    #[must_use]
    pub fn slot(&self) -> Slot {
        Slot {
            source: self.source.value().0,
            target: self.target.value().0,
            amount: self.amount.value(),
        }
    }
}

/// The ids of slot `r`'s three parameters, `r` from 1, in declaration order.
macro_rules! route_ids {
    ($($r:literal),* $(,)?) => {
        [$([
            concat!("route", $r, "_source"),
            concat!("route", $r, "_target"),
            concat!("route", $r, "_amount"),
        ]),*]
    };
}

/// Every slot's parameter ids, `[source, target, amount]`: `#[nested(id_prefix = "route<r>")]`
/// below, written out because the preset system takes `&'static str`.
pub const ROUTE_IDS: [[&str; 3]; SLOTS] =
    route_ids!(1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16);

/// The sixteen slots, nested so their ids are `route<r>_<field>`.
#[derive(Params)]
pub struct Routes {
    #[nested(id_prefix = "route1", group = "Route 1")]
    pub route1: RouteParams,
    #[nested(id_prefix = "route2", group = "Route 2")]
    pub route2: RouteParams,
    #[nested(id_prefix = "route3", group = "Route 3")]
    pub route3: RouteParams,
    #[nested(id_prefix = "route4", group = "Route 4")]
    pub route4: RouteParams,
    #[nested(id_prefix = "route5", group = "Route 5")]
    pub route5: RouteParams,
    #[nested(id_prefix = "route6", group = "Route 6")]
    pub route6: RouteParams,
    #[nested(id_prefix = "route7", group = "Route 7")]
    pub route7: RouteParams,
    #[nested(id_prefix = "route8", group = "Route 8")]
    pub route8: RouteParams,
    #[nested(id_prefix = "route9", group = "Route 9")]
    pub route9: RouteParams,
    #[nested(id_prefix = "route10", group = "Route 10")]
    pub route10: RouteParams,
    #[nested(id_prefix = "route11", group = "Route 11")]
    pub route11: RouteParams,
    #[nested(id_prefix = "route12", group = "Route 12")]
    pub route12: RouteParams,
    #[nested(id_prefix = "route13", group = "Route 13")]
    pub route13: RouteParams,
    #[nested(id_prefix = "route14", group = "Route 14")]
    pub route14: RouteParams,
    #[nested(id_prefix = "route15", group = "Route 15")]
    pub route15: RouteParams,
    #[nested(id_prefix = "route16", group = "Route 16")]
    pub route16: RouteParams,
}

impl Default for Routes {
    fn default() -> Self {
        Self {
            route1: RouteParams::new(1),
            route2: RouteParams::new(2),
            route3: RouteParams::new(3),
            route4: RouteParams::new(4),
            route5: RouteParams::new(5),
            route6: RouteParams::new(6),
            route7: RouteParams::new(7),
            route8: RouteParams::new(8),
            route9: RouteParams::new(9),
            route10: RouteParams::new(10),
            route11: RouteParams::new(11),
            route12: RouteParams::new(12),
            route13: RouteParams::new(13),
            route14: RouteParams::new(14),
            route15: RouteParams::new(15),
            route16: RouteParams::new(16),
        }
    }
}

impl Routes {
    /// The sixteen in order.
    #[must_use]
    pub fn all(&self) -> [&RouteParams; SLOTS] {
        [
            &self.route1,
            &self.route2,
            &self.route3,
            &self.route4,
            &self.route5,
            &self.route6,
            &self.route7,
            &self.route8,
            &self.route9,
            &self.route10,
            &self.route11,
            &self.route12,
            &self.route13,
            &self.route14,
            &self.route15,
            &self.route16,
        ]
    }

    /// The first slot not in use, the one a card's *modulate* takes; `None` when all sixteen are.
    #[must_use]
    pub fn first_free(&self) -> Option<usize> {
        self.all().iter().position(|route| !route.in_use())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **Every DSP source and target has exactly one spelling, in the DSP's order, after `Off`**,
    /// and each index reads back as itself.
    #[test]
    fn the_lists_are_off_then_the_dsps_in_order() {
        assert_eq!(SourceChoice::from_index(0), SourceChoice(None));
        assert_eq!(TargetChoice::from_index(0), TargetChoice(None));
        for (i, source) in Source::ALL.into_iter().enumerate() {
            assert_eq!(SourceChoice::from_index(i + 1), SourceChoice(Some(source)));
            assert_eq!(SourceChoice(Some(source)).to_index(), i + 1);
        }
        for (i, target) in Target::ALL.into_iter().enumerate() {
            assert_eq!(TargetChoice::from_index(i + 1), TargetChoice(Some(target)));
            assert_eq!(TargetChoice(Some(target)).to_index(), i + 1);
        }
        // Out of range is Off, as nice-plug's `Enum` asks.
        assert_eq!(SourceChoice::from_index(SOURCE_COUNT), SourceChoice(None));
        assert_eq!(TargetChoice::from_index(TARGET_COUNT), TargetChoice(None));
        assert_eq!(SourceChoice::variants().len(), 20);
        assert_eq!(TargetChoice::variants().len(), 41);
    }

    /// Names and ids are unique, so a typed name and a saved id each find one value.
    #[test]
    fn every_name_and_id_is_unique() {
        for list in [&SOURCE_NAMES[..], &SOURCE_IDS, &TARGET_NAMES, &TARGET_IDS] {
            let mut sorted = list.to_vec();
            sorted.sort_unstable();
            sorted.dedup();
            assert_eq!(sorted.len(), list.len(), "{list:?}");
        }
    }

    #[test]
    fn the_route_ids_are_the_nested_ids() {
        let routes = Routes::default();
        let ids: Vec<String> = routes.param_map().into_iter().map(|(id, ..)| id).collect();
        let expected: Vec<&str> = ROUTE_IDS.iter().flatten().copied().collect();
        assert_eq!(ids, expected);
    }

    /// A slot is in use only with both ends set; switching either off keeps the amount.
    #[test]
    fn a_slot_is_in_use_only_with_a_source_and_a_target() {
        use nice_plug::params::InternalParamMut;
        let routes = Routes::default();
        let route = &routes.route3;
        assert!(!route.in_use());
        assert_eq!(routes.first_free(), Some(0));
        // SAFETY: exclusive test ownership; no process or GUI thread exists.
        unsafe {
            route
                .source
                ._internal_set_plain_value(SourceChoice(Some(Source::Lfo2)));
            route.amount._internal_set_plain_value(0.25);
        }
        assert!(!route.in_use(), "a source alone is not a route");
        unsafe {
            route
                .target
                ._internal_set_plain_value(TargetChoice(Some(Target::Brightness)));
        }
        assert!(route.in_use());
        assert_eq!(
            route.slot(),
            Slot {
                source: Some(Source::Lfo2),
                target: Some(Target::Brightness),
                amount: 0.25
            }
        );
        unsafe {
            route.source._internal_set_plain_value(SourceChoice(None));
        }
        assert!(!route.in_use());
        assert_eq!(route.amount.value(), 0.25, "the amount stays");
        assert_eq!(
            route.slot().target,
            Some(Target::Brightness),
            "so does the target"
        );
    }
}
