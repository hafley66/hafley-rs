use egui::{Response, Ui};

use crate::current_theme;

/// Render a compact loading state with an optional label.
pub fn loading(ui: &mut Ui, label: impl Into<String>) -> Response {
    let theme = current_theme(ui);
    ui.horizontal(|ui| {
        ui.spinner();
        ui.label(egui::RichText::new(label.into()).color(theme.palette.muted));
    })
    .response
}

/// Render an error state using the theme's danger token.
pub fn error_state(ui: &mut Ui, message: impl Into<String>) -> Response {
    let theme = current_theme(ui);
    ui.label(egui::RichText::new(message.into()).color(theme.palette.danger))
}

/// Render an empty state using the theme's muted token.
pub fn empty_state(ui: &mut Ui, message: impl Into<String>) -> Response {
    let theme = current_theme(ui);
    ui.label(egui::RichText::new(message.into()).color(theme.palette.muted))
}
