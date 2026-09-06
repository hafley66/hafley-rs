use std::ops::RangeInclusive;

use egui::{Button, Checkbox, Color32, Response, RichText, Slider, TextEdit, Ui};

use crate::current_theme;

pub fn button(ui: &mut Ui, label: impl Into<egui::WidgetText>) -> Response {
    let theme = current_theme(ui);
    ui.add(Button::new(label.into().color(theme.palette.ink)))
}

pub fn danger_button(ui: &mut Ui, label: impl Into<egui::WidgetText>) -> Response {
    let theme = current_theme(ui);
    ui.add(Button::new(label.into().color(theme.palette.danger)))
}

pub fn badge(ui: &mut Ui, text: impl Into<egui::WidgetText>, color: Color32) -> Response {
    ui.add(Button::new(text.into().strong().color(color)))
}

pub fn field_label(ui: &mut Ui, text: impl Into<egui::WidgetText>) -> Response {
    let theme = current_theme(ui);
    ui.label(text.into().color(theme.palette.ink))
}

pub fn muted_label(ui: &mut Ui, text: impl Into<egui::WidgetText>) -> Response {
    let theme = current_theme(ui);
    ui.label(text.into().color(theme.palette.muted))
}

/// Render a labeled checkbox and mutate `checked` when the user clicks it.
pub fn checkbox(ui: &mut Ui, checked: &mut bool, label: impl Into<egui::WidgetText>) -> Response {
    let theme = current_theme(ui);
    ui.add(Checkbox::new(
        checked,
        label.into().color(theme.palette.ink),
    ))
}

/// Render a compact selectable toggle. The returned response is the whole control.
pub fn toggle(ui: &mut Ui, on: &mut bool, label: impl Into<egui::WidgetText>) -> Response {
    let theme = current_theme(ui);
    let response = ui.selectable_label(
        *on,
        label.into().color(if *on {
            theme.palette.ink
        } else {
            theme.palette.muted
        }),
    );
    if response.clicked() {
        *on = !*on;
    }
    response
}

/// Render a floating-point slider with an optional label.
pub fn slider(
    ui: &mut Ui,
    value: &mut f32,
    range: RangeInclusive<f32>,
    label: impl Into<egui::WidgetText>,
) -> Response {
    ui.add(Slider::new(value, range).text(label.into()))
}

/// Render a single-line text field backed by the supplied `String`.
pub fn text_input(ui: &mut Ui, value: &mut String, hint: impl Into<egui::WidgetText>) -> Response {
    ui.add(TextEdit::singleline(value).hint_text(hint.into()))
}

/// Render a row of tabs and return the response for the selected tab, if any.
///
/// `selected` is clamped to the available tab range. An empty tab list simply returns `None`.
pub fn tabs(ui: &mut Ui, selected: &mut usize, labels: &[impl AsRef<str>]) -> Option<Response> {
    let theme = current_theme(ui);
    if labels.is_empty() {
        return None;
    }
    *selected = (*selected).min(labels.len() - 1);
    let mut selected_response = None;
    ui.horizontal(|ui| {
        for (index, label) in labels.iter().enumerate() {
            let response = ui.selectable_label(
                *selected == index,
                RichText::new(label.as_ref()).color(theme.palette.ink),
            );
            if response.clicked() {
                *selected = index;
            }
            if *selected == index {
                selected_response = Some(response);
            }
        }
    });
    selected_response
}
