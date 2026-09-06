//! Small reusable egui primitives.
//!
//! The kit owns no application state, router, renderer, signal runtime, Godot node, or global
//! singleton. Each helper is independently usable and takes an egui `Ui` plus a [`Theme`] token
//! set. Use one helper, several helpers, or none of them.

#[path = "4_composition.rs"]
mod composition;
#[path = "1_context.rs"]
mod context;
#[path = "2_layout.rs"]
mod layout;
#[path = "0_theme.rs"]
mod theme;
#[path = "3_widgets.rs"]
mod widgets;

pub use composition::{
    context_menu, drawer, empty_state, error_state, loading, modal, popover, toast,
};
pub use context::{
    current as ui_context, provider as ui_provider, theme as current_theme, theme_from_context,
    UiContext,
};
pub use layout::{
    card, card_with_theme, grid, panel, panel_with_theme, scroll, section, section_with_theme,
    stack, toolbar,
};
pub use theme::{Palette, Theme};
pub use widgets::{
    badge, button, checkbox, danger_button, field_label, muted_label, slider, tabs, text_input,
    toggle,
};
