//! The editor: a software-native panel over the collection's shell (`mxm-editor`, `mxm-ui`), its
//! cards reflowing through the shared paging renderer.
//!
//! **Twelve cards in the conventions' category order** ([`cards::CARDS`]): Voice; the three
//! envelopes and the three LFOs; the model and its tuning, the exciters' levels and Contour, the
//! bow's and the blow's timbres, the strike's; the resonator with Space. **A route slot is a row on
//! the card of the target it
//! moves**, as mxm-drum-machine's are under the knob each moves ([`route_rows`]): a card's
//! `‹ modulate ›` takes a free slot, and says so once all sixteen are in use. Volume, the
//! instrument's output level, is in the app bar beside the meter (design system §3.1 slot 6).
//!
//! Every sound edit is a host parameter, through the collection's one binding; telemetry is read,
//! never written, and the DSP reads nothing of the editor.

pub mod binding;
pub mod cards;
pub mod route_rows;

use std::{collections::HashMap, sync::Arc};

use egui::Ui;
use mxm_ui::{space::SPACE_5, theme::Tokens};
use nice_plug::{context::gui::GuiContext, prelude::*};

use self::cards::CARDS;
use crate::{params::ModalSynthParams, telemetry::Telemetry};

pub use mxm_preset::PresetUi;

/// The opening frame, **the quarter-4K budget hugged**: the panel laid out at the budget, every
/// page's slack taken away, which is every card on one page. Measured by
/// `proof::the_opening_size_is_the_budget_hugged`, which fails with the size to take, not chosen;
/// held ten points inside that check's gutter, so the one page keeps a little more than a hair of
/// room (`proof::the_window_opens_on_one_page_with_every_card`).
const REFERENCE: (u32, u32) = (1821, 1080);
/// The narrowest window: the wider of the widest card with the shell's two gutters and the app bar
/// at its last compact step. Held by `proof::the_app_bar_holds_in_the_minimum_window` and
/// `proof::card_inventory_and_the_minimum_hold_the_widest_card`.
const MINIMUM: (u32, u32) = (446, 360);

/// The one parameter drawn outside the cards: the instrument's output level, which design system
/// §3.1 puts in the app bar beside the meter.
pub(crate) const BAR_PARAMETER: &str = "volume";
/// The keyboard cursor's card for [`BAR_PARAMETER`], outside the paging keys.
const VOLUME_CARD: u64 = 64;

pub type MxmModalSynthEditor = mxm_editor::Editor<MxmModalSynthApp>;

pub fn create(
    params: Arc<ModalSynthParams>,
    telemetry: Arc<Telemetry>,
) -> Option<MxmModalSynthEditor> {
    mxm_editor::create(
        crate::NAME,
        REFERENCE,
        MINIMUM,
        MxmModalSynthApp::new(params, telemetry),
    )
}

pub struct MxmModalSynthApp {
    params: Arc<ModalSynthParams>,
    telemetry: Arc<Telemetry>,
    /// The musician surface (0) or the developer Parameters list (`paging::PARAMETERS`).
    view: usize,
    text_entry: HashMap<&'static str, Option<String>>,
    presets: PresetUi,
    /// Where the keyboard is: a card, and a parameter inside it. Transient, like the text buffers;
    /// it is not a parameter and nothing durable reads it.
    nav: mxm_ui::navigation::State,
}

impl MxmModalSynthApp {
    pub fn new(params: Arc<ModalSynthParams>, telemetry: Arc<Telemetry>) -> Self {
        let presets = PresetUi::new(params.as_ref());
        Self {
            params,
            telemetry,
            view: 0,
            text_entry: HashMap::new(),
            presets,
            nav: mxm_ui::navigation::State::default(),
        }
    }
}

impl mxm_editor::Panel for MxmModalSynthApp {
    fn ui(&mut self, ui: &mut Ui, gui: &GuiContext) {
        panel(
            ui,
            &self.params,
            &self.telemetry,
            &gui.param_setter(),
            &mut self.view,
            &mut self.text_entry,
            &mut self.presets,
            &mut self.nav,
        );
    }
}

/// Every paging item, each floor **computed from its card's tree** in `ui`'s fonts: the tree's
/// narrowest plus the card's chrome, each card exactly as wide as that floor (its ceiling is its
/// floor, `plans/plan-editor-standard.md` A1).
pub fn page_items(ui: &Ui, params: &ModalSynthParams) -> Vec<mxm_ui::paging::Item<'static>> {
    use mxm_ui::{flow::Card as FlowCard, paging::Item, paging::Key};
    CARDS
        .iter()
        .enumerate()
        .map(|(i, card)| {
            let floor = mxm_ui::tree::card_floor(ui, card.title(), &cards::card(ui, *card, params));
            Item {
                key: Key(i as u64),
                card: FlowCard::new(card.title(), floor).capped(floor),
                category: card.category(),
                kind: card.kind(),
            }
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
pub fn panel(
    ui: &mut Ui,
    params: &ModalSynthParams,
    telemetry: &Telemetry,
    setter: &ParamSetter<'_>,
    view: &mut usize,
    text_entry: &mut HashMap<&'static str, Option<String>>,
    presets: &mut PresetUi,
    nav: &mut mxm_ui::navigation::State,
) {
    // A frame every 50 ms while open: the meter wants it, and a developer request is taken when
    // `panel` runs (`docs/plugin-conventions.md`, *A developer channel in every editor*).
    ui.ctx()
        .request_repaint_after(std::time::Duration::from_millis(50));
    // One question, and both layers suspend on it: the paging renderer's `hold` and the cursor's
    // `inert` both ask whether another surface owns this frame's keyboard.
    let busy = presets.holds_the_keyboard() || text_entry.values().any(Option::is_some);
    mxm_ui::paging::editor::hold(ui.ctx(), busy);
    // CC 119: a category's first card, or the developer Parameters list (127).
    mxm_ui::paging::editor::developer_request(ui.ctx(), view, telemetry.dev.take_view());

    // **The keyboard cursor moves before anything is drawn**, from the registry and the card
    // rectangles the last frame built.
    if *view == mxm_ui::paging::PARAMETERS {
        // A surface with no cards: stopped, so its controls keep their own bare-arrow editing.
        mxm_ui::navigation::stop(ui.ctx());
    } else {
        mxm_ui::navigation::paged_with_bar(ui.ctx(), nav, busy, &[VOLUME_CARD]);
    }
    // CC 117: the preset browser.
    if let Some(open) = telemetry.dev.take_browser() {
        presets.set_browser_open(open);
    }
    // CC 118 asks for the expander, and this editor has none: every control is on a card, so the
    // request is taken and does nothing.
    let _ = telemetry.dev.take_disclosure();
    // CC 116: the theme, for this run only.
    mxm_ui::shell::developer_theme(ui.ctx(), &telemetry.dev);

    let tokens = tokens_for(ui);
    mxm_ui::AppBar::new(crate::NAME).show_with(
        ui,
        &tokens,
        |ui| mxm_preset::ui::preset_row(ui, &tokens, params, setter, presets),
        |ui| {
            mxm_ui::shell::peak_meter(ui, &tokens, &telemetry.meter);
            // Design system §3.1 slot 6: the instrument's output level beside its meter, not on a
            // card.
            mxm_ui::navigation::bar_card(ui, VOLUME_CARD, |ui| {
                ui.scope(|ui| {
                    cards::binding_for(BAR_PARAMETER, params)
                        .slider_inline(ui, &tokens, setter, text_entry, 96.0);
                })
                .response
                .rect
            });
            mxm_ui::shell::zoom_control(ui);
            mxm_ui::shell::editor_theme_control(ui);
        },
    );
    mxm_preset::ui::overlays(ui, &tokens, params, setter, presets);

    egui::CentralPanel::default()
        .frame(
            egui::Frame::new()
                .fill(tokens.canvas)
                .inner_margin(egui::Margin::same(SPACE_5 as i8)),
        )
        .show(ui, |ui| {
            if *view == mxm_ui::paging::PARAMETERS {
                parameters_view(ui, &tokens, params, setter, text_entry);
            } else {
                paged_view(ui, &tokens, params, telemetry, setter, text_entry);
            }
        });
}

fn paged_view(
    ui: &mut Ui,
    tokens: &Tokens,
    params: &ModalSynthParams,
    telemetry: &Telemetry,
    setter: &ParamSetter<'_>,
    entries: &mut HashMap<&'static str, Option<String>>,
) {
    let items = page_items(ui, params);
    let text_editing = entries.values().any(Option::is_some);
    let mut live = cards::Live {
        params,
        tempo: telemetry.tempo.get(),
        setter,
        entries,
    };
    mxm_ui::paging::editor::show(
        ui,
        tokens,
        &items,
        // No preferred groups: with none, all twelve cards stand on one page at the budget, the
        // LFOs reading on across the row break in their order.
        &[],
        text_editing,
        &mut |ui, index| cards::card(ui, CARDS[index], params),
        &mut |ui, _, leaf, rect| cards::paint(ui, tokens, leaf, rect, &mut live),
    );
}

/// **The developer Parameters list**: every parameter by its full name, the route slots' three
/// each included, as a host's automation list shows them. No tab reaches it; CC 119's 127 does.
fn parameters_view(
    ui: &mut Ui,
    tokens: &Tokens,
    params: &ModalSynthParams,
    setter: &ParamSetter<'_>,
    entries: &mut HashMap<&'static str, Option<String>>,
) {
    mxm_ui::shell::scroll_list(ui).show(ui, |ui| {
        let columns = if ui.available_width() >= 1050.0 {
            3
        } else if ui.available_width() >= 620.0 {
            2
        } else {
            1
        };
        // A flat list with no card to shorten a name: every slider paints its full name.
        let bound: Vec<_> = cards::all_bindings(params)
            .into_iter()
            .map(binding::Bound::unlabelled)
            .collect();
        let per_column = bound.len().div_ceil(columns);
        ui.columns(columns, |uis| {
            for (column, chunk) in uis.iter_mut().zip(bound.chunks(per_column)) {
                for parameter in chunk {
                    parameter.slider(column, tokens, setter, entries);
                }
            }
        });
    });
}

fn tokens_for(ui: &Ui) -> Tokens {
    if ui.visuals().dark_mode {
        mxm_ui::DARK
    } else {
        mxm_ui::LIGHT
    }
}

/// The paging items as the editor computes them for `params`, from a context set up as an
/// editor's is — three passes in, so the weighted font cuts are bound — for tests, which have no
/// editor `Ui` to hand.
#[cfg(test)]
pub(crate) fn items_in(params: &ModalSynthParams) -> Vec<mxm_ui::paging::Item<'static>> {
    let ctx = egui::Context::default();
    mxm_ui::typography::apply(&ctx);
    mxm_ui::theme::apply(&ctx);
    let mut items = Vec::new();
    for _ in 0..3 {
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            items = page_items(ui, params);
        });
        output.textures_delta.clear();
    }
    items
}

/// [`items_in`] at Init.
#[cfg(test)]
pub(crate) fn test_items() -> Vec<mxm_ui::paging::Item<'static>> {
    items_in(&ModalSynthParams::default())
}

/// The cards' floors in paging order, at Init.
#[cfg(test)]
pub(crate) fn test_floors() -> Vec<f32> {
    test_items().iter().map(|item| item.card.floor).collect()
}

/// A card's index in [`CARDS`].
#[cfg(test)]
pub(crate) fn index_of(card: cards::Card) -> usize {
    CARDS
        .iter()
        .position(|c| *c == card)
        .expect("every card is listed")
}

#[cfg(test)]
mod proof;
