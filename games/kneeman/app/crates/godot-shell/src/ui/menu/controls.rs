use super::Screen;
use super::router::{Intent, MenuCtx};
use crate::controls::P1_MANUAL;
use crate::ui::themes::Theme;
use egui_rsx_macro::egui_rsx;

/// The game manual (display only -- no rebinding). `Nav::default`'s `last_page` lands new players
/// here, so it is the first thing a fresh pause shows: player 1's keyboard + gamepad bindings, read
/// from `controls::P1_MANUAL` (that module owns every raw device read; this screen only renders the
/// static table it exports).
pub struct Controls;

// P2 (couch co-op) is a second gamepad only: the keyboard now belongs entirely to P1 (WASD move +
// arrow-key c-stick), so the old left-hand P2 cluster is gone. See `controls::mod::poll_p2`.
const P2: &[(&str, &str)] = &[
    ("L-stick / D-pad", "move"),
    ("A", "jump"),
    ("R1", "shorthop"),
    ("X / R2", "attack"),
    ("L1", "shield"),
    ("Y / Back", "grab"),
    ("B", "special"),
];

// Small monospace token for a key/button cell -- keeps bindings scannable at a glance instead of
// blending into the action prose next to them.
fn chip(text: &str) -> egui::RichText {
    egui::RichText::new(text).monospace().size(12.0)
}

impl Screen for Controls {
    fn view<T: Theme>(&self, ui: &mut egui::Ui, _theme: &T, _cx: &MenuCtx, _out: &mut Vec<Intent>) {
        egui_rsx! {
            stylesheet: "xp.css",
            div { class: "groupbox",
                label { class: "legend", "Controls" }
                {
                    // Two panels side by side (P1 table left, P2 mini-table right) instead of stacked,
                    // so the page's height is roughly the taller of the two lists rather than their sum.
                    // The egui::Grid + RichText styling here has no rsx equivalent; raw-expr escape
                    // hatch keeps the layout verbatim under a CSS-styled groupbox chrome.
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label(egui::RichText::new("Player 1").strong().size(13.0));
                            egui::Grid::new("p1_manual")
                                .striped(true)
                                .num_columns(3)
                                .spacing([10.0, 3.0])
                                .show(ui, |ui| {
                                    for h in ["Action", "Key", "Pad"] {
                                        ui.label(egui::RichText::new(h).strong().weak().size(11.0));
                                    }
                                    ui.end_row();

                                    for row in P1_MANUAL {
                                        ui.label(egui::RichText::new(row.action).size(12.0));
                                        ui.label(chip(row.keyboard));
                                        ui.label(chip(row.gamepad));
                                        ui.end_row();
                                    }
                                });
                            ui.label(
                                egui::RichText::new("* or flick the stick/D-pad up")
                                    .weak()
                                    .size(10.0),
                            );
                        });

                        ui.add_space(12.0);
                        ui.separator();
                        ui.add_space(12.0);

                        ui.vertical(|ui| {
                            ui.label(
                                egui::RichText::new("Player 2 (2nd gamepad)")
                                    .strong()
                                    .size(13.0),
                            );
                            egui::Grid::new("p2_manual")
                                .striped(true)
                                .num_columns(2)
                                .spacing([8.0, 3.0])
                                .show(ui, |ui| {
                                    for h in ["Button", "Action"] {
                                        ui.label(egui::RichText::new(h).strong().weak().size(11.0));
                                    }
                                    ui.end_row();

                                    for (key, action) in P2 {
                                        ui.label(chip(key));
                                        ui.label(egui::RichText::new(*action).size(12.0));
                                        ui.end_row();
                                    }
                                });
                        });
                    });
                }
            }
        }
    }
}
