//! The menu as a memory-router state machine, split the way the sim is: a PURE reducer over nav
//! state ([`Nav::reduce`], no I/O, no cells, no egui) plus an impure effect layer ([`Router`]) that
//! interprets app [`Intent`]s against the game cells. Screens never write cells -- they push Intents
//! into a Vec, drained AFTER the egui frame (egui can't poke Godot mid-draw), so the route has one writer.
//!
//! Footnote: the [`Nav`] half wants to be its own crate. It is a generic string state machine of
//! positional inputs (routes = the path) and unpositional inputs (dialogs = query-param overlays);
//! parameterized over the app's `Route`/`Dialog` types it is a reusable router with no game ties.
//! Reducer-pure like `smash_core::step`, so it would be trivially testable + rollback-friendly. Kept
//! concrete + in-tree for now; extract once a second consumer wants it. See plans/router-as-crate.md.

use futures_signals::signal::Mutable;

use crate::identity::Identity;
use crate::net::NetDebug;
use crate::sim::{self, ItemKind, SimState, StrokeId, ToolKind, Tune};

// ============================ pure nav reducer (extractable crate) ============================

/// The base page. `CharEdit` carries its slot the way a URL path carries a positional param.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Route {
    Closed, // in game; the menu is not shown
    /// The nav root: a real page (not a synthesized combo/popover) listing every section as
    /// full-width entries -- the compact layout's "main menu". Always seeded into the history
    /// right under whatever page a reopen lands on (see `Nav::reduce`'s `Esc`-from-`Closed` arm),
    /// so it is reachable by backing out (`Esc`/`Back`) or by clicking the root breadcrumb link,
    /// same as any other page.
    Menu,
    Items,
    Charss, // character select (never "css")
    CharEdit {
        slot: u8,
    },
    Rules,
    Background,
    Feel,
    Controls,
    Network,
}

/// A modal laid over the base (reachable from many bases). Single confirm: one button fires it, one
/// cancels. No per-player gate — a dialog either fires or it doesn't.
#[derive(Clone, Copy, PartialEq)]
pub enum Dialog {
    ConfirmReset,
    GifImport,
}

#[derive(Clone, Copy, PartialEq)]
pub struct DialogState {
    pub kind: Dialog,
}

/// Where the menu is: a base page plus an optional dialog on top. Copy + small, so it is cheap to
/// snapshot and (later) to serialize for rollback tests.
#[derive(Clone, Copy, PartialEq)]
pub struct Location {
    pub base: Route,
    pub dialog: Option<DialogState>,
}

/// One nav transition. The only inputs the pure reducer understands. App effects (spawn an item,
/// reset tune) are NOT here -- they live in [`Intent`] and run in the effect layer.
pub enum NavCmd {
    Esc, // context-sensitive: cancel dialog, else close the menu, else open it (to the last page)
    Push(Route),
    Back,
    /// Pop straight to a given index in the history stack (a breadcrumb click): the route at
    /// `history[idx]` becomes the new base, and the stack is truncated to `history[0..idx]`, same
    /// as pressing `Back` `history.len() - idx` times but in one reducer step. A no-op if `idx` is
    /// out of range (defensive; the UI never emits one -- the current page's crumb isn't clickable).
    PopTo(usize),
    OpenDialog(Dialog),
    Confirm, // fire the open dialog
    CancelDialog,
}

/// What the reducer hands back: nothing, or "this dialog passed its gate -- caller, run its effect".
/// Routing stays pure; the dialog's actual consequence is the caller's to interpret.
#[derive(Clone, Copy, PartialEq)]
pub enum NavOut {
    None,
    Fire(Dialog),
}

/// The pure nav state: current location, a history stack for Back, and the dialog gate setting.
/// `reduce` is a total function of (state, cmd) -> state with no side effects.
#[derive(Clone)]
pub struct Nav {
    loc: Location,
    history: Vec<Route>,
    last_page: Route, // page shown when the menu closed; reopening lands back on it
}

impl Default for Nav {
    fn default() -> Self {
        Self {
            loc: Location {
                base: Route::Closed,
                dialog: None,
            },
            history: Vec::new(),
            // A fresh pause (nobody has navigated yet) lands on the manual, not Items -- new players
            // need the control reference before they need the item spawner. Once a player visits any
            // other page, `close` makes it sticky (see `close` below) and this default no longer applies.
            last_page: Route::Controls,
        }
    }
}

impl Nav {
    pub fn location(&self) -> Location {
        self.loc
    }

    /// The history stack below the current base, oldest first (index 0 is the earliest page still
    /// on the stack). Read-only: a breadcrumb renders one crumb per entry and reports clicks back
    /// as [`NavCmd::PopTo`] indices into this same slice, so the two stay in lockstep by construction.
    pub fn history(&self) -> &[Route] {
        &self.history
    }

    /// Apply one nav command. Pure: mutates only `self`, returns any dialog that passed its gate.
    pub fn reduce(&mut self, cmd: NavCmd) -> NavOut {
        match cmd {
            NavCmd::Esc => {
                if self.loc.dialog.is_some() {
                    self.loc.dialog = None;
                } else if matches!(self.loc.base, Route::Closed) {
                    // In game -> reopen: seed the root `Menu` page under the landing page first,
                    // so there is always a real root to back out to (skip the extra push if
                    // `last_page` IS the root -- reopening should land there directly, not push
                    // a duplicate history entry on top of itself).
                    self.push(Route::Menu);
                    if !matches!(self.last_page, Route::Menu) {
                        self.push(self.last_page);
                    }
                } else {
                    self.back(); // in a menu -> pop one level; popping the root dismisses (see `back`)
                }
                NavOut::None
            }
            NavCmd::Push(r) => {
                self.push(r);
                NavOut::None
            }
            NavCmd::Back => {
                self.back();
                NavOut::None
            }
            NavCmd::PopTo(idx) => {
                self.pop_to(idx);
                NavOut::None
            }
            NavCmd::OpenDialog(d) => {
                self.loc.dialog = Some(DialogState { kind: d });
                NavOut::None
            }
            NavCmd::CancelDialog => {
                self.loc.dialog = None;
                NavOut::None
            }
            NavCmd::Confirm => self.confirm(),
        }
    }

    fn push(&mut self, r: Route) {
        if matches!(r, Route::Closed) {
            self.close(); // "Resume game" etc. route here; Closed is a reset, not a stack frame
            return;
        }
        self.history.push(self.loc.base);
        self.loc = Location {
            base: r,
            dialog: None,
        };
    }

    fn back(&mut self) {
        match self.history.pop() {
            Some(Route::Closed) | None => self.close(),
            Some(base) => self.loc = Location { base, dialog: None },
        }
    }

    /// Pop straight to `history[idx]`: it becomes the new base, the stack truncates to
    /// `history[0..idx]` (dropping `idx` itself along with everything after it, mirroring how
    /// `back` drops the popped entry), and any open dialog is dismissed. Out-of-range `idx` is a
    /// no-op rather than a panic -- see [`NavCmd::PopTo`].
    fn pop_to(&mut self, idx: usize) {
        let Some(&target) = self.history.get(idx) else {
            return;
        };
        self.history.truncate(idx);
        self.loc = Location {
            base: target,
            dialog: None,
        };
    }

    /// Dismiss the menu: remember the page for the next open, drop the history.
    fn close(&mut self) {
        if !matches!(self.loc.base, Route::Closed) {
            self.last_page = self.loc.base;
        }
        self.history.clear();
        self.loc = Location {
            base: Route::Closed,
            dialog: None,
        };
    }

    /// Confirm the open dialog: clear it and report it for firing.
    fn confirm(&mut self) -> NavOut {
        let Some(ds) = self.loc.dialog.as_ref() else {
            return NavOut::None;
        };
        let kind = ds.kind;
        self.loc.dialog = None;
        NavOut::Fire(kind)
    }
}

// ================================ impure effect layer (shell) =================================

/// A read-only snapshot the screens see: the game cells sampled once this frame, plus the current
/// route (for nav highlighting).
pub struct MenuCtx<'a> {
    pub state: &'a SimState,
    pub tune: &'a Tune,
    pub charsel: [i64; 2],
    pub route: Route,
    pub net: &'a NetDebug, // transport snapshot for the Network page
    pub lobbies: &'a [crate::net::LobbyRow], // shell-held lobby list for the Network page's grid
    pub push_status: &'a str, // web-push opt-in status ("pinging for '<room>'"), mirrored from JS
    pub identity: &'a Identity, // local player presentation (name/color) for the CharEdit page
    pub local_handle: usize, // which fighter slot is the local player (name edits apply to it only)
}

/// The writable cells the effect layer drains into. Borrowed from KneeMan for the frame.
pub struct MenuCells<'a> {
    pub state: &'a Mutable<SimState>,
    pub tune: &'a Mutable<Tune>,
    pub charsel: &'a Mutable<[i64; 2]>,
    pub net: &'a Mutable<NetDebug>,
    pub identity: &'a Mutable<Identity>,
}

/// What a screen asks for: nav edges (routed through the pure reducer) + app effects (run on cells).
pub enum Intent {
    Esc,
    Nav(Route),
    Back,
    /// A breadcrumb crumb click: pop straight to `history[idx]`. See [`NavCmd::PopTo`].
    PopTo(usize),
    OpenDialog(Dialog),
    DialogConfirm,
    DialogCancel,
    /// Spawn a menu-card item: kind + the pen loadout (tool, stroke registry row; guns ignore them).
    SpawnItem(ItemKind, ToolKind, StrokeId),
    ClearItems,
    SetChar {
        slot: usize,
        idx: i64,
    },
    /// Rules page: how the fighter blast zone is computed each frame. Writes the tune cell directly
    /// (match-global config, not per-slot like `SetChar`).
    SetZoneMode(sim::ZoneMode),
    /// Rename the local player. Writes the identity cell; KneeMan's `sync_identity` persists the
    /// change and refreshes the nametag. Cap applied here, trim/empty fallback at `sanitize_name`.
    SetName(String),
    /// Recolor the local player (rgb 0..1; the godot Color is built at apply). Same identity
    /// cell + persistence path as `SetName`.
    SetColor([f32; 3]),
    /// Signal to the shell (`DebugUi::process`) to open the egui debug panel. The pure Router
    /// treats this as a no-op; the shell intercepts and drains it before calling `Router::apply`.
    OpenDebugPanel,
    /// Network page actions. Like `OpenDebugPanel`, the pure Router no-ops these; the shell
    /// intercepts them before `Router::apply` and drives the KneeMan netplay methods.
    FindMatch,
    LeaveMatch,
    /// Versioned lobby browser (also shell-intercepted, Router no-ops). `OpenLobby` hosts a room for
    /// the current version; `JoinLobby` dials an existing one by its key. Wired to the mesh in P2.
    OpenLobby,
    JoinLobby(String),
    /// Web-push opt-in (shell-intercepted): the shell calls the JS bridge to run the subscribe flow.
    PushSubscribe,
    /// Shareable quick-match invite (shell-intercepted): the shell mints a code, hosts it, and
    /// copies the `?join=` link to the clipboard. See `KneeMan::invite`.
    CopyInviteLink,
}

/// Wraps the pure [`Nav`] and interprets [`Intent`]s: nav edges go through `Nav::reduce`, app edges
/// hit the game cells. A dialog that passes its gate fires its app effect here.
#[derive(Default)]
pub struct Router {
    nav: Nav,
}

impl Router {
    pub fn location(&self) -> Location {
        self.nav.location()
    }

    /// The nav history stack, for drawing a breadcrumb. See [`Nav::history`].
    pub fn history(&self) -> &[Route] {
        self.nav.history()
    }

    /// Drain a frame's intents, after the egui pass.
    pub fn apply(&mut self, intents: Vec<Intent>, cells: &MenuCells) {
        for it in intents {
            self.apply_one(it, cells);
        }
    }

    fn apply_one(&mut self, intent: Intent, cells: &MenuCells) {
        match intent {
            Intent::Esc => self.dispatch(NavCmd::Esc, cells),
            Intent::Nav(r) => self.dispatch(NavCmd::Push(r), cells),
            Intent::Back => self.dispatch(NavCmd::Back, cells),
            Intent::PopTo(idx) => self.dispatch(NavCmd::PopTo(idx), cells),
            Intent::OpenDialog(d) => self.dispatch(NavCmd::OpenDialog(d), cells),
            Intent::DialogCancel => self.dispatch(NavCmd::CancelDialog, cells),
            Intent::DialogConfirm => self.dispatch(NavCmd::Confirm, cells),
            Intent::SpawnItem(k, tool, stroke) => spawn_item(cells, k, tool, stroke),
            Intent::ClearItems => clear_items(cells),
            Intent::SetChar { slot, idx } => {
                if slot < 2 {
                    let mut c = cells.charsel.get();
                    c[slot] = idx;
                    cells.charsel.set(c);
                }
            }
            Intent::SetZoneMode(mode) => {
                let mut tv = cells.tune.get_cloned();
                tv.zone_mode = mode;
                cells.tune.set(tv);
            }
            Intent::SetName(name) => {
                let mut id = cells.identity.get_cloned();
                id.name = name.chars().take(16).collect();
                cells.identity.set(id);
            }
            Intent::SetColor([r, g, b]) => {
                let mut id = cells.identity.get_cloned();
                id.color = godot::builtin::Color::from_rgb(r, g, b);
                cells.identity.set(id);
            }
            // Intercepted by the shell before Router::apply is called; pure Router ignores them.
            Intent::OpenDebugPanel
            | Intent::FindMatch
            | Intent::LeaveMatch
            | Intent::OpenLobby
            | Intent::JoinLobby(_)
            | Intent::CopyInviteLink
            | Intent::PushSubscribe => {}
        }
    }

    /// Run a nav command through the pure reducer; if a dialog passed its gate, fire its effect.
    fn dispatch(&mut self, cmd: NavCmd, cells: &MenuCells) {
        if let NavOut::Fire(d) = self.nav.reduce(cmd) {
            self.fire_dialog(d, cells);
        }
    }

    fn fire_dialog(&mut self, kind: Dialog, cells: &MenuCells) {
        match kind {
            Dialog::ConfirmReset => cells.tune.set(Tune::default()),
            Dialog::GifImport => { /* wired with the gif-background feature */ }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Push a chain of routes onto a fresh `Nav` via the public reducer, base -> ... -> last.
    fn pushed(routes: &[Route]) -> Nav {
        let mut nav = Nav::default();
        for &r in routes {
            nav.reduce(NavCmd::Push(r));
        }
        nav
    }

    #[test]
    fn pop_to_truncates_history_and_lands_on_the_target_route() {
        let mut nav = pushed(&[Route::Controls, Route::Items, Route::Charss, Route::Rules]);
        // The first-ever push always records the pre-open `Closed` base, so history is one
        // longer than the number of real pages visited.
        assert_eq!(
            nav.history(),
            &[Route::Closed, Route::Controls, Route::Items, Route::Charss]
        );
        assert_eq!(nav.location().base, Route::Rules);

        nav.reduce(NavCmd::PopTo(2)); // the "Items" crumb
        assert_eq!(nav.location().base, Route::Items);
        assert_eq!(nav.history(), &[Route::Closed, Route::Controls]);
    }

    #[test]
    fn pop_to_index_zero_lands_on_the_closed_sentinel() {
        // idx 0 is always the `Closed` base recorded by the very first push (before any real page
        // was visited) -- a breadcrumb never renders a crumb for it (see `menu::breadcrumb`, which
        // filters `Route::Closed`), but the reducer itself treats it like any other history entry.
        let mut nav = pushed(&[Route::Items, Route::Charss]);
        nav.reduce(NavCmd::PopTo(0));
        assert_eq!(nav.location().base, Route::Closed);
        assert!(nav.history().is_empty());
    }

    #[test]
    fn pop_to_out_of_range_is_a_no_op() {
        let mut nav = pushed(&[Route::Items, Route::Charss]);
        let before_base = nav.location().base;
        let before_history = nav.history().to_vec();

        nav.reduce(NavCmd::PopTo(99));

        assert_eq!(nav.location().base, before_base);
        assert_eq!(nav.history(), before_history.as_slice());
    }

    #[test]
    fn pop_to_dismisses_any_open_dialog() {
        let mut nav = pushed(&[Route::Items, Route::Charss]);
        nav.reduce(NavCmd::OpenDialog(Dialog::ConfirmReset));
        assert!(nav.location().dialog.is_some());

        nav.reduce(NavCmd::PopTo(0));

        assert!(nav.location().dialog.is_none());
    }

    #[test]
    fn pop_to_matches_repeated_back_to_the_same_depth() {
        // PopTo(idx) should be equivalent to calling Back enough times to reach the same depth --
        // it's offered as a single-step alternative to that loop, not a different destination.
        let mut via_pop_to = pushed(&[Route::Items, Route::Charss, Route::Rules]);
        via_pop_to.reduce(NavCmd::PopTo(1));

        let mut via_back = pushed(&[Route::Items, Route::Charss, Route::Rules]);
        via_back.reduce(NavCmd::Back);
        via_back.reduce(NavCmd::Back);

        assert_eq!(via_pop_to.location().base, via_back.location().base);
        assert_eq!(via_pop_to.history(), via_back.history());
    }

    #[test]
    fn esc_from_closed_seeds_the_menu_root_under_the_landing_page() {
        // `Nav::default()`'s `last_page` is `Route::Controls` -- a fresh open should land there
        // directly (unchanged from before), but with `Route::Menu` seeded underneath it so the
        // root is reachable by backing out, not just skipped straight past.
        let mut nav = Nav::default();
        nav.reduce(NavCmd::Esc);
        assert_eq!(nav.location().base, Route::Controls);
        assert_eq!(nav.history(), &[Route::Closed, Route::Menu]);
    }

    #[test]
    fn esc_pops_one_level_at_a_time_down_to_the_menu_root_then_dismisses() {
        let mut nav = Nav::default();
        nav.reduce(NavCmd::Esc); // open: base Controls, history [Closed, Menu]
        nav.reduce(NavCmd::Push(Route::Charss)); // drill in one more page

        nav.reduce(NavCmd::Esc); // back one level, not a full dismiss
        assert_eq!(nav.location().base, Route::Controls);
        assert_eq!(nav.history(), &[Route::Closed, Route::Menu]);

        nav.reduce(NavCmd::Esc); // back one more level: lands on the root itself
        assert_eq!(nav.location().base, Route::Menu);
        assert_eq!(nav.history(), &[Route::Closed]);

        nav.reduce(NavCmd::Esc); // at the root -- Esc now dismisses (unchanged old behavior)
        assert_eq!(nav.location().base, Route::Closed);
        assert!(nav.history().is_empty());
    }

    #[test]
    fn esc_reopens_directly_on_the_menu_root_if_that_is_where_it_was_left() {
        let mut nav = Nav::default();
        nav.reduce(NavCmd::Esc);
        nav.reduce(NavCmd::Esc); // back to the root, history [Closed]
        nav.reduce(NavCmd::Esc); // dismiss from the root
        assert_eq!(nav.location().base, Route::Closed);

        nav.reduce(NavCmd::Esc); // reopen: last_page was Menu, so no duplicate push
        assert_eq!(nav.location().base, Route::Menu);
        assert_eq!(nav.history(), &[Route::Closed]);
    }

    #[test]
    fn esc_cancels_an_open_dialog_before_touching_navigation() {
        let mut nav = pushed(&[Route::Feel]);
        nav.reduce(NavCmd::OpenDialog(Dialog::ConfirmReset));

        nav.reduce(NavCmd::Esc);

        assert!(nav.location().dialog.is_none());
        assert_eq!(nav.location().base, Route::Feel); // unchanged -- Esc only cancelled the dialog
    }
}

fn spawn_item(cells: &MenuCells, kind: ItemKind, tool: ToolKind, stroke: StrokeId) {
    let mut s = cells.state.get();
    sim::spawn_kind(&mut s, kind, tool, stroke, &cells.tune.get_cloned());
    cells.state.set(s);
}

fn clear_items(cells: &MenuCells) {
    let mut s = cells.state.get();
    sim::clear_items(&mut s);
    cells.state.set(s);
}
