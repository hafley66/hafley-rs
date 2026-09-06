//! Items page. Lists the spawnable roster from `core`'s `MENU_ITEMS` and offers a direct spawn plus
//! a two-player confirm-dialog spawn. Will grow its own submodules (drop tables, per-item config).

use super::Screen;
use super::router::{Intent, MenuCtx};
use crate::sim::MENU_ITEMS;
use crate::ui::themes::{Theme, xp::*};
use egui::{Align, Button, Color32, Layout, RichText, Stroke};
use egui_rsx_macro::egui_rsx;

pub struct Items;

impl Screen for Items {
    fn view<T: Theme>(&self, ui: &mut egui::Ui, theme: &T, cx: &MenuCtx, out: &mut Vec<Intent>) {
        let on_stage = cx.state.items.iter().filter(|i| i.active()).count();
        egui_rsx! {
            stylesheet: "xp.css",
            div { class: "groupbox",
                label { class: "legend", "Spawn items onto the stage. ({on_stage} live)" }
                {
                    // Each roster entry is a FULL-WIDTH clickable row: the entire row is the click
                    // target, not just a "Spawn" button on the side. xp_button_row wraps egui::Button
                    // in a ui.scope with the XP visuals (FACE beige, FACE_HI hover, BEVEL_LO border)
                    // so each row reads as a bordered list entry that highlights on hover and presses
                    // on click. Click anywhere -> SpawnItem intent fires.
                    let _ = ui.available_width(); // touch so the layout closure sees the right width
                    for card in MENU_ITEMS {
                        let body = format!("{}  ·  {}", card.name, card.blurb);
                        if xp_button_row(ui, &body).clicked() {
                            out.push(Intent::SpawnItem(card.kind, card.tool, card.stroke));
                        }
                    }
                    ui.add_space(8.0);
                    if theme.button(ui, "Clear field").clicked() {
                        out.push(Intent::ClearItems);
                    }
                }
            }
        }
    }
}

/// One full-width XP-styled button used as a list row. Wraps in `ui.scope` so the XP visuals apply
/// only to this widget (the global dark visuals the debug panel uses are untouched -- see
/// `themes/xp.rs:2`). Reuses the same `FACE`/`FACE_HI`/`FACE_DN`/`BEVEL_LO` palette as the
/// `theme.button` helper but overrides min_size to span the available width, so the entire row is
/// the click target rather than a small button hugged to its label.
fn xp_button_row(ui: &mut egui::Ui, body: &str) -> egui::Response {
    ui.scope(|ui| {
        let v = ui.visuals_mut();
        let r = egui::CornerRadius::same(2);
        v.widgets.inactive.weak_bg_fill = FACE;
        v.widgets.inactive.bg_fill = FACE;
        v.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, INK);
        v.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, BEVEL_LO);
        v.widgets.inactive.corner_radius = r;
        v.widgets.hovered.weak_bg_fill = FACE_HI;
        v.widgets.hovered.bg_fill = FACE_HI;
        v.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, INK);
        v.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, TITLE_BOT);
        v.widgets.hovered.corner_radius = r;
        v.widgets.active.weak_bg_fill = FACE_DN;
        v.widgets.active.bg_fill = FACE_DN;
        v.widgets.active.fg_stroke = Stroke::new(1.0_f32, INK);
        v.widgets.active.bg_stroke = Stroke::new(1.0_f32, TITLE_BOT);
        v.widgets.active.corner_radius = r;
        // Full width, fixed height: the row IS the click target. available_width inside the scope
        // reflects the parent groupbox body width (minus its 8px padding).
        let w = ui.available_width();
        ui.add_sized([w, 28.0], Button::new(RichText::new(body).color(INK)))
    })
    .inner
}

// Silence the unused-import warning for the layout/align imports we keep around for the next
// iteration (multiline rows with name bold + blurb small, sized via allocate_ui_with_layout).
#[allow(dead_code)]
fn _keep_imports(_a: Align, _l: Layout, _c: Color32) {}
