use egui::{InnerResponse, Popup, Response, Ui};

use crate::theme_from_context;

/// Attach a styled context menu to any response.
pub fn context_menu(response: &Response, add: impl FnOnce(&mut Ui)) -> Option<InnerResponse<()>> {
    let theme = theme_from_context(&response.ctx);
    Popup::context_menu(response)
        .frame(theme.frame(theme.palette.surface))
        .show(add)
}
