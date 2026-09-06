use super::Screen;
use super::router::{Dialog, Intent, MenuCtx};
use crate::sim::ZoneMode;
use crate::ui::themes::Theme;
use egui_rsx_macro::egui_rsx;

/// Match rules. For now: the blast-zone mode plus a reset-feel confirm. Grows into items on/off,
/// spawn interval, knockback, i-frames (mirrors the debug panel's "rules" group).
pub struct Rules;

impl Screen for Rules {
    fn view<T: Theme>(&self, ui: &mut egui::Ui, theme: &T, cx: &MenuCtx, out: &mut Vec<Intent>) {
        let mode = cx.tune.zone_mode;
        egui_rsx! {
            stylesheet: "xp.css",
            div { class: "groupbox",
                label { class: "legend", "Match rules" }
                label { "Blast zone:" }
                {
                    // selectable_label isn't in the rsx grammar yet; raw-expr escape hatch.
                    // Each click dispatches an Intent; the shell applies it post-frame.
                    ui.horizontal(|ui| {
                        for (m, label) in [
                            (ZoneMode::Static, "Static"),
                            (ZoneMode::InkExtends, "Ink extends"),
                            (ZoneMode::Off, "Off"),
                        ] {
                            if ui.selectable_label(mode == m, label).clicked() && mode != m {
                                out.push(Intent::SetZoneMode(m));
                            }
                        }
                    });
                }
                { ui.add_space(8.0); }
                {
                    if theme.button(ui, "Reset feel…").clicked() {
                        out.push(Intent::OpenDialog(Dialog::ConfirmReset));
                    }
                }
            }
        }
    }
}
