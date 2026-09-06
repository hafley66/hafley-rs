//! Stateless composition patterns for transient and asynchronous UI.

#[path = "composition/1_context_menu.rs"]
mod context_menu;
#[path = "composition/4_drawer.rs"]
mod drawer;
#[path = "composition/3_modal.rs"]
mod modal;
#[path = "composition/2_popover.rs"]
mod popover;
#[path = "composition/0_states.rs"]
mod states;
#[path = "composition/5_toast.rs"]
mod toast;

pub use context_menu::context_menu;
pub use drawer::drawer;
pub use modal::modal;
pub use popover::popover;
pub use states::{empty_state, error_state, loading};
pub use toast::toast;
