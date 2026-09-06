use egui::{InnerResponse, Popup, Response, Ui};

use crate::theme_from_context;

/// Show a styled popup attached to a response, preserving egui's popup open/close behavior.
pub fn popover<R>(response: &Response, add: impl FnOnce(&mut Ui) -> R) -> Option<InnerResponse<R>> {
    let theme = theme_from_context(&response.ctx);
    Popup::menu(response)
        .frame(theme.frame(theme.palette.surface))
        .show(add)
}
