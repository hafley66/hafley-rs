//! Synchronous `futures-signals` bindings for `egui-kit`.
//!
//! These helpers use `Mutable::lock_mut()` during the current egui frame. No async runtime,
//! subscription, or signal stream is required; reactive stream APIs remain available to the host
//! when it needs them elsewhere.

use egui::{Checkbox, Response, Slider, TextEdit, Ui};
use futures_signals::signal::Mutable;

/// Render a text input directly against a [`Mutable<String>`].
pub fn text_input(
    ui: &mut Ui,
    value: &Mutable<String>,
    hint: impl Into<egui::WidgetText>,
) -> Response {
    let mut value = value.lock_mut();
    ui.add(TextEdit::singleline(&mut *value).hint_text(hint.into()))
}

/// Render a checkbox directly against a [`Mutable<bool>`].
pub fn checkbox(
    ui: &mut Ui,
    value: &Mutable<bool>,
    label: impl Into<egui::WidgetText>,
) -> Response {
    let mut value = value.lock_mut();
    ui.add(Checkbox::new(&mut value, label))
}

/// Render a floating-point slider directly against a [`Mutable<f32>`].
pub fn slider(
    ui: &mut Ui,
    value: &Mutable<f32>,
    range: std::ops::RangeInclusive<f32>,
    label: impl Into<egui::WidgetText>,
) -> Response {
    let mut value = value.lock_mut();
    ui.add(Slider::new(&mut *value, range).text(label))
}
