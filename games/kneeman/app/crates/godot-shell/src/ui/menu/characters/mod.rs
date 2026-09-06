//! Characters page: per-slot roster cycle (writes the charsel cell) + an Edit route per fighter.
//! Will grow its own submodules (skins, attribute editors) -- hence the folder.

use super::Screen;
use super::router::{Intent, MenuCtx, Route};
use crate::identity::Identity;
use crate::roster::roster_names;
use crate::ui::themes::Theme;
use egui_rsx_macro::egui_rsx;

pub struct Charss;

impl Screen for Charss {
    fn view<T: Theme>(&self, ui: &mut egui::Ui, theme: &T, cx: &MenuCtx, out: &mut Vec<Intent>) {
        let names = roster_names();
        let n = names.len().max(1) as i64;
        egui_rsx! {
            stylesheet: "xp.css",
            div { class: "groupbox",
                label { class: "legend", "Characters" }
                label { "Pick each fighter." }
                { ui.hyperlink_to("Make a fighter from photos", "https://hafley.codes/game3/poses/index.html"); }
                { ui.add_space(6.0); }
                {
                    // Grid + multiple click handlers: closure-capture reborrow limit + Grid's
                    // geometric focus nav (arrow keys) -- raw-expr escape both. theme.button
                    // keeps the XP per-widget styling.
                    egui::Grid::new("charss_slots")
                        .num_columns(5)
                        .spacing([8.0, 6.0])
                        .show(ui, |ui| {
                            for slot in 0..2usize {
                                let cur = cx.charsel[slot].rem_euclid(n);
                                let name = names.get(cur as usize).map(String::as_str).unwrap_or("?");
                                ui.label(format!("P{}", slot + 1));
                                if theme.button(ui, "◀").clicked() {
                                    out.push(Intent::SetChar { slot, idx: (cur - 1).rem_euclid(n) });
                                }
                                ui.label(egui::RichText::new(name).strong());
                                if theme.button(ui, "▶").clicked() {
                                    out.push(Intent::SetChar { slot, idx: (cur + 1).rem_euclid(n) });
                                }
                                if theme.button(ui, "Edit").clicked() {
                                    out.push(Intent::Nav(Route::CharEdit { slot: slot as u8 }));
                                }
                                ui.end_row();
                            }
                        });
                }
            }
        }
    }
}

pub struct CharEdit;

impl Screen for CharEdit {
    fn view<T: Theme>(&self, ui: &mut egui::Ui, theme: &T, cx: &MenuCtx, out: &mut Vec<Intent>) {
        let slot = match cx.route {
            Route::CharEdit { slot } => slot as usize,
            _ => 0,
        };
        let slot_label = format!("P{}", slot + 1);

        // Which charsel entry this page edits: offline hotseat maps slot -> charsel[slot] directly;
        // in a net match charsel[0] is always the local pick (the remote slot's pick rides the
        // handshake and isn't editable here).
        let sel = if cx.net.phase == "offline" {
            (slot < 2).then_some(slot)
        } else {
            (slot == cx.local_handle).then_some(0)
        };

        egui_rsx! {
            stylesheet: "xp.css",
            div { class: "groupbox",
                label { class: "legend", "Edit Fighter -- {slot_label}" }
                {
                    // Local slot only: name + color + clear + fighter cycle + back, all with
                    // multiple sequential click handlers. Raw-expr escape for the lot.
                    if slot == cx.local_handle {
                        ui.horizontal(|ui| {
                            ui.label("Name");
                            let mut name = cx.identity.name.clone();
                            let resp = ui.add(
                                egui::TextEdit::singleline(&mut name)
                                    .char_limit(16)
                                    .desired_width(160.0),
                            );
                            if resp.changed() {
                                out.push(Intent::SetName(name));
                            }
                        });
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            ui.label("Color");
                            let c = cx.identity.color;
                            let mut rgb = [c.r, c.g, c.b];
                            if ui.color_edit_button_rgb(&mut rgb).changed() {
                                out.push(Intent::SetColor(rgb));
                            }
                        });
                        ui.add_space(6.0);
                        if theme.button(ui, "Clear").clicked() {
                            let d = Identity::default();
                            out.push(Intent::SetName(d.name));
                            out.push(Intent::SetColor([d.color.r, d.color.g, d.color.b]));
                            if let Some(slot) = sel {
                                out.push(Intent::SetChar { slot, idx: default_char_idx(slot) });
                            }
                        }
                        ui.add_space(6.0);
                    }

                    if let Some(slot) = sel {
                        let names = roster_names();
                        let n = names.len().max(1) as i64;
                        let cur = cx.charsel[slot].rem_euclid(n);
                        let name = names.get(cur as usize).map(String::as_str).unwrap_or("?");
                        ui.horizontal(|ui| {
                            ui.label("Fighter");
                            if theme.button(ui, "◀").clicked() {
                                out.push(Intent::SetChar { slot, idx: (cur - 1).rem_euclid(n) });
                            }
                            ui.label(egui::RichText::new(name).strong());
                            if theme.button(ui, "▶").clicked() {
                                out.push(Intent::SetChar { slot, idx: (cur + 1).rem_euclid(n) });
                            }
                        });
                        ui.add_space(8.0);
                    }

                    if theme.button(ui, "Back").clicked() {
                        out.push(Intent::Back);
                    }
                }
            }
        }
    }
}

/// Default roster pick per hotseat slot, restored by the Clear button -- mirrors
/// `identity::load_charsel`'s own fallback (`[0, 1]`) so clearing lands on the same fighters a
/// fresh, never-saved profile would.
fn default_char_idx(slot: usize) -> i64 {
    if slot == 1 { 1 } else { 0 }
}
