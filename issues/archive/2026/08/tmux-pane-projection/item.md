---
created: 2026-08-20
updated: 2026-08-30
type: feature
status: obsolete
priority: high
labels: [domain-boop, component-tmux, intent-observability]
related: ['@terminal-snapshot-boundary']
closed: 2026-08-30
disposition_note: Superseded by the host-neutral TerminalSnapshot seam and parity scope in @terminal-snapshot-boundary.
disposition_reason: superseded
---

# Project tmux panes onto Boop turns for xterm overlays

## Description

## Description

Expose a reusable Rust projection engine that joins tmux pane state with Boop
turn rows. An xterm host supplies browser geometry and input events. Boop
supplies pane text, tmux viewport state, transcript turns, visible-turn
intersection, and structured Markdown regions.

Instant currently performs this join in TypeScript:

| concern | current site |
|---|---|
| shell out to `boop db turn list` | `instant/src-tauri/src/0_boop.rs` |
| normalize and intersect xterm rows with turns | `instant/src/0_terminalTurnVisibility.ts` |
| detect/project Mermaid, D2, table, and list regions | `instant/src/00_terminalTurnRegions.ts` |
| pane capture | `boop-mux::Multiplexer::capture_pane` |
| typed turn rows | `boop-store::Store::turn_rows(TurnQuery)` |

The first consumer is Instant. The crate API must remain usable by another
xterm host without importing Tauri, DOM types, or TypeScript.

## Boundary

Rust owns:

- tmux pane identity, dimensions, cursor, history size, copy-mode viewport,
  visible capture, and change generation
- read-only Boop store access and typed turn retrieval
- terminal-line normalization and physical/logical line representation
- turn-to-pane intersection and reverse lookup by pane row
- fenced Mermaid/D2 detection and Markdown table/list source regions
- source-region to pane-row projection, including a region larger than the
  visible pane and repeated source lines

The xterm host owns:

- PTY byte transport and terminal input
- native text selection
- mapping pane rows and columns to client pixels
- overlay DOM, renderer lifecycle, pointer routing, and CSS

## Type signatures

Foundational types belong in a numerically prefixed module below
`boop-mux/src/0_*.rs`; later source, store join, and stream modules follow in
dependency order.

```rust
pub struct PaneTarget {
    pub socket: Option<String>,
    pub pane: String,
}

pub struct PaneFacts {
    pub pane_id: String,
    pub session_name: String,
    pub width: u16,
    pub height: u16,
    pub history_size: u64,
    pub cursor_x: u16,
    pub cursor_y: u16,
    pub viewport_top: i64,
    pub pane_in_mode: bool,
}

pub struct PaneRow {
    pub absolute_row: i64,
    pub text: String,
    pub wrapped: bool,
}

pub struct PaneSnapshot {
    pub target: PaneTarget,
    pub generation: u64,
    pub facts: PaneFacts,
    pub rows: Vec<PaneRow>,
}

pub trait PaneSnapshotSource: Send + Sync {
    fn snapshot(&self, target: &PaneTarget) -> anyhow::Result<PaneSnapshot>;
}

pub struct TmuxPaneSnapshotSource;

pub trait TurnSource: Send + Sync {
    fn turns(&self, session: &str) -> anyhow::Result<Vec<TurnRow>>;
}

pub struct BoopStoreTurnSource {
    pub store: boop_store::Store,
}

pub enum RegionKind { Mermaid, D2, Table, List }

pub struct SourceRegion {
    pub kind: RegionKind,
    pub source_start: usize,
    pub source_end: usize,
    pub text: String,
}

pub struct VisibleRegion {
    pub id: String,
    pub kind: RegionKind,
    pub source_start: usize,
    pub source_end: usize,
    pub pane_start: i64,
    pub pane_end: i64,
    pub text: String,
}

pub enum ProjectionConfidence { Anchored, Extended }

pub struct VisibleTurnProjection {
    pub id: String,
    pub session: String,
    pub turn: u64,
    pub role: String,
    pub pane_start: i64,
    pub pane_end: i64,
    pub confidence: ProjectionConfidence,
    pub regions: Vec<VisibleRegion>,
}

pub struct PaneProjection {
    pub snapshot: PaneSnapshot,
    pub visible_turns: Vec<VisibleTurnProjection>,
}

pub enum PaneProjectionEvent {
    Snapshot(PaneProjection),
    Entered(Vec<VisibleTurnProjection>),
    Moved(Vec<VisibleTurnProjection>),
    Exited(Vec<String>),
    Resized { width: u16, height: u16 },
    Invalidated { generation: u64 },
}

pub fn project_pane(
    snapshot: &PaneSnapshot,
    turns: &[TurnRow],
) -> PaneProjection;

pub fn turn_at_row(
    projection: &PaneProjection,
    absolute_row: i64,
) -> Option<&VisibleTurnProjection>;

pub fn region_at_row(
    projection: &PaneProjection,
    absolute_row: i64,
) -> Option<&VisibleRegion>;
```

The event API may use the repository's existing reactive runtime if one is
already present at implementation time. The public contract is a stream with
subscription cancellation. Resource teardown and subscription cancellation
remain distinct operations.

## Instance timeline

1. One `TmuxPaneSnapshotSource` exists per process.
2. One watcher exists per subscribed pane target.
3. The watcher starts a persistent `tmux -C` control-mode client for event
   invalidation. It consumes `%output`, layout, mode, window, pane, and session
   notifications.
4. Events coalesce into one pending reconciliation per pane. At most one
   snapshot/store join is in flight; one dirty bit requests the next run.
5. Reconciliation reads tmux facts and pane capture, then reads the session's
   Boop turns, then computes one immutable `PaneProjection`.
6. Entered, moved, exited, resized, and invalidated events are derived by
   comparing stable ids and row spans with the prior projection.
7. Dropping the final subscription stops that pane watcher. Explicit source
   disposal terminates the control-mode child and all watchers.

## Storage, reads, writes, and uniqueness

- Tmux is read through `display-message`, `list-panes`, and `capture-pane`.
  Observation performs zero `send-keys`, `paste-buffer`, option changes, mode
  changes, scroll commands, or selection commands.
- Boop opens its SQLite store read-only and calls typed `turn_rows`; the
  projection path does not spawn the `boop` CLI.
- The projection engine writes no tmux or SQLite state.
- Pane identity is `(socket, pane_id)`. Window indexes and session names are
  metadata because indexes move and names can change.
- Turn identity is `(session, turn)`, serialized as `<session>:<turn>`.
- Region identity is `(turn_id, kind, source_start)`.
- `generation` increases only when the captured pane facts or rows differ from
  the previous snapshot.
- A pane has one active reconciliation and one queued dirty state. Concurrent
  subscribers share the same store read and tmux capture.

## Projection algorithm

1. Capture pane facts and visible rows with absolute tmux row coordinates.
   Copy-mode position determines `viewport_top`; xterm scrollback is not used
   as the authoritative history coordinate.
2. Reconstruct logical rows from tmux wrapped-line metadata where available.
3. Normalize terminal decorations, Markdown punctuation, whitespace, and
   harness prefixes consistently for pane rows and turn source lines.
4. Build an inverted index `normalized_line -> set<turn_id>`. Unique lines are
   primary anchors. Substring anchors require minimum lengths and retain their
   source index.
5. Compute candidate `pane_row - source_row` offsets. Select the offset by
   vote count, exact-match count, anchor weight, and deterministic row tie
   break. This handles repeated Mermaid keywords and repeated table cells.
6. Sort anchored turns by pane row. Extend each span toward the adjacent turn
   boundary and viewport edge so partially visible turns at both the top and
   bottom remain addressable.
7. Detect source regions from the full Markdown turn. Project every matched
   source row using the same offset vote. Infer offscreen region ends from the
   source span so a table or diagram larger than the pane remains one region.
8. Return reverse lookup by absolute pane row. Pixel lookup remains an xterm
   host operation: client point -> xterm row -> absolute pane row -> Rust
   projection.

## Delivery plan

### Phase 0: fixtures and current-parity corpus

- Port Instant's mock Boop turns and tmux/xterm screen cases into crate
  fixtures.
- Include one, two, and three turns; top and bottom partial turns; multiline
  turns; wrapped rows; duplicate lines; a Mermaid sequence diagram with
  repeated `activate`/`deactivate`/`end`; D2; a table taller and wider than the
  pane; responsive table-as-list output; list regions; resize; copy-mode jump;
  tailing output; and session/window changes.

### Phase 1: read-only pane snapshots

- Extend `boop-mux` with typed pane facts and visible snapshots behind
  `PaneSnapshotSource`.
- Parse persistent control-mode notifications into typed invalidations.
- Retain `Multiplexer::capture_pane` for existing callers.

### Phase 2: store join and pure projection

- Add `BoopStoreTurnSource` over `boop-store::Store::open_readonly` and
  `turn_rows`.
- Implement normalization, turn intersection, source-region detection,
  projection, and reverse lookup as pure functions.
- Keep Mermaid/D2/table/list rendering outside Boop.

### Phase 3: observable watcher

- Share one watcher per pane target.
- Coalesce control-mode bursts and bound reconciliation to one in-flight read
  plus one pending rerun.
- Emit structural events only when generation, dimensions, visible ids, spans,
  or regions change. No timer-only event emission.

### Phase 4: Instant adapter and parity removal

- Add one Tauri command or channel that serializes `PaneProjectionEvent`.
- Replace the `boop` CLI subprocess and TypeScript projection calls.
- Keep xterm selection, keyboard, mouse, scroll, command-click, drag/drop, and
  pixel geometry paths unchanged.
- Remove the replaced TypeScript matching code after the parity suite passes.

## Acceptance Criteria

- [ ] `boop-mux` exports typed pane facts and a read-only `PaneSnapshotSource` implementation.
- [ ] A persistent tmux control-mode watcher produces typed pane invalidations and shuts down when its source is disposed.
- [ ] The watcher allows one reconciliation in flight and one pending rerun per pane.
- [ ] `boop-store` is accessed in-process through `Store::open_readonly` and `turn_rows`; the projection path spawns zero `boop` processes.
- [ ] `project_pane`, `turn_at_row`, and `region_at_row` are pure Rust APIs with deterministic snapshots.
- [ ] Stable ids follow `(socket,pane_id)`, `(session,turn)`, and `(turn_id,kind,source_start)`.
- [ ] Tests cover one, two, and three turns with partial intersections at both viewport edges.
- [ ] Tests cover multiline and wrapped messages, duplicate source lines, and repeated Mermaid sequence keywords.
- [ ] Tests cover a Markdown table whose start and end are both offscreen while its middle intersects the pane.
- [ ] Tests cover responsive table-as-list terminal output and preserve the source table region identity.
- [ ] Tests cover resize, zoom-induced xterm geometry changes at the adapter boundary, tmux copy-mode scroll, tailing output, jumps, window changes, and session changes.
- [ ] A read-only isolation test hashes tmux session/window/pane state before and after at least 600 observations and reports byte-identical state.
- [ ] A watcher test demonstrates zero projection events while pane and store state remain unchanged.
- [ ] An event-burst test demonstrates bounded captures and store reads under sustained `%output` notifications.
- [ ] Instant Playwright parity tests cover right-click turn lookup, partial top/bottom turns, inline Mermaid/D2 scroll anchoring, native selection, keyboard input, command-click, and image drop.
- [ ] `cargo test -p boop-mux` and the workspace gates pass.

## Tests Run

Planning only.

## Implementation Notes

The existing `Multiplexer` trait remains load-bearing for lifecycle and input
callers. Snapshot observation is additive. The Instant proof script at
`/Users/chrishafley/projects/instant/scripts/0_tmux-readonly-proof.sh`
already demonstrates 600 read queries with identical before/after state and
should be ported into the isolated Rust harness.

## Comments

### 2026-08-31T01:23:09Z · @intake

Obsolete: Superseded by the host-neutral TerminalSnapshot seam and parity scope in @terminal-snapshot-boundary. (superseded by terminal-snapshot-boundary)
