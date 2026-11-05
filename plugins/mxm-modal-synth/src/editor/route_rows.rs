//! **The route slots, drawn as rows on the card of the target they move** (the plan's §5.5, named
//! in the root `AGENTS.md`), as mxm-drum-machine draws its slots under the knob each moves: a
//! target's line offers `‹ modulate ›`, and every slot in use aimed at it is a row under
//! that line — its amount, painted as its source, and a remove that switches the source off.
//!
//! **The rows come from the parameters alone**, so a preset decides the panel and there is no
//! editor-only routing state. A slot is in use when neither its source nor its target is `Off`
//! (`RouteParams::in_use`); a slot that is not appears on no card until it is given a source on one.
//! The collection's route stack (`mxm_modulation_params::ui`) draws one fixed pair per source; a
//! slot here can take any source and any target, so this draws the same rows over the slots'
//! parameters, as the drum machine's `route_stack` does.

use std::collections::HashMap;

use egui::Ui;
use mxm_modal_synth_dsp::{SLOTS, Source, Target};
use mxm_ui::control::{self, ParamView, Steps, Wheel};
use mxm_ui::space::SPACE_2;
use mxm_ui::theme::Tokens;
use nice_plug::prelude::*;

use super::binding::{self, Bound, ErasedParam};
use crate::routes::{ROUTE_IDS, RouteParams, Routes, SourceChoice, TargetChoice};

/// What a target's line offers while a slot is free.
pub const MODULATE: &str = "modulate";
/// What it says instead once all sixteen slots are in use: no more can be added anywhere.
pub const FULL: &str = "all 16 in use";

/// What a target's line says on hover once every slot is in use.
const FULL_DESCRIPTION: &str = "All sixteen routes are in use. Remove one to add another here.";

/// A target's index in the DSP's list, which keys its routes' leaf.
#[must_use]
pub fn index_of(target: Target) -> usize {
    Target::ALL
        .iter()
        .position(|&t| t == target)
        .expect("every target is in the DSP's list")
}

/// What a target's line paints on the card it sits on: the card already names the envelope, the
/// LFO or the exciter, so its own words are dropped (design system §7.1). The canonical name, which the host, the
/// accessibility tree and each row's name read, stays `TargetChoice`'s.
#[must_use]
pub const fn panel_name(target: Target) -> &'static str {
    match target {
        Target::Pitch => "Pitch",
        Target::Strength => "Strength",
        Target::Contour => "Contour",
        Target::Bow => "Bow",
        Target::Blow => "Blow",
        Target::Strike => "Strike",
        Target::BowTimbre => "Bow timbre",
        Target::BlowTimbre => "Blow timbre",
        Target::StrikeTimbre => "Timbre",
        Target::Flow => "Flow",
        Target::Mallet => "Mallet",
        Target::Geometry => "Geometry",
        Target::Brightness => "Brightness",
        Target::Damping => "Damping",
        Target::Position => "Position",
        Target::Space => "Space",
        Target::Model => "Model",
        Target::Amplitude => "Amplitude",
        Target::Pan => "Pan",
        Target::Lfo1Rate | Target::Lfo2Rate | Target::Lfo3Rate => "Rate",
        Target::Envelope1Rise | Target::Envelope2Rise | Target::Envelope3Rise => "Rise",
        Target::Envelope1Fall | Target::Envelope2Fall | Target::Envelope3Fall => "Fall",
        Target::Envelope1Time | Target::Envelope2Time | Target::Envelope3Time => "Time",
        Target::Envelope1Cycle | Target::Envelope2Cycle | Target::Envelope3Cycle => "Cycle",
        Target::Envelope1Trigger | Target::Envelope2Trigger | Target::Envelope3Trigger => "Trigger",
        Target::Envelope1Input | Target::Envelope2Input | Target::Envelope3Input => "Input",
    }
}

/// The canonical name: what the host calls the target.
fn canonical(target: Target) -> &'static str {
    TargetChoice(Some(target)).name()
}

/// The slots, 0-based, in use and aimed at `target`, in slot order.
#[must_use]
pub fn routes_on(routes: &Routes, target: Target) -> Vec<usize> {
    routes
        .all()
        .iter()
        .enumerate()
        .filter(|(_, route)| route.in_use() && route.target.value().0 == Some(target))
        .map(|(r, _)| r)
        .collect()
}

/// The slot adding a source to `target` takes: a free one already aimed at it first — a source
/// removed from this target, so adding one back keeps its depth, as the drum machine's do — and
/// otherwise the first free one (`Routes::first_free`). `None` when all sixteen are in use.
#[must_use]
pub fn free_slot(routes: &Routes, target: Target) -> Option<usize> {
    routes
        .all()
        .iter()
        .position(|route| !route.in_use() && route.target.value().0 == Some(target))
        .or_else(|| routes.first_free())
}

/// The sources `‹ modulate ›` offers on `target`: every one not already moving it, while a slot
/// is free; none once all sixteen are in use.
#[must_use]
pub fn offered(routes: &Routes, target: Target) -> Vec<Source> {
    if free_slot(routes, target).is_none() {
        return Vec::new();
    }
    let all = routes.all();
    let on: Vec<Option<Source>> = routes_on(routes, target)
        .into_iter()
        .map(|r| all[r].source.value().0)
        .collect();
    Source::ALL
        .into_iter()
        .filter(|source| !on.contains(&Some(*source)))
        .collect()
}

/// **Adds `source` to `target` in one gesture**: the free slot's source and target, and its amount
/// to zero unless the slot was already aimed there, every write opened before the first value and
/// closed after the last (`binding::set_together`).
pub fn add(routes: &Routes, target: Target, source: Source, setter: &ParamSetter<'_>) {
    let Some(r) = free_slot(routes, target) else {
        return;
    };
    let route = routes.all()[r];
    let mut edits: Vec<(&dyn ErasedParam, f32)> = vec![
        (
            &route.source,
            route.source.preview_normalized(SourceChoice(Some(source))),
        ),
        (
            &route.target,
            route.target.preview_normalized(TargetChoice(Some(target))),
        ),
    ];
    if route.target.value().0 != Some(target) {
        edits.push((&route.amount, route.amount.preview_normalized(0.0)));
    }
    binding::set_together(setter, &edits);
}

/// **Removing a route is one write**: its source off. Its target and amount stay, so adding a
/// source back on the same card finds the depth it had.
pub fn remove(route: &RouteParams, setter: &ParamSetter<'_>) {
    binding::set_together(
        setter,
        &[(
            &route.source as &dyn ErasedParam,
            route.source.preview_normalized(SourceChoice(None)),
        )],
    );
}

/// One row's size, without drawing it: the amount's slider painted as `source`, at least
/// `TRACK_MIN` wide, and the remove's square beside it.
fn row_size(ui: &Ui, source: SourceChoice, widest: &str) -> egui::Vec2 {
    let spacing = ui.spacing().item_spacing;
    let slider = control::slider_size(ui, source.name(), true, widest);
    egui::Vec2::new(
        slider.x.max(mxm_modulation_params::ui::TRACK_MIN) + spacing.x + control::REMOVE_SIZE,
        slider.y.max(control::REMOVE_SIZE),
    )
}

/// **What a target's routes occupy, without drawing them**: at their narrowest the widest row any
/// source could draw, or the target's line whichever thing it says; their height as the patch
/// stands — the line alone, or the group of the line and a row for each slot in use. The
/// collection's own stack's rule (`mxm_modulation_params::ui::stack_size`), over the slots.
#[must_use]
pub fn size(ui: &Ui, routes: &Routes, target: Target) -> egui::Vec2 {
    let inset = 2.0 * mxm_ui::tree::GROUP_INSET;
    let amount: &dyn ErasedParam = &routes.route1.amount;
    let widest = control::widest_value(|n| amount.format(n as f32));
    let line = control::selector_size(ui, panel_name(target), &[MODULATE, FULL]);
    let widest_row = Source::ALL
        .iter()
        .map(|source| row_size(ui, SourceChoice(Some(*source)), &widest).x)
        .fold(0.0, f32::max);
    let width = (widest_row.max(line.x) + inset).max(line.x);
    let all = routes.all();
    let rows: Vec<f32> = routes_on(routes, target)
        .into_iter()
        .map(|r| row_size(ui, all[r].source.value(), &widest).y)
        .collect();
    let height = if rows.is_empty() {
        line.y
    } else {
        inset + line.y + SPACE_2 + rows.iter().sum::<f32>() + SPACE_2 * (rows.len() - 1) as f32
    };
    egui::Vec2::new(width, height)
}

/// One target's routes: its line, and with a slot in use a group of the line and its rows.
pub fn stack(
    ui: &mut Ui,
    tokens: &Tokens,
    routes: &Routes,
    target: Target,
    setter: &ParamSetter<'_>,
    entries: &mut HashMap<&'static str, Option<String>>,
) {
    let on = routes_on(routes, target);
    let offered = offered(routes, target);
    if on.is_empty() {
        line(ui, tokens, routes, target, &offered, setter);
    } else {
        mxm_ui::shell::group(ui, tokens, |ui| {
            line(ui, tokens, routes, target, &offered, setter);
            for r in on {
                row(ui, tokens, routes.all()[r], r, target, setter, entries);
            }
        });
    }
}

/// The target's line: its name and `‹ modulate ›`, offering the sources it can still take; **once
/// all sixteen slots are in use it says so**, `‹ all 16 in use ›`, and offers nothing.
fn line(
    ui: &mut Ui,
    tokens: &Tokens,
    routes: &Routes,
    target: Target,
    offered: &[Source],
    setter: &ParamSetter<'_>,
) {
    let panel = panel_name(target);
    let name = canonical(target);
    if offered.is_empty() {
        ui.add_enabled_ui(false, |ui| {
            let mut chosen = 0usize;
            control::selector_named(
                ui,
                tokens,
                panel,
                name,
                &[FULL],
                &mut chosen,
                None,
                None,
                FULL_DESCRIPTION,
            );
        });
        return;
    }
    let labels: Vec<&str> = std::iter::once(MODULATE)
        .chain(
            offered
                .iter()
                .map(|source| SourceChoice(Some(*source)).name()),
        )
        .collect();
    let mut chosen = 0usize;
    let description = format!("Adds a modulation source to {name}.");
    let changed = control::selector_named(
        ui,
        tokens,
        panel,
        name,
        &labels,
        &mut chosen,
        None,
        Some(0),
        &description,
    );
    if changed && chosen > 0 {
        add(routes, target, offered[chosen - 1], setter);
    }
}

/// One slot in use: its amount, painted as its source and named in full (*Timbre from LFO 1*), with
/// the remove beside it, which switches the source off — one write, the depth kept. The amount is
/// the collection's slider: a drag, the keyboard's steps, OPEN to type a value and BACK (`Escape`)
/// to put back where a drag began, all the shared control's.
fn row(
    ui: &mut Ui,
    tokens: &Tokens,
    route: &RouteParams,
    r: usize,
    target: Target,
    setter: &ParamSetter<'_>,
    entries: &mut HashMap<&'static str, Option<String>>,
) {
    let [source_id, _, amount_id] = ROUTE_IDS[r];
    let source = route.source.value().name();
    let target_name = canonical(target);
    let amount: &dyn ErasedParam = &route.amount;
    let name = format!("{target_name} from {source}");
    let text = amount.text();
    let description = format!(
        "How much {source} moves {target_name}. Below the centre it moves it the other way."
    );
    let stepping = Bound::new(amount_id, amount, "").bipolar();
    let (fine_up, fine_down, coarse_up, coarse_down) = amount.stepping();
    let view = ParamView {
        name: &name,
        label: source,
        text: &text,
        widest: "",
        description: &description,
        default: f64::from(binding::reset_target(amount)),
        bipolar: true,
        read_only: false,
        modulation: f64::from(amount.modulation()),
        marked: amount.modulation() != 0.0,
        steps: Steps {
            fine_up,
            fine_down,
            coarse_up,
            coarse_down,
        },
        next: Some(control::Next(&stepping)),
        quiet_label: true,
    };
    let mut normalised = f64::from(amount.normalised());
    // The amount's text entry, as the binding's: OPEN opens it with the value as the plugin writes
    // it, `Escape` closes it, and what is typed is parsed by the parameter on commit.
    let entry = entries.entry(amount_id).or_default();
    if entry.is_none() && mxm_ui::navigation::take_type(ui, amount_id) {
        *entry = Some(amount.text());
    }
    if entry.is_some()
        && ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
    {
        *entry = None;
    }
    let mut removed = false;
    let outcome = ui
        .horizontal(|ui| {
            // The remove's width is reserved before the slider takes the rest.
            let gap = ui.spacing().item_spacing.x;
            let track = (ui.available_width() - control::REMOVE_SIZE - gap)
                .max(mxm_modulation_params::ui::TRACK_MIN);
            let out = ui
                .scope(|ui| {
                    ui.set_max_width(track);
                    mxm_ui::navigation::at(ui, amount_id, |ui| {
                        control::slider(
                            ui,
                            tokens,
                            &view,
                            &mut normalised,
                            track,
                            entry,
                            Wheel::Off,
                        )
                    })
                })
                .inner;
            // Bottom-aligned onto the slider's track, as the collection's route rows are.
            let height = ui.min_rect().height().max(control::REMOVE_SIZE);
            ui.allocate_ui_with_layout(
                egui::Vec2::new(control::REMOVE_SIZE, height),
                egui::Layout::bottom_up(egui::Align::Center),
                |ui| {
                    mxm_ui::navigation::at(ui, source_id, |ui| {
                        if control::remove_mark(
                            ui,
                            tokens,
                            control::REMOVE_SIZE,
                            true,
                            &format!("Remove {source} from {target_name}"),
                            route.source.modulation() != 0.0,
                            "The depth stays, so adding the source back here restores it.",
                        ) {
                            remove(route, setter);
                            removed = true;
                        }
                    });
                },
            );
            out
        })
        .inner;
    if removed {
        return;
    }
    // A committed text entry arrives as an instantaneous outcome with the buffer still open: the
    // parameter parses it, and an unparseable entry changes nothing.
    if outcome.changed && entry.is_some() {
        let parsed = entry.as_deref().and_then(|text| amount.parse(text));
        *entry = None;
        if let Some(value) = parsed {
            amount.begin(setter);
            amount.set(setter, value);
            amount.end(setter);
        }
        return;
    }
    if outcome.gesture_started {
        amount.begin(setter);
    }
    if outcome.changed {
        amount.set(setter, normalised as f32);
    }
    if outcome.gesture_ended {
        amount.end(setter);
    }
}

const _: () = assert!(SLOTS == 16, "FULL and FULL_DESCRIPTION name sixteen slots");
