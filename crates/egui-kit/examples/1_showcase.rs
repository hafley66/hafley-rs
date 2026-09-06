//! Host-neutral showcase for the piecemeal `egui-kit` API.
//!
//! A host application can call [`render`] from its egui frame callback.  This example
//! deliberately owns no state, renderer, window, or runtime so it remains useful as a
//! compile-time composition reference for games and tools.

use egui_kit::{
    badge, button, card, checkbox, context_menu, danger_button, drawer, empty_state, error_state,
    field_label, grid, loading, modal, muted_label, panel, popover, scroll, section, slider, tabs,
    text_input, toast, toggle, toolbar, ui_provider, Theme, UiContext,
};

/// Render a small dashboard/settings/inventory screen using only kit primitives.
#[allow(dead_code)]
pub fn render(ctx: &egui::Context) {
    let theme = Theme::default();
    let mut selected_tab = 0usize;
    let mut notifications = true;
    let mut online = true;
    let mut volume = 0.75f32;
    let mut player_name = String::from("Ness");

    theme.install(ctx);
    egui::CentralPanel::default().show(ctx, |ui| {
        ui_provider(ui, UiContext::new(theme), |ui| {
            panel(ui, |ui| {
                toolbar(ui, |ui| {
                    ui.heading("Player hub");
                    ui.separator();
                    badge(ui, "ONLINE", theme.palette.accent);
                    ui.add_space(theme.spacing);
                    button(ui, "Sync");
                    danger_button(ui, "Sign out");
                });

                ui.add_space(theme.spacing);
                tabs(
                    ui,
                    &mut selected_tab,
                    &["Overview", "Settings", "Inventory"],
                );
                ui.separator();

                ui.columns(2, |columns| {
                    section(&mut columns[0], "Inventory", |ui| {
                        muted_label(ui, "Recent items");
                        scroll(ui, "inventory", 180.0, |ui| {
                            grid(ui, "inventory-grid", 3, |ui| {
                                field_label(ui, "Item");
                                field_label(ui, "Rarity");
                                field_label(ui, "Count");
                                ui.end_row();

                                for (item, rarity, count) in [
                                    ("PK Freeze", "Rare", "2"),
                                    ("Franklin Badge", "Uncommon", "1"),
                                    ("Saturn", "Legendary", "4"),
                                    ("Snack", "Common", "18"),
                                ] {
                                    ui.label(item);
                                    ui.label(rarity);
                                    ui.label(count);
                                    ui.end_row();
                                }
                            });
                        });
                        toolbar(ui, |ui| {
                            button(ui, "Equip selected");
                            button(ui, "Sort");
                        });
                    });

                    section(&mut columns[1], "Settings", |ui| {
                        card(ui, |ui| {
                            field_label(ui, "Profile");
                            text_input(ui, &mut player_name, "Display name");
                            checkbox(ui, &mut notifications, "Notifications");
                            toggle(ui, &mut online, "Appear online");
                            slider(ui, &mut volume, 0.0..=1.0, "Master volume");
                        });
                        ui.add_space(theme.spacing);
                        section(ui, "Quick actions", |ui| {
                            button(ui, "Manage controls");
                            button(ui, "Replay settings");
                        });
                    });
                });

                ui.add_space(theme.spacing);
                section(ui, "Composition states", |ui| {
                    loading(ui, "Refreshing inventory");
                    empty_state(ui, "No pending invites");
                    error_state(ui, "Preview unavailable");

                    let menu_button = button(ui, "Open actions");
                    popover(&menu_button, |ui| {
                        button(ui, "Duplicate");
                        danger_button(ui, "Delete");
                    });
                    context_menu(&menu_button, |ui| {
                        button(ui, "Copy link");
                    });
                });
            });

            // A real host would keep these flags in application state and set them from intent
            // handlers. False keeps this compile-only showcase unobtrusive by default.
            let dialog_open = false;
            modal(
                ui.ctx(),
                "showcase-dialog",
                "Confirm action",
                dialog_open,
                |ui| {
                    ui.label("The host decides what this action means.");
                },
            );
            drawer(ui.ctx(), "showcase-drawer", "Details", false, |ui| {
                ui.label("Drawer content is ordinary egui.");
            });
            if false {
                toast(ui.ctx(), "showcase-toast", "Saved");
            }
        });
    });
}

fn main() {}
