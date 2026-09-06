use std::hash::Hash;

use egui::{Align2, Area, Id, InnerResponse, Order, Ui};

use crate::theme_from_context;

/// Show a right-anchored drawer while `open` is true. Closing remains the caller's responsibility.
pub fn drawer<R>(
    ctx: &egui::Context,
    id: impl Hash,
    title: impl Into<egui::WidgetText>,
    open: bool,
    add: impl FnOnce(&mut Ui) -> R,
) -> Option<InnerResponse<R>> {
    if !open {
        return None;
    }

    let theme = theme_from_context(ctx);
    let title = title.into();
    Some(
        Area::new(Id::new(id))
            .anchor(Align2::RIGHT_CENTER, egui::vec2(-theme.spacing, 0.0))
            .order(Order::Foreground)
            .movable(false)
            .show(ctx, |ui| {
                theme
                    .frame(theme.palette.surface)
                    .show(ui, |ui| {
                        ui.label(
                            title
                                .text_style(egui::TextStyle::Heading)
                                .color(theme.palette.ink),
                        );
                        ui.add_space(theme.spacing * 0.5);
                        add(ui)
                    })
                    .inner
            }),
    )
}
