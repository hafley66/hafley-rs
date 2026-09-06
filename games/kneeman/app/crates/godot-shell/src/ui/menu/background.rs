use super::Screen;
use super::router::{Dialog, Intent, MenuCtx};
use crate::ui::themes::Theme;
use egui_rsx_macro::egui_rsx;

/// Stage background page. The GIF library + import land here (see plans/gif-background-library.md).
pub struct Background;

impl Screen for Background {
    fn view<T: Theme>(&self, ui: &mut egui::Ui, theme: &T, _cx: &MenuCtx, out: &mut Vec<Intent>) {
        egui_rsx! {
            stylesheet: "xp.css",
            div { class: "groupbox",
                label { class: "legend", "Stage background" }
                label { "GIF library + import land here (see plans/gif-background-library.md)." }
                { ui.add_space(8.0); }
                {
                    // theme.button (not ui.button): XP styles ride a per-widget scope, since the
                    // XP theme refuses global egui Visuals (xp.rs:2). Raw ui.button would inherit
                    // egui's dark defaults -- the contrast bug.
                    if theme.button(ui, "Import GIF…").clicked() {
                        out.push(Intent::OpenDialog(Dialog::GifImport));
                    }
                }
            }
        }
    }
}
