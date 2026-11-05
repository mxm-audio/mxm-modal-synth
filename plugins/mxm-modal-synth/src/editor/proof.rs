//! The editor's proof, as mxm-mono-pr1's and mxm-drum-machine's: every card's layout tree in every
//! state that changes it, the paging and the opening size, the keyboard cursor over every control
//! and its keys (F1, BACK), the route rows and their gestures, the developer channel, both themes,
//! and the review pictures. Headless: it proves behaviour and geometry, not native feel or a DAW.

use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
};

use egui::{Rect, ThemePreference, vec2};
use kittest::{By, Queryable};
use mxm_modal_synth_dsp::{Source, Target};
use mxm_plugin_test::keyboard_checks::{self, Job, OUT, VALUE, key_of};
use mxm_plugin_test::{opening_size, paging_checks, tree_checks};
use nice_plug::{
    params::{InternalParamMut, internals::ParamPtr},
    prelude::{ParamSetter, PluginApi, PluginState},
};

use super::cards::{self, Card, ENVELOPE_TARGETS, LFO_TARGETS};
use super::route_rows;
use super::*;
use crate::routes::{ROUTE_IDS, SourceChoice, TargetChoice};

/// A host that applies each write at once, as a GUI gesture reaches the plugin, and records every
/// call in order by the parameter's name.
#[derive(Default)]
struct ApplyingHost(Mutex<Vec<(&'static str, String)>>);

impl ApplyingHost {
    fn record(&self, action: &'static str, param: ParamPtr) {
        self.0
            .lock()
            .unwrap()
            .push((action, unsafe { param.name() }.to_owned()));
    }

    fn calls(&self) -> Vec<(&'static str, String)> {
        self.0.lock().unwrap().clone()
    }

    fn clear(&self) {
        self.0.lock().unwrap().clear();
    }

    fn is_quiet(&self) -> bool {
        self.0.lock().unwrap().is_empty()
    }

    /// One balanced gesture on `name`, and nothing else; then forgets it.
    fn assert_gesture(&self, name: &str) {
        assert_eq!(
            self.calls(),
            ["begin", "set", "end"].map(|action| (action, name.to_owned())),
        );
        self.clear();
    }
}

impl nice_plug::context::gui::GuiContextInner for ApplyingHost {
    fn request_restart(&self) {}
    fn plugin_api(&self) -> PluginApi {
        PluginApi::Clap
    }
    unsafe fn raw_begin_set_parameter(&self, param: ParamPtr) {
        self.record("begin", param);
    }
    unsafe fn raw_set_parameter_normalized(&self, param: ParamPtr, value: f32) {
        self.record("set", param);
        // SAFETY: the test owns the parameters and no other thread touches them.
        unsafe {
            param._internal_set_normalized_value(value);
        }
    }
    unsafe fn raw_end_set_parameter(&self, param: ParamPtr) {
        self.record("end", param);
    }
    fn get_state(&self) -> PluginState {
        PluginState {
            version: String::new(),
            params: Default::default(),
            fields: Default::default(),
        }
    }
    fn set_state(&self, _state: PluginState) {}
}

/// What this editor keeps behind a disclosure, opened so the checks see it. Nothing: every
/// control is on a card or, for Volume, in the app bar.
const REVEAL: fn(&egui::Context) = |_| {};

/// The editor's transient state, as its app holds it.
type EditorState = (
    usize,
    HashMap<&'static str, Option<String>>,
    PresetUi,
    mxm_ui::navigation::State,
);

fn editor_state(params: &ModalSynthParams) -> EditorState {
    (
        0,
        HashMap::new(),
        PresetUi::at(mxm_preset::Library::at(None), params),
        mxm_ui::navigation::State::default(),
    )
}

/// Drives `panel` with throwaway editor state, which the shared checks run many times over.
fn drive<'a>(
    params: &'a ModalSynthParams,
    telemetry: &'a Telemetry,
    setter: &'a ParamSetter<'a>,
    state: &'a mut EditorState,
) -> impl FnMut(&mut egui::Ui) + 'a {
    move |ui| {
        let (view, text_entry, presets, nav) = state;
        panel(
            ui, params, telemetry, setter, view, text_entry, presets, nav,
        );
    }
}

fn reference() -> egui::Vec2 {
    vec2(REFERENCE.0 as f32, REFERENCE.1 as f32)
}

fn minimum() -> egui::Vec2 {
    vec2(MINIMUM.0 as f32, MINIMUM.1 as f32)
}

/// Sets slot `r` to `source` on `target` at `amount`, as a loaded preset would.
fn route(params: &ModalSynthParams, r: usize, source: Source, target: Target, amount: f32) {
    let slot = params.routes.all()[r];
    // SAFETY: exclusive test ownership; no process or GUI thread exists.
    unsafe {
        slot.source
            ._internal_set_plain_value(SourceChoice(Some(source)));
        slot.target
            ._internal_set_plain_value(TargetChoice(Some(target)));
        slot.amount._internal_set_plain_value(amount);
    }
}

/// Sixteen targets on every card that carries routes, one slot each.
const SPREAD: [Target; 16] = [
    Target::Pitch,
    Target::Model,
    Target::Strength,
    Target::Bow,
    Target::Flow,
    Target::Mallet,
    Target::Brightness,
    Target::Damping,
    Target::Space,
    Target::Pan,
    Target::Amplitude,
    Target::Lfo1Rate,
    Target::Lfo2Rate,
    Target::Lfo3Rate,
    Target::Envelope1Rise,
    Target::Envelope2Input,
];

/// The first `count` of [`SPREAD`] routed, each from its own source at full negative depth — the
/// widest reading a row shows, its sign and every digit.
fn present_routes(params: &ModalSynthParams, count: usize) {
    for (r, target) in SPREAD.into_iter().enumerate().take(count) {
        route(params, r, Source::ALL[r], target, -1.0);
    }
}

/// Every slot in use, on targets on every card that carries routes.
fn present_every_route(params: &ModalSynthParams) {
    present_routes(params, crate::routes::ROUTE_IDS.len());
}

/// All sixteen slots on `target`, each from its own source: its tallest stack.
fn every_slot_on(params: &ModalSynthParams, target: Target) {
    for r in 0..ROUTE_IDS.len() {
        route(params, r, Source::ALL[r], target, -1.0);
    }
}

/// Every id the keyboard may reach: the parameters, but a slot's target, which the card it sits
/// on is, and which adding the route writes.
fn reachable_ids(params: &ModalSynthParams, with_routes: bool) -> Vec<&'static str> {
    let mut ids: Vec<&'static str> = params
        .all_parameters()
        .into_iter()
        .map(|(id, _)| id)
        .filter(|id| {
            let routed = id.starts_with("route");
            if routed {
                with_routes && !id.ends_with("_target")
            } else {
                true
            }
        })
        .collect();
    // Every target's `‹ modulate ›` while it offers a source, and none once the slots are full.
    for target in Target::ALL {
        if !route_rows::offered(&params.routes, target).is_empty() {
            ids.push(Box::leak(route_rows::modulate_key(target).into_boxed_str()));
        }
    }
    ids
}

// ---------------------------------------------------------------------------------------------
// The cards and the inventory
// ---------------------------------------------------------------------------------------------

/// The twelve cards in the conventions' category order, and a minimum that holds the widest.
#[test]
fn card_inventory_and_the_minimum_hold_the_widest_card() {
    assert_eq!(
        CARDS.iter().map(|card| card.title()).collect::<Vec<_>>(),
        [
            "Voice",
            "Envelope 1",
            "Envelope 2",
            "Envelope 3",
            "LFO 1",
            "LFO 2",
            "LFO 3",
            "Model",
            "Exciter",
            "Bow and blow",
            "Strike",
            "Resonator",
        ]
    );
    let categories: Vec<_> = CARDS.iter().map(|card| card.category()).collect();
    let mut sorted = categories.clone();
    sorted.sort();
    assert_eq!(categories, sorted, "the cards are in category order");
    let widest = test_floors().into_iter().fold(0.0, f32::max);
    assert!(
        MINIMUM.0 as f32 >= widest + 2.0 * SPACE_5,
        "the widest card's floor is {widest:.1}; the window minimum must hold it and two gutters"
    );
}

/// **Every musician control is drawn exactly once**, on a card or, for Volume, in the app bar;
/// the route slots are not, their rows drawn because a slot is in use.
#[test]
fn every_host_parameter_is_drawn_once() {
    let params = ModalSynthParams::default();
    let voice: Vec<&str> = params
        .all_parameters()
        .into_iter()
        .map(|(id, _)| id)
        .filter(|id| !id.starts_with("route"))
        .collect();
    let drawn: Vec<&str> = CARDS
        .iter()
        .flat_map(|card| card.parameters())
        .chain([BAR_PARAMETER])
        .collect();
    for id in &voice {
        assert_eq!(drawn.iter().filter(|d| *d == id).count(), 1, "{id}");
    }
    assert_eq!(drawn.len(), voice.len());
    assert_eq!(
        params.all_parameters().len() - voice.len(),
        3 * ROUTE_IDS.len()
    );
}

/// **Each target's routes are on exactly one card**, the card of what it moves: an envelope's six
/// on its own card, an LFO's Rate on its own.
#[test]
fn every_target_has_its_routes_on_exactly_one_card() {
    for target in Target::ALL {
        let on: Vec<&str> = CARDS
            .iter()
            .filter(|card| card.targets().contains(&target))
            .map(|card| card.title())
            .collect();
        assert_eq!(on.len(), 1, "{target:?} is on {on:?}");
    }
    for n in 0..3 {
        assert_eq!(Card::Envelope(n).targets(), &ENVELOPE_TARGETS[n]);
        assert_eq!(Card::Lfo(n).targets(), &[LFO_TARGETS[n]]);
    }
}

/// Every binding says what it does, in a sentence for the player; every segmented cell has its own.
#[test]
fn every_binding_has_a_sentence_and_every_cell_its_own() {
    let params = ModalSynthParams::default();
    for bound in cards::all_bindings(&params) {
        assert!(
            bound.description.ends_with('.') && bound.description.len() > 12,
            "{}",
            bound.id
        );
    }
    for id in [
        "model",
        "mode",
        "env1_mode",
        "env2_mode",
        "env3_mode",
        "lfo1_shape",
    ] {
        let bound = cards::binding_for(id, &params);
        assert_eq!(
            Some(bound.details.len()),
            bound.param.steps().map(|s| s + 1),
            "{id}: a sentence for each cell"
        );
    }
    // A signed depth rests at the centre, in the Parameters list as on a route row.
    for [_, _, amount] in crate::routes::ROUTE_IDS {
        assert!(cards::binding_for(amount, &params).bipolar, "{amount}");
    }
}

/// **Every card passes the layout tree's checks in every state that changes it**
/// (`tree_checks::card`): its computed floor holds its content with nothing painted outside the
/// card, the content floor is exact, the height its tree states is the height it draws, and every
/// leaf stays in the room it was given.
///
/// This editor's structural-state matrix: Init; half the slots in use (rows under a line that still
/// offers `‹ modulate ›`); every slot in use at full negative depth (every line saying all sixteen
/// are in use); all sixteen on one target, its tallest stack; and each LFO synced, with no tempo
/// and with one, where its Rate reads a division. It has no disclosure and reserves nothing.
#[test]
fn every_card_passes_the_tree_checks_in_every_state() {
    let host = ApplyingHost::default();
    let setter = ParamSetter::new(&host);
    for state in [
        "init",
        "half the slots in use",
        "every slot in use",
        "sixteen routes on Brightness",
        "sixteen routes on an envelope's Rise",
        "every LFO synced, no tempo",
        "every LFO synced to a tempo",
    ] {
        let params = ModalSynthParams::default();
        let mut tempo = None;
        match state {
            "half the slots in use" => present_routes(&params, 8),
            "every slot in use" => present_every_route(&params),
            "sixteen routes on Brightness" => every_slot_on(&params, Target::Brightness),
            "sixteen routes on an envelope's Rise" => every_slot_on(&params, Target::Envelope2Rise),
            "every LFO synced, no tempo" | "every LFO synced to a tempo" => {
                for lfo in params.lfos() {
                    // SAFETY: exclusive test ownership.
                    unsafe { lfo.sync._internal_set_plain_value(true) };
                }
                if state.ends_with("tempo") && !state.contains("no tempo") {
                    tempo = Some(120.0);
                }
            }
            _ => {}
        }
        let floors: Vec<f32> = items_in(&params)
            .iter()
            .map(|item| item.card.floor)
            .collect();
        for (index, card) in CARDS.iter().enumerate() {
            let mut entries = HashMap::new();
            let mut live = cards::Live {
                params: &params,
                tempo,
                setter: &setter,
                entries: &mut entries,
            };
            tree_checks::card(
                &|_| {},
                state,
                card.title(),
                floors[index],
                &|ui| cards::card(ui, *card, &params),
                &mut |ui, leaf, rect| cards::paint(ui, &mxm_ui::LIGHT, leaf, rect, &mut live),
            );
        }
        assert!(host.is_quiet(), "drawing a card emitted a host gesture");
    }
}

// ---------------------------------------------------------------------------------------------
// The window: opening size, minimum, paging and reflow
// ---------------------------------------------------------------------------------------------

/// **The editor opens at the quarter-4K budget, hugged** — the owner's rule, 2026-09-09.
#[test]
fn the_opening_size_is_the_budget_hugged() {
    let params = ModalSynthParams::default();
    let telemetry = Telemetry::default();
    let host = ApplyingHost::default();
    let setter = ParamSetter::new(&host);
    let mut state = editor_state(&params);
    opening_size::is_the_budget_hugged(
        reference(),
        &REVEAL,
        &mut drive(&params, &telemetry, &setter, &mut state),
    );
}

/// **The window opens on one page with every card**, as `mxm-editor` opens it: the collection's
/// fonts and theme installed before the first frame (`mxm_ui::shell::install`), then the panel at
/// the opening size.
#[test]
fn the_window_opens_on_one_page_with_every_card() {
    let params = ModalSynthParams::default();
    let telemetry = Telemetry::default();
    let host = ApplyingHost::default();
    let setter = ParamSetter::new(&host);
    let mut state = editor_state(&params);
    let mut panel = drive(&params, &telemetry, &setter, &mut state);
    let ctx = egui::Context::default();
    mxm_ui::theme::apply(&ctx);
    mxm_ui::typography::apply(&ctx);
    ctx.all_styles_mut(|style| style.animation_time = 0.0);
    let input = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, reference())),
        ..Default::default()
    };
    for _ in 0..4 {
        let mut output = ctx.run_ui(input.clone(), &mut panel);
        output.textures_delta.clear();
    }
    let report = mxm_ui::paging::editor::report(&ctx).expect("the panel pages");
    assert_eq!(report.plan.pages.len(), 1, "{:?}", report.plan.pages);
    assert_eq!(report.visible.len(), CARDS.len(), "every card is on it");
}

/// **The app bar holds in the narrowest window**: its `…` menu whole and nothing drawn over
/// anything else, from `MINIMUM` up (`opening_size::bar_holds_from_the_minimum`).
#[test]
fn the_app_bar_holds_in_the_minimum_window() {
    let params = ModalSynthParams::default();
    let telemetry = Telemetry::default();
    let host = ApplyingHost::default();
    let setter = ParamSetter::new(&host);
    let mut state = editor_state(&params);
    opening_size::bar_holds_from_the_minimum(
        minimum(),
        &mut drive(&params, &telemetry, &setter, &mut state),
    );
}

/// Every card reachable on some page, at every size and both scales, light and dark.
#[test]
fn every_dynamic_page_fits_and_every_card_is_reachable() {
    let params = ModalSynthParams::default();
    let telemetry = Telemetry::default();
    let host = ApplyingHost::default();
    let setter = ParamSetter::new(&host);
    let mut state = editor_state(&params);
    paging_checks::verify(
        &test_items(),
        &[
            reference(),
            minimum(),
            vec2(617.0, 562.0),
            vec2(900.0, 700.0),
            vec2(1280.0, 800.0),
        ],
        drive(&params, &telemetry, &setter, &mut state),
    );
    assert!(
        host.is_quiet(),
        "measurement and navigation emitted parameter gestures"
    );
}

/// The cards laid out together in a window `width` wide and tall enough for all of them.
fn render_layout(width: f32) -> Vec<Rect> {
    let params = ModalSynthParams::default();
    let telemetry = Telemetry::default();
    let host = ApplyingHost::default();
    let setter = ParamSetter::new(&host);
    let mut state = editor_state(&params);
    let mut panel = drive(&params, &telemetry, &setter, &mut state);
    let ctx = egui::Context::default();
    mxm_ui::typography::apply(&ctx);
    mxm_ui::theme::apply(&ctx);
    ctx.set_theme(ThemePreference::Light);
    ctx.all_styles_mut(|style| style.animation_time = 0.0);
    let input = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(width, 20_000.0))),
        ..Default::default()
    };
    for _ in 0..3 {
        let mut output = ctx.run_ui(input.clone(), &mut panel);
        output.textures_delta.clear();
    }
    drop(panel);
    assert!(host.is_quiet(), "measurement emitted a host gesture");
    paging_checks::all_rects(&ctx, CARDS.len())
}

/// The cards as rows, read off their drawn rectangles: what a person sees.
fn rows(rects: &[Rect]) -> Vec<Vec<(usize, Rect)>> {
    let mut ordered: Vec<_> = rects.iter().copied().enumerate().collect();
    ordered.sort_by(|(_, a), (_, b)| {
        a.top()
            .total_cmp(&b.top())
            .then(a.left().total_cmp(&b.left()))
    });
    let mut rows: Vec<Vec<(usize, Rect)>> = Vec::new();
    for item in ordered {
        match rows.last_mut() {
            Some(row)
                if row
                    .iter()
                    .any(|(_, rect)| rect.bottom() > item.1.top() + 1.0) =>
            {
                row.push(item);
            }
            _ => rows.push(vec![item]),
        }
    }
    rows
}

/// **Rows, not columns** (`docs/plugin-conventions.md`, *A column test keeps passing on a layout
/// with no columns*): every card at its floor, no two overlapping, the cards of a row ending on one
/// line, and the order the cards read in the category order.
#[test]
fn narrow_default_and_wide_reflow_keep_floors_order_rows_and_no_overlap() {
    let floors = test_floors();
    for width in [MINIMUM.0 as f32, REFERENCE.0 as f32, 2200.0] {
        let rects = render_layout(width);
        for (index, (rect, floor)) in rects.iter().zip(&floors).enumerate() {
            assert!(
                (rect.width() - floor).abs() <= 0.75,
                "{} is not its {floor}-point floor at {width}: {rect:?}",
                CARDS[index].title()
            );
            assert!(
                rect.left() >= SPACE_5 - 0.75 && rect.right() <= width - SPACE_5 + 0.75,
                "{} leaves the workspace at {width}: {rect:?}",
                CARDS[index].title()
            );
        }
        for (index, first) in rects.iter().enumerate() {
            for second in rects.iter().skip(index + 1) {
                let overlap = first.intersect(*second);
                assert!(
                    overlap.width() <= 0.75 || overlap.height() <= 0.75,
                    "cards overlap at {width}: {first:?} and {second:?}"
                );
            }
        }
        for row in rows(&rects) {
            let bottoms: Vec<f32> = row.iter().map(|(_, rect)| rect.bottom()).collect();
            let low = bottoms.iter().copied().fold(f32::INFINITY, f32::min);
            let high = bottoms.iter().copied().fold(f32::NEG_INFINITY, f32::max);
            assert!(
                high - low < 1.0,
                "row bottoms differ by {:.1} at {width}: {row:?}",
                high - low
            );
        }
        let order: Vec<usize> = rows(&rects)
            .into_iter()
            .flatten()
            .map(|(index, _)| index)
            .collect();
        assert_eq!(
            order,
            (0..CARDS.len()).collect::<Vec<_>>(),
            "reflow changed the cards' order at {width}"
        );
    }
}

/// **No text inside a card is cut away by its own clip**, at the opening size and the minimum, at
/// one and two times, with every slot in use so the densest cards are the ones under test. The app
/// bar is excluded: its preset name is variable-length and truncates by design.
#[test]
fn nothing_inside_a_card_is_clipped_at_any_supported_size() {
    let params = ModalSynthParams::default();
    present_every_route(&params);
    let telemetry = Telemetry::default();
    let host = ApplyingHost::default();
    let setter = ParamSetter::new(&host);
    let items = test_items();
    for physical in [reference(), minimum()] {
        for scale in [1.0, 2.0] {
            let size = physical / scale;
            let mut state = editor_state(&params);
            let mut panel = drive(&params, &telemetry, &setter, &mut state);
            let ctx = egui::Context::default();
            ctx.set_pixels_per_point(scale);
            mxm_ui::typography::apply(&ctx);
            mxm_ui::theme::apply(&ctx);
            ctx.all_styles_mut(|style| style.animation_time = 0.0);
            let screen = Rect::from_min_size(egui::Pos2::ZERO, size);
            for item in &items {
                mxm_ui::paging::editor::request_card(&ctx, item.key);
                let mut last = None;
                for _ in 0..4 {
                    let mut output = ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(screen),
                            ..Default::default()
                        },
                        &mut panel,
                    );
                    output.textures_delta.clear();
                    last = Some(output.shapes);
                }
                for clipped in last.into_iter().flatten() {
                    let egui::Shape::Text(text) = &clipped.shape else {
                        continue;
                    };
                    let painted = text.visual_bounding_rect();
                    if !painted.is_positive() {
                        continue;
                    }
                    let visible = painted.intersect(clipped.clip_rect);
                    let lost = painted.width() - visible.width().max(0.0);
                    let in_the_app_bar = clipped.clip_rect.height() < 64.0;
                    assert!(
                        lost <= 1.0 || in_the_app_bar,
                        "card {:?} at {physical:?} x{scale}: {lost:.1} pt of {:?} is clipped away",
                        item.card.title,
                        text.galley.text()
                    );
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The keyboard cursor and its keys
// ---------------------------------------------------------------------------------------------

/// **The cursor lands, operates and reaches every control** (`keyboard_checks`): a control whose
/// `navigation::at` scope was forgotten paints as before and is simply unreachable, which nothing
/// else would say. At Init no slot is in use, so no route row is drawn.
#[test]
fn the_keyboard_cursor_reaches_and_operates_every_parameter() {
    let params = ModalSynthParams::default();
    let telemetry = Telemetry::default();
    let recorder = keyboard_checks::Recorder::default();
    let setter = ParamSetter::new(&recorder);
    let mut state = editor_state(&params);
    let ids = reachable_ids(&params, false);
    keyboard_checks::the_cursor_reaches_and_operates(
        reference(),
        &test_items(),
        keyboard_checks::Coverage::Exactly(&ids),
        &REVEAL,
        &recorder,
        &mut drive(&params, &telemetry, &setter, &mut state),
    );
}

/// **With every slot in use the cursor reaches every route**: each slot's amount and its remove
/// (the source's control), on the card of its target — and nothing that is not a parameter.
#[test]
fn the_keyboard_cursor_reaches_every_route() {
    let params = ModalSynthParams::default();
    present_every_route(&params);
    let telemetry = Telemetry::default();
    let recorder = keyboard_checks::Recorder::default();
    let setter = ParamSetter::new(&recorder);
    let mut state = editor_state(&params);
    let ids = reachable_ids(&params, true);
    keyboard_checks::the_cursor_reaches_and_operates(
        reference(),
        &test_items(),
        keyboard_checks::Coverage::Exactly(&ids),
        &REVEAL,
        &recorder,
        &mut drive(&params, &telemetry, &setter, &mut state),
    );
}

/// Presses `keys` one after another, each pressed and released.
fn taps(keys: &[egui::Key]) -> Vec<egui::Event> {
    keys.iter()
        .flat_map(|&k| keyboard_checks::press(k, egui::Modifiers::NONE))
        .collect()
}

/// A mouse drag from `at` by `by` in `steps`, BACK pressed while it is held, then the release.
fn drag_then_back(
    session: &keyboard_checks::Session,
    panel: &mut impl FnMut(&mut egui::Ui),
    at: egui::Pos2,
    by: egui::Vec2,
    during: &mut dyn FnMut(),
) {
    let button = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    session.frame(panel, vec![egui::Event::PointerMoved(at), button(at, true)]);
    let steps = 4;
    for step in 1..=steps {
        session.frame(
            panel,
            vec![egui::Event::PointerMoved(at + by * step as f32)],
        );
        during();
    }
    session.frame(panel, taps(&[key_of(Job::Back)]));
    session.frame(panel, vec![button(at + by * steps as f32, false)]);
    session.frame(panel, Vec::new());
}

/// **BACK cancels a mouse drag** (the owner, 2026-10-08: the plugins match newDAWn): on a knob and
/// on a route row's amount the control puts back the value the drag began at, and the host hears a
/// gesture that opens and closes.
#[test]
fn back_cancels_a_drag_on_a_knob_and_on_a_route_row() {
    let params = ModalSynthParams::default();
    route(&params, 0, Source::Lfo1, Target::Brightness, 0.25);
    let telemetry = Telemetry::default();
    let host = ApplyingHost::default();
    let setter = ParamSetter::new(&host);
    let mut state = editor_state(&params);
    let mut panel = drive(&params, &telemetry, &setter, &mut state);
    let session = keyboard_checks::Session::new(reference());
    mxm_ui::paging::editor::request_card(
        session.context(),
        mxm_ui::paging::Key(index_of(Card::Resonator) as u64),
    );
    session.settle(&mut panel);
    let spot = |key: &str| {
        mxm_ui::navigation::spots(session.context())
            .into_iter()
            .find(|spot| spot.key == key)
            .unwrap_or_else(|| panic!("{key} is on the page"))
            .rect
    };
    for (id, by) in [
        ("brightness", vec2(0.0, -12.0)),
        ("route1_amount", vec2(14.0, 0.0)),
    ] {
        let param = cards::param_of(id, &params);
        let before = param.normalised();
        let rect = spot(id);
        // A knob is grabbed anywhere; a slider on its track, at the bottom of its row.
        let at = if id == "brightness" {
            rect.center()
        } else {
            egui::pos2(rect.center().x, rect.bottom() - 4.0)
        };
        let mut moved = false;
        drag_then_back(&session, &mut panel, at, by, &mut || {
            moved |= (param.normalised() - before).abs() > 1e-6;
        });
        assert!(moved, "{id}: the drag moved it");
        assert_eq!(
            param.normalised(),
            before,
            "{id}: BACK put back the value the drag began at"
        );
        let calls = host.calls();
        let opened = calls
            .iter()
            .filter(|(action, _)| *action == "begin")
            .count();
        let closed = calls.iter().filter(|(action, _)| *action == "end").count();
        assert_eq!(opened, closed, "{id}: an unbalanced gesture {calls:?}");
        host.clear();
    }
}

/// **F1 shows the keys** — a sheet from the keymap in use — **and `Escape` closes it**, with nothing
/// wired here: the cursor draws it.
#[test]
fn f1_shows_the_keys_and_escape_closes_them() {
    let params = Arc::new(ModalSynthParams::default());
    let telemetry = Arc::new(Telemetry::default());
    let host = Arc::new(ApplyingHost::default());
    let mut harness = editor_harness(params, telemetry, Arc::clone(&host), reference());
    harness.run_steps(4);
    assert!(harness.query_by_label_contains("show or hide it").is_none());
    harness.key_press(egui::Key::F1);
    harness.run_steps(3);
    assert!(
        harness.query_by_label_contains("show or hide it").is_some(),
        "F1 shows the keys"
    );
    harness.key_press(egui::Key::Escape);
    harness.run_steps(3);
    assert!(
        harness.query_by_label_contains("show or hide it").is_none(),
        "Escape closes the sheet"
    );
    assert!(host.is_quiet(), "the sheet edits nothing");
}

// ---------------------------------------------------------------------------------------------
// The route rows
// ---------------------------------------------------------------------------------------------

/// **A card's `‹ modulate ›` takes a free slot in one gesture** — its source, its target and its
/// depth at zero — the first free one (`Routes::first_free`), or one this target had, whose depth
/// it keeps; a depth left in a slot aimed elsewhere is not inherited.
#[test]
fn adding_a_route_takes_a_free_slot_in_one_gesture() {
    let params = ModalSynthParams::default();
    let routes = &params.routes;
    route(&params, 0, Source::Lfo1, Target::Strength, 0.5);
    // A leftover depth in slot 2, aimed elsewhere with no source: free, and not inherited.
    // SAFETY: exclusive test ownership.
    unsafe {
        routes
            .route2
            .target
            ._internal_set_plain_value(TargetChoice(Some(Target::Pan)));
        routes.route2.amount._internal_set_plain_value(0.7);
    }
    assert_eq!(routes.first_free(), Some(1));
    let host = ApplyingHost::default();
    let setter = ParamSetter::new(&host);
    route_rows::add(routes, Target::Brightness, Source::Wheel, &setter);
    assert_eq!(
        routes.route2.source.value(),
        SourceChoice(Some(Source::Wheel))
    );
    assert_eq!(
        routes.route2.target.value(),
        TargetChoice(Some(Target::Brightness))
    );
    assert_eq!(routes.route2.amount.value(), 0.0);
    assert_eq!(route_rows::routes_on(routes, Target::Brightness), vec![1]);
    let calls = host.calls();
    let phase = |action: &str| match action {
        "begin" => 0,
        "set" => 1,
        _ => 2,
    };
    assert_eq!(calls.len(), 9, "three writes, each bracketed: {calls:?}");
    assert!(calls.windows(2).all(|w| phase(w[0].0) <= phase(w[1].0)));
    host.clear();

    // Removed and added back on the same card, the route keeps its depth, though slot 3 is free
    // before it would be.
    // SAFETY: as above.
    unsafe { routes.route2.amount._internal_set_plain_value(-0.4) };
    route_rows::remove(&routes.route2, &setter);
    host.assert_gesture("Route 2 source");
    assert!(route_rows::routes_on(routes, Target::Brightness).is_empty());
    assert_eq!(
        routes.route2.target.value(),
        TargetChoice(Some(Target::Brightness))
    );
    route_rows::add(routes, Target::Brightness, Source::Lfo2, &setter);
    assert_eq!(routes.route2.amount.value(), -0.4);
    assert_eq!(
        routes.route2.source.value(),
        SourceChoice(Some(Source::Lfo2))
    );
}

/// **What a card offers is what it can still take**: never a source already on that target, never
/// one the standard refuses on it (a gesture into a gate), and nothing anywhere once all sixteen
/// slots are in use.
#[test]
fn a_card_offers_what_it_can_still_take() {
    let params = ModalSynthParams::default();
    let routes = &params.routes;
    assert_eq!(
        route_rows::offered(routes, Target::Brightness).len(),
        Source::ALL.len()
    );
    let gate = route_rows::offered(routes, Target::Envelope1Trigger);
    for source in Source::ALL {
        assert_eq!(
            gate.contains(&source),
            Target::Envelope1Trigger.offer(source) != mxm_modal_synth_dsp::Offer::Refused,
            "{source:?} into a Trigger"
        );
    }
    assert!(!gate.contains(&Source::Wheel) && gate.contains(&Source::Lfo1));
    route(&params, 0, Source::Lfo1, Target::Brightness, 0.2);
    assert!(!route_rows::offered(routes, Target::Brightness).contains(&Source::Lfo1));
    assert!(route_rows::offered(routes, Target::Damping).contains(&Source::Lfo1));
    present_every_route(&params);
    for target in Target::ALL {
        assert!(route_rows::offered(routes, target).is_empty(), "{target:?}");
        assert_eq!(route_rows::free_slot(routes, target), None);
    }
}

/// The words painted on `card`'s page with `params` as they stand.
fn painted_words(params: &ModalSynthParams, card: Card) -> Vec<String> {
    fn collect(shape: &egui::Shape, out: &mut Vec<String>) {
        match shape {
            egui::Shape::Text(text) => out.push(text.galley.text().to_owned()),
            egui::Shape::Vec(shapes) => {
                for shape in shapes {
                    collect(shape, out);
                }
            }
            _ => {}
        }
    }
    let telemetry = Telemetry::default();
    let host = ApplyingHost::default();
    let setter = ParamSetter::new(&host);
    let mut state = editor_state(params);
    let mut panel = drive(params, &telemetry, &setter, &mut state);
    let ctx = egui::Context::default();
    mxm_ui::typography::apply(&ctx);
    mxm_ui::theme::apply(&ctx);
    ctx.all_styles_mut(|style| style.animation_time = 0.0);
    mxm_ui::paging::editor::request_card(&ctx, mxm_ui::paging::Key(index_of(card) as u64));
    let mut words = Vec::new();
    for _ in 0..4 {
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, reference())),
                ..Default::default()
            },
            &mut panel,
        );
        output.textures_delta.clear();
        words.clear();
        for clipped in &output.shapes {
            collect(&clipped.shape, &mut words);
        }
    }
    words
}

/// **Once all sixteen slots are in use, a card says so**: every target's line reads
/// `‹ all 16 in use ›` where it offered `‹ modulate ›`, and each slot in use is a row named by its
/// source on the card of its target.
#[test]
fn once_every_slot_is_in_use_a_card_says_so() {
    let params = ModalSynthParams::default();
    let words = painted_words(&params, Card::Resonator);
    assert!(words.iter().any(|w| w == route_rows::MODULATE));
    assert!(!words.iter().any(|w| w == route_rows::FULL));

    present_every_route(&params);
    for card in [Card::Resonator, Card::Envelope(1), Card::Voice] {
        let words = painted_words(&params, card);
        assert!(
            words.iter().any(|w| w == route_rows::FULL),
            "{} says the slots are full: {words:?}",
            card.title()
        );
        assert!(
            !words.iter().any(|w| w == route_rows::MODULATE),
            "{} offers nothing more",
            card.title()
        );
    }
    // Brightness's one slot is a row painted as its source.
    let timbre = SPREAD
        .iter()
        .position(|t| *t == Target::Brightness)
        .unwrap();
    let words = painted_words(&params, Card::Resonator);
    assert!(
        words
            .iter()
            .any(|w| w == SourceChoice(Some(Source::ALL[timbre])).name()),
        "the row reads its source"
    );
}

/// A slot whose target is Off, or whose source is, is on no card.
#[test]
fn a_slot_not_in_use_appears_nowhere() {
    let params = ModalSynthParams::default();
    // SAFETY: exclusive test ownership.
    unsafe {
        params
            .routes
            .route1
            .source
            ._internal_set_plain_value(SourceChoice(Some(Source::Lfo3)));
        params.routes.route1.amount._internal_set_plain_value(0.5);
        params
            .routes
            .route2
            .target
            ._internal_set_plain_value(TargetChoice(Some(Target::Damping)));
    }
    for target in Target::ALL {
        assert!(route_rows::routes_on(&params.routes, target).is_empty());
    }
    let telemetry = Telemetry::default();
    let recorder = keyboard_checks::Recorder::default();
    let setter = ParamSetter::new(&recorder);
    let mut state = editor_state(&params);
    let mut panel = drive(&params, &telemetry, &setter, &mut state);
    let session = keyboard_checks::Session::new(reference());
    let mut found = HashSet::new();
    for item in &test_items() {
        mxm_ui::paging::editor::request_card(session.context(), item.key);
        session.settle(&mut panel);
        found.extend(session.registered());
    }
    assert!(!found.iter().any(|id| id.starts_with("route")), "{found:?}");
}

// ---------------------------------------------------------------------------------------------
// The editor through its accessibility tree
// ---------------------------------------------------------------------------------------------

fn editor_harness(
    params: Arc<ModalSynthParams>,
    telemetry: Arc<Telemetry>,
    host: Arc<ApplyingHost>,
    size: egui::Vec2,
) -> egui_kittest::Harness<'static> {
    let params = Box::leak(Box::new(params));
    let telemetry = Box::leak(Box::new(telemetry));
    let host = Box::leak(Box::new(host));
    let view = Box::leak(Box::new(0usize));
    let entries = Box::leak(Box::new(HashMap::new()));
    let presets = Box::leak(Box::new(PresetUi::at(
        mxm_preset::Library::at(None),
        params.as_ref(),
    )));
    // Leaked with the rest: the harness keeps the panel past this call, and the cursor is
    // per-editor state that has to survive between its frames.
    let nav = Box::leak(Box::new(mxm_ui::navigation::State::default()));
    egui_kittest::Harness::builder()
        .with_size(size)
        .build_ui(move |ui| {
            mxm_ui::typography::apply(ui.ctx());
            mxm_ui::theme::apply(ui.ctx());
            panel(
                ui,
                params.as_ref(),
                telemetry.as_ref(),
                &ParamSetter::new(host.as_ref()),
                view,
                entries,
                presets,
                nav,
            );
        })
}

/// A control the app bar draws: **once**, above every card, and inside the window.
fn assert_in_the_app_bar(harness: &egui_kittest::Harness<'static>, label: &str, width: f32) {
    let nodes: Vec<_> = harness.query_all_by_label(label).collect();
    assert_eq!(nodes.len(), 1, "{label} must be drawn exactly once");
    let control = nodes[0].rect();
    let viewport = mxm_ui::paging::editor::report(&harness.ctx)
        .expect("production paging report")
        .viewport;
    assert!(
        control.bottom() <= viewport.top(),
        "{label} {control:?} is not in the app bar above the cards {viewport:?}"
    );
    assert!(
        control.left() >= 0.0 && control.right() <= width,
        "{label} {control:?} leaves the {width}-point window"
    );
}

/// Whether a control labelled `label` paints inside card `index`, scrolled to if it must be.
fn assert_label_reachable_in_card(
    harness: &mut egui_kittest::Harness<'static>,
    index: usize,
    label: &str,
    context: &str,
) {
    let card_rect = |harness: &egui_kittest::Harness<'static>| {
        mxm_ui::paging::editor::report(&harness.ctx)
            .expect("production paging report")
            .visible
            .iter()
            .find(|(key, _)| key.0 == index as u64)
            .unwrap_or_else(|| panic!("card {index} is visible ({context})"))
            .1
    };
    let card = card_rect(harness);
    let node = harness
        .query_all_by_label_contains(label)
        .find(|node| card.contains_rect(node.rect()))
        .unwrap_or_else(|| panic!("{label} did not paint in card {index} ({context})"));
    let viewport = mxm_ui::paging::editor::report(&harness.ctx)
        .unwrap()
        .viewport;
    if !viewport.contains_rect(node.rect()) {
        node.scroll_to_me();
        harness.run_steps(3);
    }
    let card = card_rect(harness);
    let viewport = mxm_ui::paging::editor::report(&harness.ctx)
        .unwrap()
        .viewport;
    let node = harness
        .query_all_by_label_contains(label)
        .find(|node| card.contains_rect(node.rect()))
        .unwrap_or_else(|| panic!("{label} disappeared from card {index} ({context})"));
    assert!(
        viewport.contains_rect(node.rect()),
        "{label} cannot be scrolled into the window ({context}): {:?}",
        node.rect()
    );
}

/// **Every musician control paints inside its card, light and dark, at one and two times**, the
/// physical window held fixed: by the accessible name a screen reader and a test find it by.
#[test]
fn every_musician_control_paints_inside_its_card_across_scales_and_themes() {
    let names: HashMap<&str, String> = ModalSynthParams::default()
        .all_parameters()
        .into_iter()
        .map(|(id, parameter)| (id, parameter.name().to_owned()))
        .collect();
    let fixed_physical_size = vec2(1_880.0, 1_040.0);
    for theme in [0, 1] {
        for scale in [1.0, 2.0] {
            let params = Arc::new(ModalSynthParams::default());
            let telemetry = Arc::new(Telemetry::default());
            telemetry.dev.request_theme(theme);
            let host = Arc::new(ApplyingHost::default());
            let logical_size = fixed_physical_size / scale;
            let mut harness = editor_harness(
                params,
                Arc::clone(&telemetry),
                Arc::clone(&host),
                fixed_physical_size,
            );
            harness.ctx.set_pixels_per_point(scale);
            harness.run_steps(20);
            let mut painted = HashSet::new();
            for (index, card) in CARDS.iter().enumerate() {
                mxm_ui::paging::editor::request_card(
                    &harness.ctx,
                    mxm_ui::paging::Key(index as u64),
                );
                harness.run_steps(3);
                let context = format!("theme {theme}, scale {scale}");
                for id in card.parameters() {
                    assert_label_reachable_in_card(&mut harness, index, &names[id], &context);
                    painted.insert(id);
                }
            }
            assert_in_the_app_bar(&harness, &names[BAR_PARAMETER], logical_size.x);
            painted.insert(BAR_PARAMETER);
            assert_eq!(
                painted.len(),
                names.len() - 3 * ROUTE_IDS.len(),
                "not every musician control painted at theme {theme}, scale {scale}"
            );
            assert!(host.is_quiet());
        }
    }
}

/// **A switch's cell, a toggle, a knob and Volume each edit as one balanced host gesture.**
#[test]
fn a_switch_a_toggle_a_knob_and_volume_each_emit_one_balanced_gesture() {
    let params = Arc::new(ModalSynthParams::default());
    let telemetry = Arc::new(Telemetry::default());
    let host = Arc::new(ApplyingHost::default());
    let mut harness = editor_harness(
        Arc::clone(&params),
        telemetry,
        Arc::clone(&host),
        vec2(REFERENCE.0 as f32, 1800.0),
    );
    harness.run_steps(3);

    mxm_ui::paging::editor::request_card(
        &harness.ctx,
        mxm_ui::paging::Key(index_of(Card::Voice) as u64),
    );
    harness.run_steps(3);
    harness.get_by_label("Voice mode: Mono").click();
    harness.run_steps(2);
    host.assert_gesture("Voice mode");

    mxm_ui::paging::editor::request_card(
        &harness.ctx,
        mxm_ui::paging::Key(index_of(Card::Lfo(0)) as u64),
    );
    harness.run_steps(3);
    harness.get_by_label("LFO 1 retrigger").click();
    harness.run_steps(2);
    host.assert_gesture("LFO 1 retrigger");

    mxm_ui::paging::editor::request_card(
        &harness.ctx,
        mxm_ui::paging::Key(index_of(Card::Resonator) as u64),
    );
    harness.run_steps(3);
    // The Brightness knob, not its routes' `‹ modulate ›`, which shares its name.
    let brightness = harness.get(
        By::new()
            .label("Brightness")
            .role(egui::accesskit::Role::Slider),
    );
    brightness.focus();
    for key in [key_of(VALUE), egui::Key::ArrowUp, key_of(OUT)] {
        harness.key_press(key);
    }
    harness.run_steps(2);
    host.assert_gesture("Brightness");

    let volume = harness.get_by_label("Volume");
    volume.focus();
    for key in [key_of(VALUE), egui::Key::ArrowUp, key_of(OUT)] {
        harness.key_press(key);
    }
    harness.run_steps(2);
    host.assert_gesture("Volume");
}

/// **A route is added from the keyboard**: the cursor on a target's `‹ modulate ›`, MENU opens its
/// list, ↓ and Enter pick its first source, and the free slot is written in one gesture, as a click
/// on the row would write it.
#[test]
fn a_route_is_added_from_the_keyboard() {
    let params = Arc::new(ModalSynthParams::default());
    let telemetry = Arc::new(Telemetry::default());
    let host = Arc::new(ApplyingHost::default());
    let mut harness = editor_harness(
        Arc::clone(&params),
        telemetry,
        Arc::clone(&host),
        reference(),
    );
    harness.run_steps(3);
    // `‹ modulate ›` on Pan's line, on the Voice card.
    harness
        .get(
            By::new()
                .label("Pan: modulate")
                .role(egui::accesskit::Role::ComboBox),
        )
        .focus();
    harness.run_steps(2);
    harness.key_press(key_of(keyboard_checks::MENU));
    harness.run_steps(3);
    harness.key_press(egui::Key::ArrowDown);
    harness.run_steps(2);
    harness.key_press(egui::Key::Enter);
    harness.run_steps(3);
    let first = route_rows::offered(&ModalSynthParams::default().routes, Target::Pan)[0];
    assert_eq!(
        params.routes.route1.source.value(),
        SourceChoice(Some(first))
    );
    assert_eq!(
        params.routes.route1.target.value(),
        TargetChoice(Some(Target::Pan))
    );
    // One gesture: both opened before the first value and closed after the last.
    let names = ["Route 1 source", "Route 1 target"];
    let expected: Vec<(&str, String)> = ["begin", "set", "end"]
        .into_iter()
        .flat_map(|action| names.map(|name| (action, name.to_owned())))
        .collect();
    assert_eq!(host.calls(), expected);
}

/// **A route row's remove is one write**: the source off, the target and the depth kept.
#[test]
fn a_route_rows_remove_is_one_write_that_keeps_the_depth() {
    let params = Arc::new(ModalSynthParams::default());
    route(&params, 0, Source::Lfo1, Target::Brightness, 0.3);
    let telemetry = Arc::new(Telemetry::default());
    let host = Arc::new(ApplyingHost::default());
    let mut harness = editor_harness(
        Arc::clone(&params),
        telemetry,
        Arc::clone(&host),
        reference(),
    );
    harness.run_steps(3);
    mxm_ui::paging::editor::request_card(
        &harness.ctx,
        mxm_ui::paging::Key(index_of(Card::Resonator) as u64),
    );
    harness.run_steps(4);
    harness.get_by_label("Remove LFO 1 from Brightness").click();
    harness.run_steps(3);
    host.assert_gesture("Route 1 source");
    assert_eq!(params.routes.route1.source.value(), SourceChoice(None));
    assert_eq!(
        params.routes.route1.target.value(),
        TargetChoice(Some(Target::Brightness))
    );
    assert_eq!(params.routes.route1.amount.value(), 0.3);
}

/// **The theme in both forms**: the panel paints a card at the opening size and the minimum in
/// light and in dark.
#[test]
fn the_panel_paints_at_the_opening_and_minimum_sizes_in_both_themes() {
    let params = ModalSynthParams::default();
    let telemetry = Telemetry::default();
    let host = ApplyingHost::default();
    let setter = ParamSetter::new(&host);
    for theme in [ThemePreference::Light, ThemePreference::Dark] {
        for size in [reference(), minimum()] {
            let mut state = editor_state(&params);
            let mut panel = drive(&params, &telemetry, &setter, &mut state);
            let ctx = egui::Context::default();
            mxm_ui::typography::apply(&ctx);
            mxm_ui::theme::apply(&ctx);
            ctx.set_theme(theme);
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, size)),
                ..Default::default()
            };
            for _ in 0..3 {
                let mut output = ctx.run_ui(input.clone(), &mut panel);
                output.textures_delta.clear();
            }
            let report = mxm_ui::paging::editor::report(&ctx).expect("the panel pages");
            assert!(!report.visible.is_empty(), "a card is visible at {size}");
            let dark = theme == ThemePreference::Dark;
            assert_eq!(
                mxm_ui::theme::tokens(&ctx).canvas,
                if dark { mxm_ui::DARK } else { mxm_ui::LIGHT }.canvas
            );
        }
    }
    assert!(host.is_quiet());
}

/// **The developer channel's requests reach the editor and edit nothing**: CC 116 the theme,
/// CC 117 the preset browser, CC 119 a category's first card and 127 the Parameters list, every
/// parameter in it by its full name; CC 118 asks for an expander this editor has not got, and
/// changes nothing.
#[test]
fn developer_requests_switch_theme_browser_category_and_parameters_without_edits() {
    let params = Arc::new(ModalSynthParams::default());
    let telemetry = Arc::new(Telemetry::default());
    let host = Arc::new(ApplyingHost::default());
    let mut harness = editor_harness(
        Arc::clone(&params),
        Arc::clone(&telemetry),
        Arc::clone(&host),
        reference(),
    );
    harness.run_steps(20);
    telemetry.dev.request_theme(1);
    harness.run_steps(3);
    assert_eq!(
        mxm_ui::theme::tokens(&harness.ctx).canvas,
        mxm_ui::DARK.canvas,
        "the developer theme request was not applied"
    );
    telemetry.dev.request_browser(true);
    harness.run_steps(3);
    harness.get_by_label("Banks");
    telemetry.dev.request_browser(false);
    harness.run_steps(3);

    // Tone, whose first card is the resonator's.
    telemetry.dev.request_view(4);
    harness.run_steps(4);
    let report = mxm_ui::paging::editor::report(&harness.ctx).unwrap();
    assert!(
        report
            .visible
            .iter()
            .any(|(key, _)| key.0 == index_of(Card::Resonator) as u64),
        "CC 119 4 shows Tone"
    );
    telemetry.dev.request_disclosure(true);
    harness.run_steps(3);
    assert!(host.is_quiet());

    telemetry.dev.request_view(mxm_ui::paging::PARAMETERS as u8);
    harness.run_steps(4);
    for (id, parameter) in params.all_parameters() {
        // The app bar draws Volume on every view, this one included.
        let expected = if id == BAR_PARAMETER { 2 } else { 1 };
        assert_eq!(
            harness.query_all_by_label(parameter.name()).count(),
            expected,
            "{id}"
        );
    }
    assert!(
        host.is_quiet(),
        "private editor state emitted parameter gestures"
    );
}

/// Every page at the opening size, light and dark, for the owner's review:
/// `target/layout-tree/mxm-modal-synth/<tag>/init/`, the opening as Init has it, and `…/routed/`,
/// three slots in use so the rows show, where `MXM_PICTURES` names the tag.
///
/// `MXM_PICTURES=after cargo test -p mxm-modal-synth --lib tree_pictures -- --ignored`
#[test]
#[ignore = "renders through wgpu; run by hand"]
fn tree_pictures() {
    let tag = std::env::var("MXM_PICTURES").unwrap_or_else(|_| "after".to_owned());
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/layout-tree/mxm-modal-synth")
        .join(tag);
    for routed in [false, true] {
        let params = ModalSynthParams::default();
        if routed {
            route(&params, 0, Source::Lfo1, Target::Brightness, 0.35);
            route(&params, 1, Source::Envelope1, Target::Strength, 1.0);
            route(&params, 2, Source::Velocity, Target::Envelope1Time, -0.5);
        }
        let telemetry = Telemetry::default();
        let host = ApplyingHost::default();
        let setter = ParamSetter::new(&host);
        let mut state = editor_state(&params);
        // The harness draws its first frame before the collection's fonts take effect, and the
        // pager keeps the plan it made in egui's own; a window installs them before its first
        // frame. So that plan is forgotten here and made again in them, as a window makes it.
        tree_checks::pictures(
            &|ctx| ctx.data_mut(|data| data.clear()),
            reference(),
            &dir.join(if routed { "routed" } else { "init" }),
            &mut drive(&params, &telemetry, &setter, &mut state),
        );
    }
}
