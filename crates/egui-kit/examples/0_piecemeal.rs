//! Compile-only smoke example for composing the kit a piece at a time.
//!
//! This intentionally has no app framework dependency: any egui host can call `show`.

use egui_kit::{button, checkbox, grid, panel, scroll, section, slider, tabs, text_input, toggle};

#[allow(dead_code)]
fn show_dashboard(ctx: &egui::Context) {
    let mut enabled = true;
    let mut volume = 0.75;
    let mut username = String::from("player");
    let mut selected_tab = 0;
    let tab_labels = ["Overview", "Settings"];
    egui::CentralPanel::default().show(ctx, |ui| {
        panel(ui, |ui| {
            section(ui, "Inventory", |ui| {
                scroll(ui, "inventory-scroll", 180.0, |ui| {
                    grid(ui, "inventory-grid", 2, |ui| {
                        ui.label("Item");
                        ui.label("Count");
                        ui.end_row();
                        ui.label("Coins");
                        ui.label("12");
                        ui.end_row();
                    });
                });
            });
            button(ui, "Equip");
            checkbox(ui, &mut enabled, "Enable notifications");
            toggle(ui, &mut enabled, "Online");
            slider(ui, &mut volume, 0.0..=1.0, "Volume");
            text_input(ui, &mut username, "Username");
            tabs(ui, &mut selected_tab, &tab_labels);
        });
    });
}

fn main() {}
