//! The XP-themed menu: a memory-router state machine ([`router`]) drawn through a [`Theme`]. Each
//! base [`Route`] maps to a [`Screen`] (a ZST unit, dispatched by `match` -> monomorphized, no dyn).
//! Screens are read-only over [`MenuCtx`] and push [`Intent`]s; the router applies them after the frame.

pub mod router;

mod background;
mod characters;
mod controls;
mod feel;
mod items;
mod network;
mod rules;

use crate::sim;
use crate::ui::themes::Theme;
use router::{Dialog, DialogState, Intent, MenuCells, MenuCtx, Route, Router};

/// One entry per page: shared by the roomy left rail and `menu_root_page` (the compact layout's
/// root "Menu" page), so the two nav surfaces can never drift out of sync with each other.
const NAV: &[(Route, &str)] = &[
    (Route::Items, "Items"),
    (Route::Charss, "Characters"),
    (Route::Rules, "Rules"),
    (Route::Background, "Background"),
    (Route::Feel, "Feel"),
    (Route::Controls, "Controls"),
    (Route::Network, "Network"),
];

/// Below this body width, drop the left rail for a breadcrumb: `132` (rail) + `248` (body
/// `set_min_width`) + ~6px frame inner margin + ~16px inter-column spacing (two default 8px
/// gaps either side of the separator) totals ~410px of unavoidable roomy-mode width, so anything
/// tighter than this is already clipping the rail or squeezing the body under its own floor.
/// The threshold sits ~50px above that floor for breathing room rather than an exact hair-trigger.
const COMPACT_WIDTH: f32 = 460.0;
/// Below this body height, prefer compact layout too: the roomy chrome alone (top bar row ~30px +
/// separator + the 7-entry-plus-Debug-plus-Resume rail at ~22px/row) already runs ~230-260px
/// before a single page widget is drawn, and a page needs real room beyond that to read as
/// comfortable rather than "technically scrolls". 560px leaves ~300px for page content.
const COMPACT_HEIGHT: f32 = 560.0;

/// A page body. Generic over the theme so screens draw their buttons through it (still monomorphized).
pub trait Screen {
    fn view<T: Theme>(&self, ui: &mut egui::Ui, theme: &T, cx: &MenuCtx, out: &mut Vec<Intent>);
}

/// Draw the whole menu for the frame and collect intents into `out`. No-op while the base is Closed.
pub fn menu<T: Theme>(
    ctx: &egui::Context,
    theme: &T,
    router: &mut Router,
    cells: &MenuCells,
    lobbies: &[crate::net::LobbyRow],
    push_status: &str,
    local_handle: usize,
    kbd_esc: bool,
    out: &mut Vec<Intent>,
) {
    let loc = router.location();
    if loc.base != Route::Controls { crate::controls::bindings::cancel(); }
    if matches!(loc.base, Route::Closed) {
        return;
    }

    // Swallow egui's Escape (before any widget) so its built-in focus-release-on-Escape can't fire
    // while a nav item is focused -- and emit the dismiss intent HERE. The bridge's `_input` marks
    // key events handled whenever egui wants keyboard (surface.rs), and the focus bootstrap below
    // keeps a rail entry focused, so the shell's Godot-side Esc handler never sees Escape while the
    // menu is up; this consume is the only place the press is observable. `kbd_esc` is false on the
    // frame the shell just resolved a menu_esc (the press that OPENED the menu is still in egui's
    // queue that frame; emitting for it would close the menu the instant it opened).
    let esc = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
    let cancelled_binding = crate::controls::bindings::take_cancelled();
    if esc && kbd_esc && !cancelled_binding {
        out.push(Intent::Esc);
    }

    // Modal backdrop: a full-screen scrim UNDER the window (Order::Background < the window's Middle)
    // so the paused game + HUD read as dimmed behind the menu.
    egui::Area::new(egui::Id::new("xp_menu_scrim"))
        .order(egui::Order::Background)
        .fixed_pos(egui::Pos2::ZERO)
        .show(ctx, |ui| {
            ui.painter().rect_filled(
                ctx.content_rect(),
                0.0,
                egui::Color32::from_black_alpha(150),
            );
        });

    let state = cells.state.get();
    let tune = cells.tune.get_cloned();
    let net = cells.net.get();
    let identity = cells.identity.get_cloned();
    let cx = MenuCtx {
        state: &state,
        tune: &tune,
        charsel: cells.charsel.get(),
        route: loc.base,
        net: &net,
        lobbies,
        push_status,
        identity: &identity,
        local_handle,
    };

    // Real (non-sentinel) history entries, paired with their true stack index so a breadcrumb
    // click can report `Intent::PopTo(idx)` against the same slice `Router::history` returns. The
    // very first entry is always the `Closed` base recorded before the menu was ever opened (see
    // `Nav::push`) -- it has no title and isn't a page, so it never gets a crumb.
    let real_crumbs: Vec<(usize, Route)> = router
        .history()
        .iter()
        .enumerate()
        .filter(|(_, r)| !matches!(r, Route::Closed))
        .map(|(i, &r)| (i, r))
        .collect();

    theme.window(ctx, title_of(loc.base), |ui, body_max| {
        let compact = body_max.x < COMPACT_WIDTH || body_max.y < COMPACT_HEIGHT;

        // Top bar: condensed netplay status + a Find/Leave shortcut + a jump to the lobby page on the
        // left; the tiny build stamp stays pinned right, with the world-size chip just to its left.
        // Present on every page so match state and the quick-match button are one glance away without
        // navigating to Network. In compact mode the pill and the button pair stack onto their own
        // rows instead of fighting for one wide row.
        ui.horizontal(|ui| {
            top_bar(ui, theme, &cx, out, compact);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
                ui.label(
                    egui::RichText::new(format!("build {}", env!("BUILD_HASH")))
                        .small()
                        .weak(),
                );
                world_size_chip(ui, &cx);
            });
        });

        // Breadcrumb: the only nav in compact mode (so it always renders there). In roomy mode the
        // rail IS the top-level nav, so the crumb strip appears only for pages nested BELOW a rail
        // entry (CharEdit): rail for the level, breadcrumb for the nesting, never both saying the
        // same thing. (The Esc-arm's Menu-root seeding means real_crumbs alone is never empty on a
        // reopen, so it can't be the roomy gate by itself -- that was the double-nav regression.)
        let nested = !matches!(loc.base, Route::Menu) && !NAV.iter().any(|&(r, _)| r == loc.base);
        if compact || (nested && !real_crumbs.is_empty()) {
            ui.separator();
            breadcrumb(ui, theme, &real_crumbs, loc.base, out, compact);
        }
        ui.separator();

        if compact {
            egui::ScrollArea::vertical()
                .id_salt("menu_body_scroll")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    render_base(loc.base, ui, theme, &cx, out);
                });
        } else {
            ui.horizontal_top(|ui| {
                // Roomy + the root page: menu_root_page's body IS the section list, so the rail
                // (the identical list) drops out -- the nav never renders itself twice.
                if !matches!(loc.base, Route::Menu) {
                    ui.vertical(|ui| {
                        ui.set_width(132.0);
                        rail(ui, theme, &cx, out);
                    });
                    ui.separator();
                }
                ui.vertical(|ui| {
                    ui.set_min_width(248.0);
                    egui::ScrollArea::vertical()
                        .id_salt("menu_body_scroll")
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            render_base(loc.base, ui, theme, &cx, out);
                        });
                });
            });
        }
    });

    if let Some(ds) = loc.dialog {
        dialog_layer(ctx, theme, ds, &cx, out);
    }
}

/// World-size debug chip, pinned next to the build stamp: wire bytes for the whole `SimState`
/// (`state_size_bytes`, same bincode config `net`'s checksum/resume snapshot uses) plus live
/// occupancy against the three fixed-cap arrays that keep `SimState` a `Copy` struct (items, ink
/// paths, the per-frame surf soup). Recomputed fresh every menu frame -- cheap, and this only runs
/// while the menu is open, never on the `step()` hot path. Soup occupancy folds in from `cx.state.paths`
/// alone (no new shell-side state plumbed through); if that stops being cheap, drop the term instead.
fn world_size_chip(ui: &mut egui::Ui, cx: &MenuCtx) {
    let kib = (sim::state_size_bytes(cx.state) + 512) / 1024; // round to nearest KiB
    let items = cx.state.items.iter().filter(|it| it.active()).count();
    let ink = cx.state.paths.iter().filter(|p| p.active()).count();
    let soup = sim::body::Soup::collect(&cx.state.paths, &cx.state.nodes, None)
        .surfs()
        .len();
    let text = format!(
        "{kib} KiB · items {items}/{} · ink {ink}/{} · soup {soup}/{}",
        sim::MAX_ITEMS,
        sim::MAX_DRAWN,
        sim::body::MAX_SURFS,
    );
    egui::Frame::NONE
        .fill(egui::Color32::from_black_alpha(40))
        .corner_radius(egui::CornerRadius::same(4))
        .inner_margin(egui::Margin::symmetric(6, 2))
        .show(ui, |ui| {
            ui.label(egui::RichText::new(text).small().weak());
        });
}

/// Condensed inline netplay status + shortcuts, shown on every page. A colored pill (fill = online/
/// offline, leading dot = live channel health) reads the same data as the Network page's chip, then a
/// Find/Leave quick-match button and a jump to the Network page for lobby selection. `out`-only, so it
/// routes through the same shell interception as the Network screen's buttons.
///
/// Roomy: pill + both buttons share one row (the original layout). Compact: the pill drops to its
/// own row and the buttons pair stacks onto the next, so this strip never forces the window wider
/// than the page body does -- a split pair rather than one ever-growing row.
fn top_bar<T: Theme>(
    ui: &mut egui::Ui,
    theme: &T,
    cx: &MenuCtx,
    out: &mut Vec<Intent>,
    compact: bool,
) {
    let net = cx.net;
    let offline = net.phase == "offline";
    // Dot = channel health at a glance: green once the data channel is live, amber while it negotiates.
    let dot = if offline {
        egui::Color32::from_rgb(120, 130, 145)
    } else if matches!(net.channel, "open" | "connected") {
        egui::Color32::from_rgb(90, 220, 130)
    } else {
        egui::Color32::from_rgb(235, 190, 70)
    };
    let fill = if offline {
        egui::Color32::from_rgb(60, 65, 80)
    } else {
        egui::Color32::from_rgb(40, 140, 80)
    };

    let pill = |ui: &mut egui::Ui| {
        egui::Frame::NONE
            .fill(fill)
            .corner_radius(egui::CornerRadius::same(6))
            .stroke(egui::Stroke::new(
                1.0_f32,
                egui::Color32::from_black_alpha(70),
            ))
            .inner_margin(egui::Margin::symmetric(8, 3))
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                let (rect, _) = ui.allocate_exact_size(egui::vec2(9.0, 9.0), egui::Sense::hover());
                ui.painter().circle_filled(rect.center(), 4.5, dot);
                // Text-only when offline (nothing to condense); online packs phase · channel · handle.
                let text = if offline {
                    "offline".to_string()
                } else {
                    format!("{}  ·  ch:{}  ·  h{}", net.phase, net.channel, net.handle)
                };
                ui.label(
                    egui::RichText::new(text)
                        .small()
                        .strong()
                        .color(egui::Color32::WHITE),
                );
            });
    };
    let buttons = |ui: &mut egui::Ui, out: &mut Vec<Intent>| {
        // Quick-match shortcut: Find when offline, Leave once in a match.
        if offline {
            if theme.button(ui, "⚔ Find match").clicked() {
                out.push(Intent::FindMatch);
            }
        } else if theme.button(ui, "Leave").clicked() {
            out.push(Intent::LeaveMatch);
        }
        // Jump to the Network page for lobby selection (hidden when already there).
        if !matches!(cx.route, Route::Network) && theme.button(ui, "🌐 Lobbies").clicked() {
            out.push(Intent::Nav(Route::Network));
        }
    };

    if compact {
        ui.vertical(|ui| {
            pill(ui);
            ui.horizontal(|ui| buttons(ui, out));
        });
    } else {
        ui.horizontal(|ui| {
            pill(ui);
            buttons(ui, out);
        });
    }
}

/// Left task-pane: one entry per page, the current one highlighted, plus a resume row.
fn rail<T: Theme>(ui: &mut egui::Ui, theme: &T, cx: &MenuCtx, out: &mut Vec<Intent>) {
    let mut first_resp: Option<egui::Response> = None;
    let mut sel_resp: Option<egui::Response> = None;
    for &(r, label) in NAV {
        let sel = same_page(cx.route, r);
        let resp = theme.nav_item(ui, label, sel);
        if first_resp.is_none() {
            first_resp = Some(resp.clone());
        }
        if sel {
            sel_resp = Some(resp.clone());
        }
        if resp.clicked() && !sel {
            out.push(Intent::Nav(r));
        }
    }
    // Debug opens the egui panel (intercepted by the shell) and closes the menu so the panel shows.
    if theme.nav_item(ui, "Debug", false).clicked() {
        out.push(Intent::OpenDebugPanel);
        out.push(Intent::Nav(Route::Closed));
    }
    ui.add_space(10.0);
    if theme.nav_item(ui, "▸ Resume game", false).clicked() {
        out.push(Intent::Nav(Route::Closed));
    }
    // Focus bootstrap: when nothing has focus (first open, or after body widgets disappeared on
    // a route change) give it to the current page's rail entry (falling back to the first), so a
    // reopened menu puts the cursor back where it was. Arrow keys need an existing focused widget
    // to move FROM; without this, directional nav is a no-op until something is manually focused.
    if ui.ctx().memory(|m| m.focused()).is_none() {
        if let Some(r) = sel_resp.or(first_resp) {
            r.request_focus();
        }
    }
}

/// Whether `nav`'s rail entry should look active given the current route (CharEdit -> Characters).
fn same_page(cur: Route, nav: Route) -> bool {
    cur == nav || matches!((cur, nav), (Route::CharEdit { .. }, Route::Charss))
}

/// Nav trail: "Menu › Characters › Edit P1". One clickable link per real history entry, then the
/// current page as plain (non-clickable) text. The root "Menu" link is nothing special here -- it
/// is just the first `real_crumbs` entry, because `Nav::reduce`'s `Esc`-from-`Closed` arm always
/// seeds `Route::Menu` into history right under whatever page a reopen lands on (see `router.rs`).
/// `real_crumbs` is `(true stack index, route)` so a click reports `Intent::PopTo(idx)` straight
/// against `Router::history`'s own indexing -- see `menu`'s `real_crumbs` computation, which
/// already dropped the un-titled `Route::Closed` sentinel.
///
/// `compact` gates the focus bootstrap only: in roomy mode the rail (drawn right below this) is
/// still the authoritative landing spot for arrow-key/gamepad focus (see its own bootstrap at the
/// end of `rail`), so this must not steal it there. In compact mode `rail` never runs, so the
/// first crumb link is the landing spot -- unless `real_crumbs` is empty, which only happens when
/// the current page IS the `Menu` root (see the module doc on `Route::Menu`), and that page
/// bootstraps its own first entry in `menu_root_page`.
fn breadcrumb<T: Theme>(
    ui: &mut egui::Ui,
    theme: &T,
    real_crumbs: &[(usize, Route)],
    current: Route,
    out: &mut Vec<Intent>,
    compact: bool,
) {
    ui.horizontal_wrapped(|ui| {
        let mut first_resp: Option<egui::Response> = None;
        for (i, &(idx, r)) in real_crumbs.iter().enumerate() {
            if i > 0 {
                ui.label("›");
            }
            let resp = theme.button(ui, title_of(r));
            if first_resp.is_none() {
                first_resp = Some(resp.clone());
            }
            if resp.clicked() {
                out.push(Intent::PopTo(idx));
            }
        }
        if !real_crumbs.is_empty() {
            ui.label("›");
        }
        ui.label(egui::RichText::new(title_of(current)).strong());

        if compact && !real_crumbs.is_empty() && ui.ctx().memory(|m| m.focused()).is_none() {
            if let Some(r) = first_resp {
                r.request_focus();
            }
        }
    });
}

/// The compact layout's root page: the section list itself, drawn as full-width entries (same
/// look as the roomy rail's rows) instead of the rail or a floating combo. Reached at the bottom
/// of the nav stack -- see the module doc on `Route::Menu` -- so it renders through `render_base`
/// exactly like any other page, no special-casing needed in `menu`'s compact branch.
fn menu_root_page<T: Theme>(ui: &mut egui::Ui, theme: &T, out: &mut Vec<Intent>) {
    let mut first_resp: Option<egui::Response> = None;
    for &(r, label) in NAV {
        let resp = theme.nav_item(ui, label, false);
        if first_resp.is_none() {
            first_resp = Some(resp.clone());
        }
        if resp.clicked() {
            out.push(Intent::Nav(r));
        }
    }
    // Focus bootstrap: this page stands in for the rail in compact mode, so it needs the same
    // "land on something" guarantee `rail`'s own bootstrap gives roomy mode -- otherwise arrow-key/
    // gamepad nav has nothing focused to move from the first time the root page is shown.
    if ui.ctx().memory(|m| m.focused()).is_none() {
        if let Some(r) = first_resp {
            r.request_focus();
        }
    }
}

fn render_base<T: Theme>(
    route: Route,
    ui: &mut egui::Ui,
    theme: &T,
    cx: &MenuCtx,
    out: &mut Vec<Intent>,
) {
    match route {
        Route::Menu => menu_root_page(ui, theme, out),
        Route::Items => items::Items.view(ui, theme, cx, out),
        Route::Charss => characters::Charss.view(ui, theme, cx, out),
        Route::CharEdit { .. } => characters::CharEdit.view(ui, theme, cx, out),
        Route::Rules => rules::Rules.view(ui, theme, cx, out),
        Route::Background => background::Background.view(ui, theme, cx, out),
        Route::Feel => feel::Feel.view(ui, theme, cx, out),
        Route::Controls => controls::Controls.view(ui, theme, cx, out),
        Route::Network => network::Network.view(ui, theme, cx, out),
        Route::Closed => {}
    }
}

fn title_of(route: Route) -> &'static str {
    match route {
        Route::Menu => "Menu",
        Route::Items => "Items",
        Route::Charss => "Characters",
        Route::CharEdit { .. } => "Edit Fighter",
        Route::Rules => "Rules",
        Route::Background => "Background",
        Route::Feel => "Feel",
        Route::Controls => "Controls",
        Route::Network => "Network",
        Route::Closed => "",
    }
}

/// A single-confirm modal: scrim + framed body with one Confirm and one Cancel.
fn dialog_layer<T: Theme>(
    ctx: &egui::Context,
    theme: &T,
    ds: DialogState,
    _cx: &MenuCtx,
    out: &mut Vec<Intent>,
) {
    theme.dialog(ctx, dialog_title(ds.kind), |ui| {
        ui.label(dialog_body(ds.kind));
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            if theme.button(ui, "Confirm").clicked() {
                out.push(Intent::DialogConfirm);
            }
            if theme.button(ui, "Cancel").clicked() {
                out.push(Intent::DialogCancel);
            }
        });
    });
}

fn dialog_title(d: Dialog) -> &'static str {
    match d {
        Dialog::ConfirmReset => "Reset feel?",
        Dialog::GifImport => "Import GIF",
    }
}

fn dialog_body(d: Dialog) -> String {
    match d {
        Dialog::ConfirmReset => "Restore all physics tuning to defaults.".to_string(),
        Dialog::GifImport => "Pick a GIF for the stage background.".to_string(),
    }
}
