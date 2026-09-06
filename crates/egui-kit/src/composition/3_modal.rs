use std::hash::Hash;

use egui::{Color32, Id, Modal, Ui};

use crate::theme_from_context;

/// Show a centered modal while `open` is true. Returns `true` when the modal requests closing.
pub fn modal<R>(
    ctx: &egui::Context,
    id: impl Hash,
    title: impl Into<egui::WidgetText>,
    open: bool,
    add: impl FnOnce(&mut Ui) -> R,
) -> Option<bool> {
    if !open {
        return None;
    }

    let theme = theme_from_context(ctx);
    let title = title.into();
    let response = Modal::new(Id::new(id))
        .backdrop_color(Color32::from_black_alpha(150))
        .frame(theme.frame(theme.palette.surface))
        .show(ctx, |ui| {
            ui.label(
                title
                    .text_style(egui::TextStyle::Heading)
                    .color(theme.palette.ink),
            );
            ui.add_space(theme.spacing * 0.5);
            add(ui);
        });
    Some(response.should_close())
}
