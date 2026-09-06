use std::hash::Hash;

use egui::{Align2, Area, Id, Order};

use crate::{card_with_theme, theme_from_context};

/// Render a transient toast in the lower-right corner. The caller controls lifetime and ordering.
pub fn toast(ctx: &egui::Context, id: impl Hash, message: impl Into<String>) {
    let theme = theme_from_context(ctx);
    let message = message.into();
    Area::new(Id::new(id))
        .anchor(
            Align2::RIGHT_BOTTOM,
            egui::vec2(-theme.spacing, -theme.spacing),
        )
        .order(Order::Foreground)
        .movable(false)
        .show(ctx, |ui| {
            card_with_theme(ui, theme, |ui| {
                ui.label(egui::RichText::new(message).color(theme.palette.ink));
            });
        });
}
