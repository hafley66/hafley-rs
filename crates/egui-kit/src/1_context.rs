//! Scoped, typed ambient values for an egui render subtree.
//!
//! The container is stored in egui's per-`Context` data map and is installed before a child
//! closure runs. It is therefore synchronous and nestable, without introducing a retained view
//! tree or a signal runtime.

use std::{any::TypeId, collections::HashMap, sync::Arc};

use egui::{Id, Ui};

use crate::Theme;

const CONTEXT_ID: &str = "egui-kit::ui-context";

/// Ambient values available to components in one render subtree.
#[derive(Clone, Default)]
pub struct UiContext {
    theme: Theme,
    values: HashMap<TypeId, Arc<dyn std::any::Any + Send + Sync>>,
}

impl UiContext {
    pub fn new(theme: Theme) -> Self {
        Self {
            theme,
            ..Self::default()
        }
    }

    pub fn theme(&self) -> Theme {
        self.theme
    }

    pub fn with_theme(mut self, theme: Theme) -> Self {
        self.theme = theme;
        self
    }

    /// Add an owned, typed dependency to this context.
    pub fn with<T: Clone + Send + Sync + 'static>(mut self, value: T) -> Self {
        self.values.insert(TypeId::of::<T>(), Arc::new(value));
        self
    }

    /// Read an owned, typed dependency from this context.
    pub fn get<T: Clone + Send + Sync + 'static>(&self) -> Option<Arc<T>> {
        self.values
            .get(&TypeId::of::<T>())
            .cloned()?
            .downcast::<T>()
            .ok()
    }
}

/// Install a context for the duration of `children`, restoring the parent context afterward.
pub fn provider<R>(ui: &mut Ui, context: UiContext, children: impl FnOnce(&mut Ui) -> R) -> R {
    let id = Id::new(CONTEXT_ID);
    let previous = ui.ctx().data(|data| data.get_temp::<UiContext>(id));
    ui.ctx().data_mut(|data| data.insert_temp(id, context));

    let result = children(ui);
    ui.ctx()
        .data_mut(|data| data.insert_temp(id, previous.unwrap_or_default()));
    result
}

/// Get the current context, falling back to the default theme and an empty dependency set.
pub fn current(ui: &Ui) -> UiContext {
    current_from_context(ui.ctx())
}

/// Get the current context from an egui context, for overlays that render without a `Ui`.
pub fn current_from_context(ctx: &egui::Context) -> UiContext {
    ctx.data(|data| data.get_temp::<UiContext>(Id::new(CONTEXT_ID)))
        .unwrap_or_default()
}

/// Convenience lookup for the current theme.
pub fn theme(ui: &Ui) -> Theme {
    current(ui).theme()
}

pub fn theme_from_context(ctx: &egui::Context) -> Theme {
    current_from_context(ctx).theme()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn providers_are_nested_and_restore_the_parent() {
        egui::__run_test_ui(|ui| {
            let outer = Theme {
                spacing: 4.0,
                ..Theme::default()
            };
            let inner = Theme {
                spacing: 12.0,
                ..Theme::default()
            };
            let context = UiContext::new(outer).with("intent-sink");

            assert_eq!(theme(ui).spacing, Theme::default().spacing);
            provider(ui, context, |ui| {
                assert_eq!(theme(ui).spacing, 4.0);
                assert_eq!(
                    current(ui).get::<&str>().map(|value| *value),
                    Some("intent-sink")
                );
                provider(ui, UiContext::new(inner), |ui| {
                    assert_eq!(theme(ui).spacing, 12.0);
                });
                assert_eq!(theme(ui).spacing, 4.0);
            });
            assert_eq!(theme(ui).spacing, Theme::default().spacing);
        });
    }
}
