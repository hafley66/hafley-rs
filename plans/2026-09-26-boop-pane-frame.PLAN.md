# pane frame in boop-harness: one frame per pane, one query per question (2026-09-26)

Inventory: `plans/2026-09-26-one-query-per-question.INVENTORY.md`.

## Projected fs

```
hafley-rs/crates/
  # NO new crates, no new packages. Two existing crates, one new file.
  boop-mux/src/lib.rs             # owns the pane model: Pane, Multiplexer trait, capture_pane, pane_snapshot
  boop-mux/src/_0_snapshot.rs     # TerminalSnapshot gains `window: PaneWindow` (scroll_position/pane_height);
                                  #   pane_snapshot takes a range -> replaces instant 1_squares.rs:145 capture_lines + 0_tmux.rs:57 pane_window
  boop-harness/src/pane.rs        # ONE new file: PlacedTurn, Confidence, PaneFrame; frame(); watch()
                                  #   session = live.rs:220; turns = turn_rows_ordered + reset_from; project = turnvis + harness anchors
  boop/src/screen.rs              # DELETED; `boop beep lane squares` prints boop_harness::pane::frame
  boop/src/cli/*                  # `lane squares` gains --watch (ndjson) instead of a new subcommand file

instant/src-tauri/src/
  1_squares.rs      1125 lines -> ~80: Tauri emit + pty dirty bit -> boop_harness::pane::watch
  0_boop.rs         drops turns_from, boop_turns_recent, locate_turns, boop_locate_turns, input_region
  0_harness_store.rs drops boop_mux_session
instant/src/
  1_agentSquares*.ts, 2_stripVisibility.ts, favorites.ts caches   DELETED (boop-xterm owns them)

hafley-rxjs/packages/boop-xterm/src/
  3_ports.ts        ports shrink to frame + frame$ (+ fork/context mutations)
  4_paneFrame.ts    new file (no new package): one Signal<PaneFrame> per pane
  2_turnLocate.ts, 1_turnMatching.ts, 6_turnVisibility.ts, 4_paneSession.ts   DELETED
  8*_ overlays      read props derived from the pane frame signal
```

## Types (one spelling, snake_case, every layer)

```rust
// boop-harness/src/pane.rs  (Turn = boop-store TurnRow, reused as-is)

#[allow(non_camel_case_types)]
pub enum Confidence { anchored, extended, pinned }          // today: Anchored / "anchored" / pinned
pub struct PlacedTurn { #[serde(flatten)] pub turn: TurnRow, pub id: String,
    pub buffer_start: usize, pub buffer_end: usize,           // today also bufferStart, viewport_start
    pub anchor_start: usize, pub anchor_end: usize, pub confidence: Confidence }
// boop-mux/src/_0_snapshot.rs
pub struct PaneWindow { pub top: i64, pub bottom: i64 }     // was turnstrip Viewport + Strip.window + instant PaneWindow
pub struct PaneFrame {
    pub pane: String, pub socket: Option<String>,
    pub session: Option<String>, pub harness: Option<String>, // was boop_mux_session
    pub at: i64,                                              // frame identity per pane, monotonic ms
    pub rows: usize, pub window: Option<PaneWindow>,
    pub turns: Vec<PlacedTurn>, pub pinned: Vec<PlacedTurn>,
    pub tags: BTreeMap<String, Vec<String>>, pub layout: Option<Layout>,
}
```

```ts
// boop-xterm/3_ports.ts — same field names as Rust
type PaneFrame = { pane: string; session: string | null; at: number; window: { top: number; bottom: number } | null; turns: PlacedTurn[]; ... }
type Ports = {
  frame: (pane: PaneRef) => Promise<PaneFrame>          // one-shot call
  frame$: (pane: PaneRef) => Observable<PaneFrame>      // push; cold, refcounted
  // fork / context-queue mutations stay as they are
}
```

## Bodies (pseudo)

```rust
// boop-harness/src/pane.rs
pub fn frame(ctx: &Ctx, pane: &PaneRef) -> Result<PaneFrame> {
    // snap    = ctx.mux.pane_snapshot(socket, pane, range)   boop-mux: rows + window + size
    // bound   = session_in_pane(ctx.registry, pane)      cached per pane pid
    // sync_session(bound.session)                        store write, only when dirty
    // turns   = turns_for(ctx.store, bound.session, reset_from(&snap.rows))
    // placed  = project(&snap.rows, &turns, bound.harness)
    // layout  = boop_turnstrip::layout(placed, window);  tags = tags_for_many(placed ids)
}
// boop-harness/src/pane.rs
pub fn watch(ctx, pane, dirty: Receiver<()>) -> impl Iterator<Item = PaneFrame> {
    // on dirty (coalesced <=250ms): f = frame(ctx, pane); if fingerprint(f) != last { yield f }
}
```

```ts
// boop-xterm/4_paneFrame.ts
// paneFrame(pane) = merge(from(ports.frame(pane)), ports.frame$(pane))
//   scan keep max `at`  -> Signal<PaneFrame | undefined>, read by path
// overlays: visible = frame.turns shifted by frame.window.top and xterm viewportY -> pure props
```

## Instance lifetimes

| instance | created | lives until | count |
|---|---|---|---|
| Rust watcher | first `frame$` subscriber for (socket, pane) | last unsubscribe, or pane dies | 1 per pane |
| session binding cache | first frame for the pane | pane pid changes | 1 per pane |
| TS pane-frame signal | tab mount | tab close; hidden tab keeps value | 1 per pane |
| overlay props | derived on read | never stored | 0 |

The watcher no longer depends on the strip toggle: the strip-off case gets frames too.

## Reads, writes, uniqueness (one tick)

1. pty output -> dirty bit (instant) -> watcher tick, coalesced <=250ms.
2. read: tmux pane geometry -> `window`.
3. read: tmux capture `-S top -E bottom` -> rows.
4. read: binding cache, else `session_in_pane_on_socket`.
5. write: `sync_session` transcript -> store (only on dirty).
6. read: `turn_rows_ordered` from reset boundary.
7. compute: project + layout + tags; fingerprint; drop if unchanged.
8. emit `pane-frame` (Tauri) / ndjson line (CLI).

Uniqueness: watcher key (socket, pane); frame key (pane, at), newest wins; turn key `id` = session:turn.

## API changes

| surface | removed | added / kept |
|---|---|---|
| boop CLI | `boop beep lane squares` own projection (screen.rs) | `lane squares` prints the frame; `--watch` streams ndjson |
| instant Tauri | `boop_turns_recent`, `boop_locate_turns`, `boop_mux_session`, `boop_sync_session`, `squares_watch`/`squares-update` | `pane_frame`, `pane_watch`/`pane_unwatch`, event `pane-frame`; `boop_turns` kept, backed by `turns_for` |
| boop-xterm ports | turns, recentTurns, capture, locate, sync, paneSession | `frame`, `frame$` |

## Open questions
- PaneFrame schema from TypeSpec (hafley-rxjs tsp-ui-names plan) or hand-written Rust + TS?
- `8f_forkRender.ts:52` uses raw capture rows: add `lines` to the frame, or keep `boop_mux_capture`?
- Recent-turns-across-harness (Q2) has no caller left once the TS scan is gone. Delete, or keep for favorites candidates?
- One lane or one per repo (hafley-rs, hafley-rxjs, instant)?
