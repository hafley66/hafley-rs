use super::Screen;
use super::router::{Intent, MenuCtx};
use crate::ui::themes::Theme;
use egui_rsx_macro::egui_rsx;

/// Feel / physics tuning page. Stub: the debug panel's sliders move here next.
pub struct Feel;

impl Screen for Feel {
    fn view<T: Theme>(&self, ui: &mut egui::Ui, _theme: &T, cx: &MenuCtx, _out: &mut Vec<Intent>) {
        let gravity = cx.tune.gravity;
        let air_speed = cx.tune.air_speed;
        egui_rsx! {
            stylesheet: "xp.css",
            div { class: "groupbox",
                label { class: "legend", "Feel / physics tuning" }
                label { "Mirrors the debug panel's sliders; lands here next." }
                { ui.add_space(6.0); }
                label { "gravity {gravity:.0}  ·  air speed {air_speed:.0}" }
            }
        }
    }
}
