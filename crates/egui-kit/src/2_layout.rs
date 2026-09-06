use std::hash::Hash;

use egui::{Grid, InnerResponse, ScrollArea, Ui};

use crate::{current_theme, Theme};

pub fn panel<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
    panel_with_theme(ui, current_theme(ui), add)
}

pub fn panel_with_theme<R>(
    ui: &mut Ui,
    theme: Theme,
    add: impl FnOnce(&mut Ui) -> R,
) -> InnerResponse<R> {
    theme.frame(theme.palette.surface).show(ui, add)
}

pub fn card<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
    card_with_theme(ui, current_theme(ui), add)
}

pub fn card_with_theme<R>(
    ui: &mut Ui,
    theme: Theme,
    add: impl FnOnce(&mut Ui) -> R,
) -> InnerResponse<R> {
    theme.frame(theme.palette.surface_alt).show(ui, add)
}

pub fn section<R>(
    ui: &mut Ui,
    title: impl Into<egui::WidgetText>,
    add: impl FnOnce(&mut Ui) -> R,
) -> InnerResponse<R> {
    section_with_theme(ui, current_theme(ui), title, add)
}

pub fn section_with_theme<R>(
    ui: &mut Ui,
    theme: Theme,
    title: impl Into<egui::WidgetText>,
    add: impl FnOnce(&mut Ui) -> R,
) -> InnerResponse<R> {
    let title = title.into();
    panel_with_theme(ui, theme, |ui| {
        ui.label(
            title
                .text_style(egui::TextStyle::Heading)
                .color(theme.palette.ink),
        );
        ui.add_space(theme.spacing * 0.5);
        add(ui)
    })
}

pub fn stack<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
    ui.vertical(add)
}

pub fn toolbar<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
    ui.horizontal(add)
}

pub fn scroll<R>(
    ui: &mut Ui,
    id: impl Hash,
    max_height: f32,
    add: impl FnOnce(&mut Ui) -> R,
) -> egui::scroll_area::ScrollAreaOutput<R> {
    ScrollArea::vertical()
        .id_salt(id)
        .max_height(max_height)
        .show(ui, add)
}

pub fn grid<R>(
    ui: &mut Ui,
    id: impl Hash,
    columns: usize,
    add: impl FnOnce(&mut Ui) -> R,
) -> InnerResponse<R> {
    Grid::new(id)
        .num_columns(columns)
        .striped(true)
        .show(ui, add)
}
