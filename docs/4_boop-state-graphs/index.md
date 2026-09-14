# Boop session graphs

Code baseline: hafley-rs `2376a81f`, Instant `9fca010e`.

Diagrams are embedded as D2 fences for Instant and `@hafley66/md`. Click a rendered diagram to pan and zoom. The final model is proposed; the preceding diagrams describe the audited code.

Edge colors: source groups in ER/flow/predicate diagrams; event phases in the identity sequence; semantic roles in state charts. Dashed logical references are retained. Rebuild inputs and color legends live in `7_theme/`.

## Implemented ER model

```d2
# Source audit: ../1_boop-session-state-audit.md
# Main: hafley-rs 2376a81f; Instant 9fca010e; 2026-09-13
vars: { d2-config: { theme-id: 201; layout-engine: elk; pad: 32 } }
classes: {
  fact: { style.fill: "#153b43"; style.stroke: "#63c7cd"; style.font-color: "#e1f5f5" }
  good: { style.fill: "#203e2b"; style.stroke: "#79b98b"; style.font-color: "#e5f3e8" }
  gap: { style.fill: "#473819"; style.stroke: "#d5b66e"; style.font-color: "#fff0c5" }
  bad: { style.fill: "#462629"; style.stroke: "#cb888a"; style.font-color: "#ffe2e2" }
}

direction: right
 dictionary: dict_session {shape: sql_table; id: integer {constraint: primary_key}; value: text {constraint: unique}}
 session: agent_session {shape: sql_table; session_id: integer {constraint: primary_key}; harness_id: integer; cwd_id: integer; started_ts: timestamp}
 trace: agent_trace {shape: sql_table; trace_id: integer {constraint: primary_key}; root_session_id: integer; started_ts: timestamp}
 membership: agent_trace_span {shape: sql_table; session_id: integer {constraint: primary_key}; trace_id: integer; attach_id: integer; attached_ts: timestamp}
 live: agent_live {shape: sql_table; session_id: integer {constraint: primary_key}; pid: nullable; tmux_pane_id: nullable; status_id: nullable; door_kind: text; door_addr: text}
 intervals: agent_live_span {shape: sql_table; session_id: integer {constraint: primary_key}; from_ts: timestamp {constraint: primary_key}; to_ts: nullable; pid: nullable; tmux_pane_id: nullable; status_id: integer}
 turns: agent_turn {shape: sql_table; session_id: integer {constraint: primary_key}; turn: integer {constraint: primary_key}; ts: timestamp; role_id: integer}
 lane: agent_lane {shape: sql_table; spawn_id: integer {constraint: primary_key}; lane_id: integer; parent_lane_id: nullable; trace_id: nullable; spawned_ts: timestamp}
 route: "registry.json route\nkey: lane name\nselected session / parent / tmux\nmodel / registration time" {class: fact}
 residency: "lane-residency.json\nkey: lane name\nlive | idle | retired" {class: fact}
 process: "OS process snapshot\nPID + parent + start time\ncurrent binding checks only PID" {class: gap}
 dictionary -> session: "1 : 0..1"
 dictionary -> membership: "1 : 0..1"
 trace -> membership: "1 : many sessions"
 dictionary -> live: "1 : 0..1"
 dictionary -> intervals: "1 : many intervals"
 session -> turns: "1 : many turns"
 trace -> lane: "1 : many spawn records"
 route -> dictionary: "optional selected session\nlogical reference" {style.stroke-dash: 3}
 route -> lane: "name lookup\nmay match multiple spawns" {style.stroke-dash: 3}
 route -> residency: "same lane name" {style.stroke-dash: 3}
 live -> process: "PID presence lookup" {style.stroke-dash: 3}
 note: "IMPLEMENTED STORAGE\nLogical cardinalities; edges do not assert SQL FK constraints.\nFirst trace attachment wins. Bare session IDs are globally unique.\nTrace has no lifecycle state; bindings omit process start time." {class: gap}

# D2 theme kit: midnight / categorical
vars: { d2-config: { theme-id: 200; theme-overrides: { N1: "#edf3f8"; N2: "#edf3f8"; N3: "#edf3f8"; N4: "#c0cce0"; N5: "#1d252d"; N6: "#1d252d"; N7: "#101820"; B1: "#67d5e8"; B2: "#67d5e8"; B3: "#67d5e8"; B4: "#1d252d"; B5: "#1d252d"; B6: "#1d252d"; AA2: "#ffc66d"; AA4: "#1d252d"; AA5: "#1d252d"; AB4: "#1d252d"; AB5: "#1d252d" } } }
style.fill: "#101820"
classes: {
  kit_c0: { style.fill: "#1b313a"; style.stroke: "#67d5e8"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c0: { style.stroke: "#67d5e8"; style.font-color: "#67d5e8"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c1: { style.fill: "#2f2f2a"; style.stroke: "#ffc66d"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c1: { style.stroke: "#ffc66d"; style.font-color: "#ffc66d"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c2: { style.fill: "#212e3d"; style.stroke: "#91c4ff"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c2: { style.stroke: "#91c4ff"; style.font-color: "#91c4ff"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c3: { style.fill: "#23312d"; style.stroke: "#a5dc83"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c3: { style.stroke: "#a5dc83"; style.font-color: "#a5dc83"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c4: { style.fill: "#2f292d"; style.stroke: "#ff9784"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c4: { style.stroke: "#ff9784"; style.font-color: "#ff9784"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c5: { style.fill: "#2c302d"; style.stroke: "#e5d482"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c5: { style.stroke: "#e5d482"; style.font-color: "#e5d482"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c6: { style.fill: "#1d3133"; style.stroke: "#75d9b0"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c6: { style.stroke: "#75d9b0"; style.font-color: "#75d9b0"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c7: { style.fill: "#272f39"; style.stroke: "#c0cce0"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c7: { style.stroke: "#c0cce0"; style.font-color: "#c0cce0"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c8: { style.fill: "#2d2b2b"; style.stroke: "#f2ad78"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c8: { style.stroke: "#f2ad78"; style.font-color: "#f2ad78"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c9: { style.fill: "#1f2f37"; style.stroke: "#86cad4"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c9: { style.stroke: "#86cad4"; style.font-color: "#86cad4"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c10: { style.fill: "#26302a"; style.stroke: "#bcd16d"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c10: { style.stroke: "#bcd16d"; style.font-color: "#bcd16d"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c11: { style.fill: "#262c3c"; style.stroke: "#b7b5fb"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c11: { style.stroke: "#b7b5fb"; style.font-color: "#b7b5fb"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c12: { style.fill: "#2b2d31"; style.stroke: "#e2bda5"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c12: { style.stroke: "#e2bda5"; style.font-color: "#e2bda5"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c13: { style.fill: "#1e2c3b"; style.stroke: "#7eb5ef"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c13: { style.stroke: "#7eb5ef"; style.font-color: "#7eb5ef"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c14: { style.fill: "#293231"; style.stroke: "#d4dfa4"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c14: { style.stroke: "#d4dfa4"; style.font-color: "#d4dfa4"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c15: { style.fill: "#282f36"; style.stroke: "#c5c9cb"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c15: { style.stroke: "#c5c9cb"; style.font-color: "#c5c9cb"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_data: { style.fill: "#1b313a"; style.stroke: "#67d5e8"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_data: { style.stroke: "#67d5e8"; style.font-color: "#67d5e8"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_control: { style.fill: "#2f2f2a"; style.stroke: "#ffc66d"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_control: { style.stroke: "#ffc66d"; style.font-color: "#ffc66d"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_identity: { style.fill: "#212e3d"; style.stroke: "#91c4ff"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_identity: { style.stroke: "#91c4ff"; style.font-color: "#91c4ff"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_success: { style.fill: "#23312d"; style.stroke: "#a5dc83"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_success: { style.stroke: "#a5dc83"; style.font-color: "#a5dc83"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_failure: { style.fill: "#2f292d"; style.stroke: "#ff9784"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_failure: { style.stroke: "#ff9784"; style.font-color: "#ff9784"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_warning: { style.fill: "#2c302d"; style.stroke: "#e5d482"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_warning: { style.stroke: "#e5d482"; style.font-color: "#e5d482"; style.stroke-width: 3; style.stroke-dash: 3 }
  kit_storage: { style.fill: "#1d3133"; style.stroke: "#75d9b0"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_storage: { style.stroke: "#75d9b0"; style.font-color: "#75d9b0"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_inactive: { style.fill: "#272f39"; style.stroke: "#c0cce0"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_inactive: { style.stroke: "#c0cce0"; style.font-color: "#c0cce0"; style.stroke-width: 3; style.stroke-dash: 5 }
}

# Explicit group assignments
(dictionary -> session)[0].style.stroke: "#67d5e8"
(dictionary -> session)[0].style.stroke-width: 3
(dictionary -> session)[0].style.font-color: "#67d5e8"
(dictionary -> session)[0].style.stroke-dash: 0
(dictionary -> membership)[0].style.stroke: "#67d5e8"
(dictionary -> membership)[0].style.stroke-width: 3
(dictionary -> membership)[0].style.font-color: "#67d5e8"
(dictionary -> membership)[0].style.stroke-dash: 0
(trace -> membership)[0].style.stroke: "#ffc66d"
(trace -> membership)[0].style.stroke-width: 3
(trace -> membership)[0].style.font-color: "#ffc66d"
(trace -> membership)[0].style.stroke-dash: 0
(dictionary -> live)[0].style.stroke: "#67d5e8"
(dictionary -> live)[0].style.stroke-width: 3
(dictionary -> live)[0].style.font-color: "#67d5e8"
(dictionary -> live)[0].style.stroke-dash: 0
(dictionary -> intervals)[0].style.stroke: "#67d5e8"
(dictionary -> intervals)[0].style.stroke-width: 3
(dictionary -> intervals)[0].style.font-color: "#67d5e8"
(dictionary -> intervals)[0].style.stroke-dash: 0
(session -> turns)[0].style.stroke: "#91c4ff"
(session -> turns)[0].style.stroke-width: 3
(session -> turns)[0].style.font-color: "#91c4ff"
(session -> turns)[0].style.stroke-dash: 0
(trace -> lane)[0].style.stroke: "#ffc66d"
(trace -> lane)[0].style.stroke-width: 3
(trace -> lane)[0].style.font-color: "#ffc66d"
(trace -> lane)[0].style.stroke-dash: 0
(route -> dictionary)[0].style.stroke: "#a5dc83"
(route -> dictionary)[0].style.stroke-width: 3
(route -> dictionary)[0].style.font-color: "#a5dc83"
(route -> dictionary)[0].style.stroke-dash: 3
(route -> lane)[0].style.stroke: "#a5dc83"
(route -> lane)[0].style.stroke-width: 3
(route -> lane)[0].style.font-color: "#a5dc83"
(route -> lane)[0].style.stroke-dash: 3
(route -> residency)[0].style.stroke: "#a5dc83"
(route -> residency)[0].style.stroke-width: 3
(route -> residency)[0].style.font-color: "#a5dc83"
(route -> residency)[0].style.stroke-dash: 3
(live -> process)[0].style.stroke: "#ff9784"
(live -> process)[0].style.stroke-width: 3
(live -> process)[0].style.font-color: "#ff9784"
(live -> process)[0].style.stroke-dash: 3
```

## Observation to Instant flow

```d2
# Source audit: ../1_boop-session-state-audit.md
# Main: hafley-rs 2376a81f; Instant 9fca010e; 2026-09-13
vars: { d2-config: { theme-id: 201; layout-engine: elk; pad: 32 } }
classes: {
  fact: { style.fill: "#153b43"; style.stroke: "#63c7cd"; style.font-color: "#e1f5f5" }
  good: { style.fill: "#203e2b"; style.stroke: "#79b98b"; style.font-color: "#e5f3e8" }
  gap: { style.fill: "#473819"; style.stroke: "#d5b66e"; style.font-color: "#fff0c5" }
  bad: { style.fill: "#462629"; style.stroke: "#cb888a"; style.font-color: "#ffe2e2" }
}

direction: down
 observations: "OBSERVATION SOURCES" {
  native: "Harness registry / native events" {class: fact}
  files: "Transcript sync" {class: fact}
  os: "OS process snapshot" {class: fact}
  tmux: "tmux listing + pane probes" {class: fact}
  supervisor: "Supervisor turn loop" {class: fact}
 }
 storage: "WRITERS AND STORES" {
  bind: "bind_native_session\nattach trace / update route\nrecord live or detached"
  sync: "sync_session_with\nwith pane: live\nwithout pane: idle"
  db: "agent_live + intervals\ntrace memberships + sessions" {shape: cylinder}
  registry: "registry.json" {shape: cylinder}
  residence: "lane-residency.json\nlive / idle / retired" {shape: cylinder}
 }
 readers: "INDEPENDENT CLASSIFIERS" {
  runtime: "runtime_snapshot\nPID existence + tmux membership"
  graph: "session graph\nnative nodes: stored status\nshells: runtime fold + pane probe" {class: gap}
  summary: "active_agents\nprocess Live OR tmux Live"
  cli: "lane list\nparent hop + target_alive + residency" {class: gap}
 }
 instant: "INSTANT" {
  network: "Boop network\nactive-only: state == live\nor a live descendant"
  screenshot: "Screenshot tmux table\nsession / title / proc / win\nno typed process or turn state" {class: gap}
 }
 observations.native -> storage.bind
 observations.files -> storage.sync
 storage.bind -> storage.db
 storage.bind -> storage.registry
 storage.sync -> storage.db
 observations.supervisor -> storage.residence
 storage.db -> readers.runtime
 storage.registry -> readers.runtime
 observations.os -> readers.runtime
 observations.tmux -> readers.runtime
 readers.runtime -> readers.summary
 readers.runtime -> readers.graph
 storage.db -> readers.graph: "durable native state"
 observations.tmux -> readers.graph: "exact pane correction"
 storage.registry -> readers.cli
 storage.residence -> readers.cli
 observations.tmux -> readers.cli
 readers.graph -> instant.network
 observations.tmux -> instant.screenshot: "pty.rs: command/title strings"
 observations.os -> instant.screenshot: "resolve boop descendant command"

# D2 theme kit: midnight / categorical
vars: { d2-config: { theme-id: 200; theme-overrides: { N1: "#edf3f8"; N2: "#edf3f8"; N3: "#edf3f8"; N4: "#c0cce0"; N5: "#1d252d"; N6: "#1d252d"; N7: "#101820"; B1: "#67d5e8"; B2: "#67d5e8"; B3: "#67d5e8"; B4: "#1d252d"; B5: "#1d252d"; B6: "#1d252d"; AA2: "#ffc66d"; AA4: "#1d252d"; AA5: "#1d252d"; AB4: "#1d252d"; AB5: "#1d252d" } } }
style.fill: "#101820"
classes: {
  kit_c0: { style.fill: "#1b313a"; style.stroke: "#67d5e8"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c0: { style.stroke: "#67d5e8"; style.font-color: "#67d5e8"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c1: { style.fill: "#2f2f2a"; style.stroke: "#ffc66d"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c1: { style.stroke: "#ffc66d"; style.font-color: "#ffc66d"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c2: { style.fill: "#212e3d"; style.stroke: "#91c4ff"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c2: { style.stroke: "#91c4ff"; style.font-color: "#91c4ff"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c3: { style.fill: "#23312d"; style.stroke: "#a5dc83"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c3: { style.stroke: "#a5dc83"; style.font-color: "#a5dc83"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c4: { style.fill: "#2f292d"; style.stroke: "#ff9784"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c4: { style.stroke: "#ff9784"; style.font-color: "#ff9784"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c5: { style.fill: "#2c302d"; style.stroke: "#e5d482"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c5: { style.stroke: "#e5d482"; style.font-color: "#e5d482"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c6: { style.fill: "#1d3133"; style.stroke: "#75d9b0"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c6: { style.stroke: "#75d9b0"; style.font-color: "#75d9b0"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c7: { style.fill: "#272f39"; style.stroke: "#c0cce0"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c7: { style.stroke: "#c0cce0"; style.font-color: "#c0cce0"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c8: { style.fill: "#2d2b2b"; style.stroke: "#f2ad78"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c8: { style.stroke: "#f2ad78"; style.font-color: "#f2ad78"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c9: { style.fill: "#1f2f37"; style.stroke: "#86cad4"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c9: { style.stroke: "#86cad4"; style.font-color: "#86cad4"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c10: { style.fill: "#26302a"; style.stroke: "#bcd16d"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c10: { style.stroke: "#bcd16d"; style.font-color: "#bcd16d"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c11: { style.fill: "#262c3c"; style.stroke: "#b7b5fb"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c11: { style.stroke: "#b7b5fb"; style.font-color: "#b7b5fb"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c12: { style.fill: "#2b2d31"; style.stroke: "#e2bda5"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c12: { style.stroke: "#e2bda5"; style.font-color: "#e2bda5"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c13: { style.fill: "#1e2c3b"; style.stroke: "#7eb5ef"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c13: { style.stroke: "#7eb5ef"; style.font-color: "#7eb5ef"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c14: { style.fill: "#293231"; style.stroke: "#d4dfa4"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c14: { style.stroke: "#d4dfa4"; style.font-color: "#d4dfa4"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c15: { style.fill: "#282f36"; style.stroke: "#c5c9cb"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c15: { style.stroke: "#c5c9cb"; style.font-color: "#c5c9cb"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_data: { style.fill: "#1b313a"; style.stroke: "#67d5e8"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_data: { style.stroke: "#67d5e8"; style.font-color: "#67d5e8"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_control: { style.fill: "#2f2f2a"; style.stroke: "#ffc66d"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_control: { style.stroke: "#ffc66d"; style.font-color: "#ffc66d"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_identity: { style.fill: "#212e3d"; style.stroke: "#91c4ff"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_identity: { style.stroke: "#91c4ff"; style.font-color: "#91c4ff"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_success: { style.fill: "#23312d"; style.stroke: "#a5dc83"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_success: { style.stroke: "#a5dc83"; style.font-color: "#a5dc83"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_failure: { style.fill: "#2f292d"; style.stroke: "#ff9784"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_failure: { style.stroke: "#ff9784"; style.font-color: "#ff9784"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_warning: { style.fill: "#2c302d"; style.stroke: "#e5d482"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_warning: { style.stroke: "#e5d482"; style.font-color: "#e5d482"; style.stroke-width: 3; style.stroke-dash: 3 }
  kit_storage: { style.fill: "#1d3133"; style.stroke: "#75d9b0"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_storage: { style.stroke: "#75d9b0"; style.font-color: "#75d9b0"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_inactive: { style.fill: "#272f39"; style.stroke: "#c0cce0"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_inactive: { style.stroke: "#c0cce0"; style.font-color: "#c0cce0"; style.stroke-width: 3; style.stroke-dash: 5 }
}

# Explicit group assignments
(observations.native -> storage.bind)[0].style.stroke: "#67d5e8"
(observations.native -> storage.bind)[0].style.stroke-width: 3
(observations.native -> storage.bind)[0].style.font-color: "#67d5e8"
(observations.native -> storage.bind)[0].style.stroke-dash: 0
(observations.files -> storage.sync)[0].style.stroke: "#ffc66d"
(observations.files -> storage.sync)[0].style.stroke-width: 3
(observations.files -> storage.sync)[0].style.font-color: "#ffc66d"
(observations.files -> storage.sync)[0].style.stroke-dash: 0
(storage.bind -> storage.db)[0].style.stroke: "#91c4ff"
(storage.bind -> storage.db)[0].style.stroke-width: 3
(storage.bind -> storage.db)[0].style.font-color: "#91c4ff"
(storage.bind -> storage.db)[0].style.stroke-dash: 0
(storage.bind -> storage.registry)[0].style.stroke: "#91c4ff"
(storage.bind -> storage.registry)[0].style.stroke-width: 3
(storage.bind -> storage.registry)[0].style.font-color: "#91c4ff"
(storage.bind -> storage.registry)[0].style.stroke-dash: 0
(storage.sync -> storage.db)[0].style.stroke: "#a5dc83"
(storage.sync -> storage.db)[0].style.stroke-width: 3
(storage.sync -> storage.db)[0].style.font-color: "#a5dc83"
(storage.sync -> storage.db)[0].style.stroke-dash: 0
(observations.supervisor -> storage.residence)[0].style.stroke: "#ff9784"
(observations.supervisor -> storage.residence)[0].style.stroke-width: 3
(observations.supervisor -> storage.residence)[0].style.font-color: "#ff9784"
(observations.supervisor -> storage.residence)[0].style.stroke-dash: 0
(storage.db -> readers.runtime)[0].style.stroke: "#e5d482"
(storage.db -> readers.runtime)[0].style.stroke-width: 3
(storage.db -> readers.runtime)[0].style.font-color: "#e5d482"
(storage.db -> readers.runtime)[0].style.stroke-dash: 0
(storage.registry -> readers.runtime)[0].style.stroke: "#75d9b0"
(storage.registry -> readers.runtime)[0].style.stroke-width: 3
(storage.registry -> readers.runtime)[0].style.font-color: "#75d9b0"
(storage.registry -> readers.runtime)[0].style.stroke-dash: 0
(observations.os -> readers.runtime)[0].style.stroke: "#c0cce0"
(observations.os -> readers.runtime)[0].style.stroke-width: 3
(observations.os -> readers.runtime)[0].style.font-color: "#c0cce0"
(observations.os -> readers.runtime)[0].style.stroke-dash: 0
(observations.tmux -> readers.runtime)[0].style.stroke: "#f2ad78"
(observations.tmux -> readers.runtime)[0].style.stroke-width: 3
(observations.tmux -> readers.runtime)[0].style.font-color: "#f2ad78"
(observations.tmux -> readers.runtime)[0].style.stroke-dash: 0
(readers.runtime -> readers.summary)[0].style.stroke: "#86cad4"
(readers.runtime -> readers.summary)[0].style.stroke-width: 3
(readers.runtime -> readers.summary)[0].style.font-color: "#86cad4"
(readers.runtime -> readers.summary)[0].style.stroke-dash: 0
(readers.runtime -> readers.graph)[0].style.stroke: "#86cad4"
(readers.runtime -> readers.graph)[0].style.stroke-width: 3
(readers.runtime -> readers.graph)[0].style.font-color: "#86cad4"
(readers.runtime -> readers.graph)[0].style.stroke-dash: 0
(storage.db -> readers.graph)[0].style.stroke: "#e5d482"
(storage.db -> readers.graph)[0].style.stroke-width: 3
(storage.db -> readers.graph)[0].style.font-color: "#e5d482"
(storage.db -> readers.graph)[0].style.stroke-dash: 0
(observations.tmux -> readers.graph)[0].style.stroke: "#f2ad78"
(observations.tmux -> readers.graph)[0].style.stroke-width: 3
(observations.tmux -> readers.graph)[0].style.font-color: "#f2ad78"
(observations.tmux -> readers.graph)[0].style.stroke-dash: 0
(storage.registry -> readers.cli)[0].style.stroke: "#75d9b0"
(storage.registry -> readers.cli)[0].style.stroke-width: 3
(storage.registry -> readers.cli)[0].style.font-color: "#75d9b0"
(storage.registry -> readers.cli)[0].style.stroke-dash: 0
(storage.residence -> readers.cli)[0].style.stroke: "#bcd16d"
(storage.residence -> readers.cli)[0].style.stroke-width: 3
(storage.residence -> readers.cli)[0].style.font-color: "#bcd16d"
(storage.residence -> readers.cli)[0].style.stroke-dash: 0
(observations.tmux -> readers.cli)[0].style.stroke: "#f2ad78"
(observations.tmux -> readers.cli)[0].style.stroke-width: 3
(observations.tmux -> readers.cli)[0].style.font-color: "#f2ad78"
(observations.tmux -> readers.cli)[0].style.stroke-dash: 0
(readers.graph -> instant.network)[0].style.stroke: "#b7b5fb"
(readers.graph -> instant.network)[0].style.stroke-width: 3
(readers.graph -> instant.network)[0].style.font-color: "#b7b5fb"
(readers.graph -> instant.network)[0].style.stroke-dash: 0
(observations.tmux -> instant.screenshot)[0].style.stroke: "#f2ad78"
(observations.tmux -> instant.screenshot)[0].style.stroke-width: 3
(observations.tmux -> instant.screenshot)[0].style.font-color: "#f2ad78"
(observations.tmux -> instant.screenshot)[0].style.stroke-dash: 0
(observations.os -> instant.screenshot)[0].style.stroke: "#c0cce0"
(observations.os -> instant.screenshot)[0].style.stroke-width: 3
(observations.os -> instant.screenshot)[0].style.font-color: "#c0cce0"
(observations.os -> instant.screenshot)[0].style.stroke-dash: 0
```

## Native wrapper state chart

```d2
# Source audit: ../1_boop-session-state-audit.md
# Main: hafley-rs 2376a81f; Instant 9fca010e; 2026-09-13
vars: { d2-config: { theme-id: 201; layout-engine: dagre; pad: 32 } }
classes: {
  fact: { style.fill: "#153b43"; style.stroke: "#63c7cd"; style.font-color: "#e1f5f5" }
  good: { style.fill: "#203e2b"; style.stroke: "#79b98b"; style.font-color: "#e5f3e8" }
  gap: { style.fill: "#473819"; style.stroke: "#d5b66e"; style.font-color: "#fff0c5" }
  bad: { style.fill: "#462629"; style.stroke: "#cb888a"; style.font-color: "#ffe2e2" }
}

direction: down
 unbound: "UNBOUND\nprocess may already exist\nroute.session_id = None" {class: gap}
 bound: "BOUND(S)\nagent_live = live\nPID + pane attached" {class: good}
 replace: "REPLACE A WITH B\nA -> detached\nclear A PID/pane" {class: fact}
 closed: "CLOSED(S)\nclear selected session/model\nclear PID/pane" {class: bad}
 detached: "DETACHED(S)\nclear PID/pane and door\nretain session + trace" {class: bad}
 respawn: "RESPAWN GATE\nobserver failure or numeric nonzero exit\nuptime >= 10s AND attempts < 3" {shape: diamond; class: gap}
 ended: "WRAPPER ENDED\nstored conversation remains" {class: bad}
 unbound -> bound: "Session(S) observed\nunique PID / pane / adapter fallback"
 unbound -> unbound: "discovery deadline\nno identity found"
 bound -> bound: "Session(S), settings(S), compact\nlate events for other sessions ignored"
 bound -> replace: "Session(B), B != A"
 replace -> bound: "bind B\ntrace(B) else carried trace else trace-B"
 bound -> closed: "Closed(S) for selected S"
 closed -> bound: "later Session(S or B)"
 bound -> detached: "wrapper releases\nrecorded PID still owned"
 bound -> bound: "release after another PID rebound\nnew owner preserved"
 detached -> respawn
 respawn -> unbound: "retry allowed\nnew process / resume"
 respawn -> ended: "clean exit / signal / short uptime / cap"
 note: "control.rs: bind_native_session / apply_native_event / release_native_route\nBoxes combine route binding and stored observations for this view.\nNo clearing or compacting state is persisted on main." {class: gap}

# D2 theme kit: midnight / categorical
vars: { d2-config: { theme-id: 200; theme-overrides: { N1: "#edf3f8"; N2: "#edf3f8"; N3: "#edf3f8"; N4: "#c0cce0"; N5: "#1d252d"; N6: "#1d252d"; N7: "#101820"; B1: "#67d5e8"; B2: "#67d5e8"; B3: "#67d5e8"; B4: "#1d252d"; B5: "#1d252d"; B6: "#1d252d"; AA2: "#ffc66d"; AA4: "#1d252d"; AA5: "#1d252d"; AB4: "#1d252d"; AB5: "#1d252d" } } }
style.fill: "#101820"
classes: {
  kit_c0: { style.fill: "#1b313a"; style.stroke: "#67d5e8"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c0: { style.stroke: "#67d5e8"; style.font-color: "#67d5e8"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c1: { style.fill: "#2f2f2a"; style.stroke: "#ffc66d"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c1: { style.stroke: "#ffc66d"; style.font-color: "#ffc66d"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c2: { style.fill: "#212e3d"; style.stroke: "#91c4ff"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c2: { style.stroke: "#91c4ff"; style.font-color: "#91c4ff"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c3: { style.fill: "#23312d"; style.stroke: "#a5dc83"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c3: { style.stroke: "#a5dc83"; style.font-color: "#a5dc83"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c4: { style.fill: "#2f292d"; style.stroke: "#ff9784"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c4: { style.stroke: "#ff9784"; style.font-color: "#ff9784"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c5: { style.fill: "#2c302d"; style.stroke: "#e5d482"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c5: { style.stroke: "#e5d482"; style.font-color: "#e5d482"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c6: { style.fill: "#1d3133"; style.stroke: "#75d9b0"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c6: { style.stroke: "#75d9b0"; style.font-color: "#75d9b0"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c7: { style.fill: "#272f39"; style.stroke: "#c0cce0"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c7: { style.stroke: "#c0cce0"; style.font-color: "#c0cce0"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c8: { style.fill: "#2d2b2b"; style.stroke: "#f2ad78"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c8: { style.stroke: "#f2ad78"; style.font-color: "#f2ad78"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c9: { style.fill: "#1f2f37"; style.stroke: "#86cad4"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c9: { style.stroke: "#86cad4"; style.font-color: "#86cad4"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c10: { style.fill: "#26302a"; style.stroke: "#bcd16d"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c10: { style.stroke: "#bcd16d"; style.font-color: "#bcd16d"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c11: { style.fill: "#262c3c"; style.stroke: "#b7b5fb"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c11: { style.stroke: "#b7b5fb"; style.font-color: "#b7b5fb"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c12: { style.fill: "#2b2d31"; style.stroke: "#e2bda5"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c12: { style.stroke: "#e2bda5"; style.font-color: "#e2bda5"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c13: { style.fill: "#1e2c3b"; style.stroke: "#7eb5ef"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c13: { style.stroke: "#7eb5ef"; style.font-color: "#7eb5ef"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c14: { style.fill: "#293231"; style.stroke: "#d4dfa4"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c14: { style.stroke: "#d4dfa4"; style.font-color: "#d4dfa4"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c15: { style.fill: "#282f36"; style.stroke: "#c5c9cb"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c15: { style.stroke: "#c5c9cb"; style.font-color: "#c5c9cb"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_data: { style.fill: "#1b313a"; style.stroke: "#67d5e8"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_data: { style.stroke: "#67d5e8"; style.font-color: "#67d5e8"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_control: { style.fill: "#2f2f2a"; style.stroke: "#ffc66d"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_control: { style.stroke: "#ffc66d"; style.font-color: "#ffc66d"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_identity: { style.fill: "#212e3d"; style.stroke: "#91c4ff"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_identity: { style.stroke: "#91c4ff"; style.font-color: "#91c4ff"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_success: { style.fill: "#23312d"; style.stroke: "#a5dc83"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_success: { style.stroke: "#a5dc83"; style.font-color: "#a5dc83"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_failure: { style.fill: "#2f292d"; style.stroke: "#ff9784"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_failure: { style.stroke: "#ff9784"; style.font-color: "#ff9784"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_warning: { style.fill: "#2c302d"; style.stroke: "#e5d482"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_warning: { style.stroke: "#e5d482"; style.font-color: "#e5d482"; style.stroke-width: 3; style.stroke-dash: 3 }
  kit_storage: { style.fill: "#1d3133"; style.stroke: "#75d9b0"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_storage: { style.stroke: "#75d9b0"; style.font-color: "#75d9b0"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_inactive: { style.fill: "#272f39"; style.stroke: "#c0cce0"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_inactive: { style.stroke: "#c0cce0"; style.font-color: "#c0cce0"; style.stroke-width: 3; style.stroke-dash: 5 }
}

# Explicit group assignments
(unbound -> bound)[0].style.stroke: "#a5dc83"
(unbound -> bound)[0].style.stroke-width: 3
(unbound -> bound)[0].style.font-color: "#a5dc83"
(unbound -> bound)[0].style.stroke-dash: 0
(unbound -> unbound)[0].style.stroke: "#e5d482"
(unbound -> unbound)[0].style.stroke-width: 3
(unbound -> unbound)[0].style.font-color: "#e5d482"
(unbound -> unbound)[0].style.stroke-dash: 3
(bound -> bound)[0].style.stroke: "#a5dc83"
(bound -> bound)[0].style.stroke-width: 3
(bound -> bound)[0].style.font-color: "#a5dc83"
(bound -> bound)[0].style.stroke-dash: 0
(bound -> replace)[0].style.stroke: "#91c4ff"
(bound -> replace)[0].style.stroke-width: 3
(bound -> replace)[0].style.font-color: "#91c4ff"
(bound -> replace)[0].style.stroke-dash: 0
(replace -> bound)[0].style.stroke: "#a5dc83"
(replace -> bound)[0].style.stroke-width: 3
(replace -> bound)[0].style.font-color: "#a5dc83"
(replace -> bound)[0].style.stroke-dash: 0
(bound -> closed)[0].style.stroke: "#ff9784"
(bound -> closed)[0].style.stroke-width: 3
(bound -> closed)[0].style.font-color: "#ff9784"
(bound -> closed)[0].style.stroke-dash: 0
(closed -> bound)[0].style.stroke: "#a5dc83"
(closed -> bound)[0].style.stroke-width: 3
(closed -> bound)[0].style.font-color: "#a5dc83"
(closed -> bound)[0].style.stroke-dash: 0
(bound -> detached)[0].style.stroke: "#c0cce0"
(bound -> detached)[0].style.stroke-width: 3
(bound -> detached)[0].style.font-color: "#c0cce0"
(bound -> detached)[0].style.stroke-dash: 5
(bound -> bound)[1].style.stroke: "#a5dc83"
(bound -> bound)[1].style.stroke-width: 3
(bound -> bound)[1].style.font-color: "#a5dc83"
(bound -> bound)[1].style.stroke-dash: 0
(detached -> respawn)[0].style.stroke: "#ffc66d"
(detached -> respawn)[0].style.stroke-width: 3
(detached -> respawn)[0].style.font-color: "#ffc66d"
(detached -> respawn)[0].style.stroke-dash: 0
(respawn -> unbound)[0].style.stroke: "#e5d482"
(respawn -> unbound)[0].style.stroke-width: 3
(respawn -> unbound)[0].style.font-color: "#e5d482"
(respawn -> unbound)[0].style.stroke-dash: 3
(respawn -> ended)[0].style.stroke: "#ff9784"
(respawn -> ended)[0].style.stroke-width: 3
(respawn -> ended)[0].style.font-color: "#ff9784"
(respawn -> ended)[0].style.stroke-dash: 0
```

## Supervisor state chart

```d2
# Source audit: ../1_boop-session-state-audit.md
# Main: hafley-rs 2376a81f; Instant 9fca010e; 2026-09-13
vars: { d2-config: { theme-id: 201; layout-engine: dagre; pad: 32 } }
classes: {
  fact: { style.fill: "#153b43"; style.stroke: "#63c7cd"; style.font-color: "#e1f5f5" }
  good: { style.fill: "#203e2b"; style.stroke: "#79b98b"; style.font-color: "#e5f3e8" }
  gap: { style.fill: "#473819"; style.stroke: "#d5b66e"; style.font-color: "#fff0c5" }
  bad: { style.fill: "#462629"; style.stroke: "#cb888a"; style.font-color: "#ffe2e2" }
}

direction: down
 start: "OPEN CHANNEL"
 running: "RUNNING TURN\nresidency = live" {class: good}
 boundary: "TURN FINISHED\nrecord turn-finish\nremember conversation" {class: fact}
 retry: "RETRYABLE?\nFlaked AND retries < 5" {shape: diamond}
 held: "HELD MAIL?" {shape: diamond}
 success: "DONE?" {shape: diamond}
 idle: "PARKED\nresidency = idle\nchannel remains open" {class: fact}
 retired: "RETIRED\nclose channel\nresidency = retired" {class: gap}
 ended: "ENDED\nclose channel / exit result" {class: bad}
 result: "Completion result may be written once\nwhile supervisor remains alive" {class: gap}
 start -> running: "start_turn"
 running -> running: "Started / poll / steer\nhold next-turn mail"
 running -> boundary: "Done / Failed / Flaked"
 boundary -> retry
 retry -> running: "yes: resume text / increment retry"
 retry -> held: "no: drain mailbox"
 held -> running: "yes: next turn"
 held -> success: "no"
 success -> ended: "Failed / exhausted Flaked"
 success -> idle: "Done"
 success -> result: "Done + verdict + no prior result" {style.stroke-dash: 3}
 idle -> running: "mail arrives"
 idle -> retired: "idle eviction predicate"
 running -> ended: "parent Kill / delete / termination"
 idle -> ended: "parent Kill / delete / termination"
 running -> running: "parent Orphan\nor successful Reparent"
 note: "supervise.rs:1204,1450,1530,1589\nStall checks run during a turn; parked idle does not use them.\nNative LiveStatus and durable agent_live are separate from residency." {class: gap}

# D2 theme kit: midnight / categorical
vars: { d2-config: { theme-id: 200; theme-overrides: { N1: "#edf3f8"; N2: "#edf3f8"; N3: "#edf3f8"; N4: "#c0cce0"; N5: "#1d252d"; N6: "#1d252d"; N7: "#101820"; B1: "#67d5e8"; B2: "#67d5e8"; B3: "#67d5e8"; B4: "#1d252d"; B5: "#1d252d"; B6: "#1d252d"; AA2: "#ffc66d"; AA4: "#1d252d"; AA5: "#1d252d"; AB4: "#1d252d"; AB5: "#1d252d" } } }
style.fill: "#101820"
classes: {
  kit_c0: { style.fill: "#1b313a"; style.stroke: "#67d5e8"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c0: { style.stroke: "#67d5e8"; style.font-color: "#67d5e8"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c1: { style.fill: "#2f2f2a"; style.stroke: "#ffc66d"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c1: { style.stroke: "#ffc66d"; style.font-color: "#ffc66d"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c2: { style.fill: "#212e3d"; style.stroke: "#91c4ff"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c2: { style.stroke: "#91c4ff"; style.font-color: "#91c4ff"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c3: { style.fill: "#23312d"; style.stroke: "#a5dc83"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c3: { style.stroke: "#a5dc83"; style.font-color: "#a5dc83"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c4: { style.fill: "#2f292d"; style.stroke: "#ff9784"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c4: { style.stroke: "#ff9784"; style.font-color: "#ff9784"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c5: { style.fill: "#2c302d"; style.stroke: "#e5d482"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c5: { style.stroke: "#e5d482"; style.font-color: "#e5d482"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c6: { style.fill: "#1d3133"; style.stroke: "#75d9b0"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c6: { style.stroke: "#75d9b0"; style.font-color: "#75d9b0"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c7: { style.fill: "#272f39"; style.stroke: "#c0cce0"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c7: { style.stroke: "#c0cce0"; style.font-color: "#c0cce0"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c8: { style.fill: "#2d2b2b"; style.stroke: "#f2ad78"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c8: { style.stroke: "#f2ad78"; style.font-color: "#f2ad78"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c9: { style.fill: "#1f2f37"; style.stroke: "#86cad4"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c9: { style.stroke: "#86cad4"; style.font-color: "#86cad4"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c10: { style.fill: "#26302a"; style.stroke: "#bcd16d"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c10: { style.stroke: "#bcd16d"; style.font-color: "#bcd16d"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c11: { style.fill: "#262c3c"; style.stroke: "#b7b5fb"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c11: { style.stroke: "#b7b5fb"; style.font-color: "#b7b5fb"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c12: { style.fill: "#2b2d31"; style.stroke: "#e2bda5"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c12: { style.stroke: "#e2bda5"; style.font-color: "#e2bda5"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c13: { style.fill: "#1e2c3b"; style.stroke: "#7eb5ef"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c13: { style.stroke: "#7eb5ef"; style.font-color: "#7eb5ef"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c14: { style.fill: "#293231"; style.stroke: "#d4dfa4"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c14: { style.stroke: "#d4dfa4"; style.font-color: "#d4dfa4"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c15: { style.fill: "#282f36"; style.stroke: "#c5c9cb"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c15: { style.stroke: "#c5c9cb"; style.font-color: "#c5c9cb"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_data: { style.fill: "#1b313a"; style.stroke: "#67d5e8"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_data: { style.stroke: "#67d5e8"; style.font-color: "#67d5e8"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_control: { style.fill: "#2f2f2a"; style.stroke: "#ffc66d"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_control: { style.stroke: "#ffc66d"; style.font-color: "#ffc66d"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_identity: { style.fill: "#212e3d"; style.stroke: "#91c4ff"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_identity: { style.stroke: "#91c4ff"; style.font-color: "#91c4ff"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_success: { style.fill: "#23312d"; style.stroke: "#a5dc83"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_success: { style.stroke: "#a5dc83"; style.font-color: "#a5dc83"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_failure: { style.fill: "#2f292d"; style.stroke: "#ff9784"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_failure: { style.stroke: "#ff9784"; style.font-color: "#ff9784"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_warning: { style.fill: "#2c302d"; style.stroke: "#e5d482"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_warning: { style.stroke: "#e5d482"; style.font-color: "#e5d482"; style.stroke-width: 3; style.stroke-dash: 3 }
  kit_storage: { style.fill: "#1d3133"; style.stroke: "#75d9b0"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_storage: { style.stroke: "#75d9b0"; style.font-color: "#75d9b0"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_inactive: { style.fill: "#272f39"; style.stroke: "#c0cce0"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_inactive: { style.stroke: "#c0cce0"; style.font-color: "#c0cce0"; style.stroke-width: 3; style.stroke-dash: 5 }
}

# Explicit group assignments
(start -> running)[0].style.stroke: "#a5dc83"
(start -> running)[0].style.stroke-width: 3
(start -> running)[0].style.font-color: "#a5dc83"
(start -> running)[0].style.stroke-dash: 0
(running -> running)[0].style.stroke: "#a5dc83"
(running -> running)[0].style.stroke-width: 3
(running -> running)[0].style.font-color: "#a5dc83"
(running -> running)[0].style.stroke-dash: 0
(running -> boundary)[0].style.stroke: "#67d5e8"
(running -> boundary)[0].style.stroke-width: 3
(running -> boundary)[0].style.font-color: "#67d5e8"
(running -> boundary)[0].style.stroke-dash: 0
(boundary -> retry)[0].style.stroke: "#ffc66d"
(boundary -> retry)[0].style.stroke-width: 3
(boundary -> retry)[0].style.font-color: "#ffc66d"
(boundary -> retry)[0].style.stroke-dash: 0
(retry -> running)[0].style.stroke: "#a5dc83"
(retry -> running)[0].style.stroke-width: 3
(retry -> running)[0].style.font-color: "#a5dc83"
(retry -> running)[0].style.stroke-dash: 0
(retry -> held)[0].style.stroke: "#ffc66d"
(retry -> held)[0].style.stroke-width: 3
(retry -> held)[0].style.font-color: "#ffc66d"
(retry -> held)[0].style.stroke-dash: 0
(held -> running)[0].style.stroke: "#a5dc83"
(held -> running)[0].style.stroke-width: 3
(held -> running)[0].style.font-color: "#a5dc83"
(held -> running)[0].style.stroke-dash: 0
(held -> success)[0].style.stroke: "#ffc66d"
(held -> success)[0].style.stroke-width: 3
(held -> success)[0].style.font-color: "#ffc66d"
(held -> success)[0].style.stroke-dash: 0
(success -> ended)[0].style.stroke: "#ff9784"
(success -> ended)[0].style.stroke-width: 3
(success -> ended)[0].style.font-color: "#ff9784"
(success -> ended)[0].style.stroke-dash: 0
(success -> idle)[0].style.stroke: "#c0cce0"
(success -> idle)[0].style.stroke-width: 3
(success -> idle)[0].style.font-color: "#c0cce0"
(success -> idle)[0].style.stroke-dash: 5
(success -> result)[0].style.stroke: "#75d9b0"
(success -> result)[0].style.stroke-width: 3
(success -> result)[0].style.font-color: "#75d9b0"
(success -> result)[0].style.stroke-dash: 3
(idle -> running)[0].style.stroke: "#a5dc83"
(idle -> running)[0].style.stroke-width: 3
(idle -> running)[0].style.font-color: "#a5dc83"
(idle -> running)[0].style.stroke-dash: 0
(idle -> retired)[0].style.stroke: "#e5d482"
(idle -> retired)[0].style.stroke-width: 3
(idle -> retired)[0].style.font-color: "#e5d482"
(idle -> retired)[0].style.stroke-dash: 3
(running -> ended)[0].style.stroke: "#ff9784"
(running -> ended)[0].style.stroke-width: 3
(running -> ended)[0].style.font-color: "#ff9784"
(running -> ended)[0].style.stroke-dash: 0
(idle -> ended)[0].style.stroke: "#ff9784"
(idle -> ended)[0].style.stroke-width: 3
(idle -> ended)[0].style.font-color: "#ff9784"
(idle -> ended)[0].style.stroke-dash: 0
(running -> running)[1].style.stroke: "#a5dc83"
(running -> running)[1].style.stroke-width: 3
(running -> running)[1].style.font-color: "#a5dc83"
(running -> running)[1].style.stroke-dash: 0
```

## Clear / compact / exit / resume sequence

```d2
# Source audit: ../1_boop-session-state-audit.md
# Main: hafley-rs 2376a81f; Instant 9fca010e; 2026-09-13
vars: { d2-config: { theme-id: 201; layout-engine: elk; pad: 32 } }
classes: {
  fact: { style.fill: "#153b43"; style.stroke: "#63c7cd"; style.font-color: "#e1f5f5" }
  good: { style.fill: "#203e2b"; style.stroke: "#79b98b"; style.font-color: "#e5f3e8" }
  gap: { style.fill: "#473819"; style.stroke: "#d5b66e"; style.font-color: "#fff0c5" }
  bad: { style.fill: "#462629"; style.stroke: "#cb888a"; style.font-color: "#ffe2e2" }
}

shape: sequence_diagram
user: "Operation"
wrapper: "Boop wrapper\nroute + carried trace"
harness: "Native harness"
store: "Boop store"
view: "Identity after event"
user -> harness: "fresh launch: process P1"
harness -> wrapper: "observed Session(A)"
wrapper -> store: "attach A -> T; write live(A,P1,pane)"
store -> view: "P1 / A / T"
user -> harness: "compact"
harness -> harness: "native compaction receipt\nlifecycle gate expects A unchanged"
harness -> view: "P1 / A / T\nno Compacted event in main wrapper enum"
user -> harness: "clear/new"
harness -> wrapper: "observed Session(B), B != A"
wrapper -> store: "A detached; bind B to P1\nattach B -> T if B has no prior trace"
store -> view: "P1 / B / T\nA retained in trace history"
user -> harness: "exit"
wrapper -> store: "detach B only if P1 still owns row\nclear door; retain B -> T"
store -> view: "no bound process / B / T"
user -> harness: "resume B: new process P2"
harness -> wrapper: "observed Session(B)"
wrapper -> store: "existing trace(B)=T\nwrite live(B,P2,pane)"
store -> view: "P2 / B / T"
user -> harness: "select previously known C"
harness -> wrapper: "observed Session(C)"
wrapper -> store: "existing trace(C)=U takes precedence\ndetach B; bind C"
store -> view: "P2 / C / U"

# D2 theme kit: midnight / categorical
vars: { d2-config: { theme-id: 200; theme-overrides: { N1: "#edf3f8"; N2: "#edf3f8"; N3: "#edf3f8"; N4: "#c0cce0"; N5: "#1d252d"; N6: "#1d252d"; N7: "#101820"; B1: "#67d5e8"; B2: "#67d5e8"; B3: "#67d5e8"; B4: "#1d252d"; B5: "#1d252d"; B6: "#1d252d"; AA2: "#ffc66d"; AA4: "#1d252d"; AA5: "#1d252d"; AB4: "#1d252d"; AB5: "#1d252d" } } }
style.fill: "#101820"
classes: {
  kit_c0: { style.fill: "#1b313a"; style.stroke: "#67d5e8"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c0: { style.stroke: "#67d5e8"; style.font-color: "#67d5e8"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c1: { style.fill: "#2f2f2a"; style.stroke: "#ffc66d"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c1: { style.stroke: "#ffc66d"; style.font-color: "#ffc66d"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c2: { style.fill: "#212e3d"; style.stroke: "#91c4ff"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c2: { style.stroke: "#91c4ff"; style.font-color: "#91c4ff"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c3: { style.fill: "#23312d"; style.stroke: "#a5dc83"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c3: { style.stroke: "#a5dc83"; style.font-color: "#a5dc83"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c4: { style.fill: "#2f292d"; style.stroke: "#ff9784"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c4: { style.stroke: "#ff9784"; style.font-color: "#ff9784"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c5: { style.fill: "#2c302d"; style.stroke: "#e5d482"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c5: { style.stroke: "#e5d482"; style.font-color: "#e5d482"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c6: { style.fill: "#1d3133"; style.stroke: "#75d9b0"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c6: { style.stroke: "#75d9b0"; style.font-color: "#75d9b0"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c7: { style.fill: "#272f39"; style.stroke: "#c0cce0"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c7: { style.stroke: "#c0cce0"; style.font-color: "#c0cce0"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c8: { style.fill: "#2d2b2b"; style.stroke: "#f2ad78"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c8: { style.stroke: "#f2ad78"; style.font-color: "#f2ad78"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c9: { style.fill: "#1f2f37"; style.stroke: "#86cad4"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c9: { style.stroke: "#86cad4"; style.font-color: "#86cad4"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c10: { style.fill: "#26302a"; style.stroke: "#bcd16d"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c10: { style.stroke: "#bcd16d"; style.font-color: "#bcd16d"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c11: { style.fill: "#262c3c"; style.stroke: "#b7b5fb"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c11: { style.stroke: "#b7b5fb"; style.font-color: "#b7b5fb"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c12: { style.fill: "#2b2d31"; style.stroke: "#e2bda5"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c12: { style.stroke: "#e2bda5"; style.font-color: "#e2bda5"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c13: { style.fill: "#1e2c3b"; style.stroke: "#7eb5ef"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c13: { style.stroke: "#7eb5ef"; style.font-color: "#7eb5ef"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c14: { style.fill: "#293231"; style.stroke: "#d4dfa4"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c14: { style.stroke: "#d4dfa4"; style.font-color: "#d4dfa4"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c15: { style.fill: "#282f36"; style.stroke: "#c5c9cb"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c15: { style.stroke: "#c5c9cb"; style.font-color: "#c5c9cb"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_data: { style.fill: "#1b313a"; style.stroke: "#67d5e8"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_data: { style.stroke: "#67d5e8"; style.font-color: "#67d5e8"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_control: { style.fill: "#2f2f2a"; style.stroke: "#ffc66d"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_control: { style.stroke: "#ffc66d"; style.font-color: "#ffc66d"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_identity: { style.fill: "#212e3d"; style.stroke: "#91c4ff"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_identity: { style.stroke: "#91c4ff"; style.font-color: "#91c4ff"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_success: { style.fill: "#23312d"; style.stroke: "#a5dc83"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_success: { style.stroke: "#a5dc83"; style.font-color: "#a5dc83"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_failure: { style.fill: "#2f292d"; style.stroke: "#ff9784"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_failure: { style.stroke: "#ff9784"; style.font-color: "#ff9784"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_warning: { style.fill: "#2c302d"; style.stroke: "#e5d482"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_warning: { style.stroke: "#e5d482"; style.font-color: "#e5d482"; style.stroke-width: 3; style.stroke-dash: 3 }
  kit_storage: { style.fill: "#1d3133"; style.stroke: "#75d9b0"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_storage: { style.stroke: "#75d9b0"; style.font-color: "#75d9b0"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_inactive: { style.fill: "#272f39"; style.stroke: "#c0cce0"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_inactive: { style.stroke: "#c0cce0"; style.font-color: "#c0cce0"; style.stroke-width: 3; style.stroke-dash: 5 }
}

# Explicit group assignments
(user -> harness)[0].style.stroke: "#67d5e8"
(user -> harness)[0].style.stroke-width: 3
(user -> harness)[0].style.font-color: "#67d5e8"
(user -> harness)[0].style.stroke-dash: 0
(harness -> wrapper)[0].style.stroke: "#67d5e8"
(harness -> wrapper)[0].style.stroke-width: 3
(harness -> wrapper)[0].style.font-color: "#67d5e8"
(harness -> wrapper)[0].style.stroke-dash: 0
(wrapper -> store)[0].style.stroke: "#67d5e8"
(wrapper -> store)[0].style.stroke-width: 3
(wrapper -> store)[0].style.font-color: "#67d5e8"
(wrapper -> store)[0].style.stroke-dash: 0
(store -> view)[0].style.stroke: "#67d5e8"
(store -> view)[0].style.stroke-width: 3
(store -> view)[0].style.font-color: "#67d5e8"
(store -> view)[0].style.stroke-dash: 0
(user -> harness)[1].style.stroke: "#ffc66d"
(user -> harness)[1].style.stroke-width: 3
(user -> harness)[1].style.font-color: "#ffc66d"
(user -> harness)[1].style.stroke-dash: 0
(harness -> harness)[0].style.stroke: "#ffc66d"
(harness -> harness)[0].style.stroke-width: 3
(harness -> harness)[0].style.font-color: "#ffc66d"
(harness -> harness)[0].style.stroke-dash: 0
(harness -> view)[0].style.stroke: "#ffc66d"
(harness -> view)[0].style.stroke-width: 3
(harness -> view)[0].style.font-color: "#ffc66d"
(harness -> view)[0].style.stroke-dash: 0
(user -> harness)[2].style.stroke: "#91c4ff"
(user -> harness)[2].style.stroke-width: 3
(user -> harness)[2].style.font-color: "#91c4ff"
(user -> harness)[2].style.stroke-dash: 0
(harness -> wrapper)[1].style.stroke: "#91c4ff"
(harness -> wrapper)[1].style.stroke-width: 3
(harness -> wrapper)[1].style.font-color: "#91c4ff"
(harness -> wrapper)[1].style.stroke-dash: 0
(wrapper -> store)[1].style.stroke: "#91c4ff"
(wrapper -> store)[1].style.stroke-width: 3
(wrapper -> store)[1].style.font-color: "#91c4ff"
(wrapper -> store)[1].style.stroke-dash: 0
(store -> view)[1].style.stroke: "#91c4ff"
(store -> view)[1].style.stroke-width: 3
(store -> view)[1].style.font-color: "#91c4ff"
(store -> view)[1].style.stroke-dash: 0
(user -> harness)[3].style.stroke: "#a5dc83"
(user -> harness)[3].style.stroke-width: 3
(user -> harness)[3].style.font-color: "#a5dc83"
(user -> harness)[3].style.stroke-dash: 0
(wrapper -> store)[2].style.stroke: "#a5dc83"
(wrapper -> store)[2].style.stroke-width: 3
(wrapper -> store)[2].style.font-color: "#a5dc83"
(wrapper -> store)[2].style.stroke-dash: 0
(store -> view)[2].style.stroke: "#a5dc83"
(store -> view)[2].style.stroke-width: 3
(store -> view)[2].style.font-color: "#a5dc83"
(store -> view)[2].style.stroke-dash: 0
(user -> harness)[4].style.stroke: "#ff9784"
(user -> harness)[4].style.stroke-width: 3
(user -> harness)[4].style.font-color: "#ff9784"
(user -> harness)[4].style.stroke-dash: 0
(harness -> wrapper)[2].style.stroke: "#ff9784"
(harness -> wrapper)[2].style.stroke-width: 3
(harness -> wrapper)[2].style.font-color: "#ff9784"
(harness -> wrapper)[2].style.stroke-dash: 0
(wrapper -> store)[3].style.stroke: "#ff9784"
(wrapper -> store)[3].style.stroke-width: 3
(wrapper -> store)[3].style.font-color: "#ff9784"
(wrapper -> store)[3].style.stroke-dash: 0
(store -> view)[3].style.stroke: "#ff9784"
(store -> view)[3].style.stroke-width: 3
(store -> view)[3].style.font-color: "#ff9784"
(store -> view)[3].style.stroke-dash: 0
(user -> harness)[5].style.stroke: "#e5d482"
(user -> harness)[5].style.stroke-width: 3
(user -> harness)[5].style.font-color: "#e5d482"
(user -> harness)[5].style.stroke-dash: 0
(harness -> wrapper)[3].style.stroke: "#e5d482"
(harness -> wrapper)[3].style.stroke-width: 3
(harness -> wrapper)[3].style.font-color: "#e5d482"
(harness -> wrapper)[3].style.stroke-dash: 0
(wrapper -> store)[4].style.stroke: "#e5d482"
(wrapper -> store)[4].style.stroke-width: 3
(wrapper -> store)[4].style.font-color: "#e5d482"
(wrapper -> store)[4].style.stroke-dash: 0
(store -> view)[4].style.stroke: "#e5d482"
(store -> view)[4].style.stroke-width: 3
(store -> view)[4].style.font-color: "#e5d482"
(store -> view)[4].style.stroke-dash: 0
```

## Active predicates

```d2
# Source audit: ../1_boop-session-state-audit.md
# Main: hafley-rs 2376a81f; Instant 9fca010e; 2026-09-13
vars: { d2-config: { theme-id: 201; layout-engine: elk; pad: 32 } }
classes: {
  fact: { style.fill: "#153b43"; style.stroke: "#63c7cd"; style.font-color: "#e1f5f5" }
  good: { style.fill: "#203e2b"; style.stroke: "#79b98b"; style.font-color: "#e5f3e8" }
  gap: { style.fill: "#473819"; style.stroke: "#d5b66e"; style.font-color: "#fff0c5" }
  bad: { style.fill: "#462629"; style.stroke: "#cb888a"; style.font-color: "#ffe2e2" }
}

direction: down
 evidence: "SAME LANE / SESSION\nseparate evidence sources" {class: fact}
 runtime: "runtime_snapshot" {
  pid: "Recorded PID present?" {shape: diamond}
  process: "yes: Live\nno: Dead\nno PID: Unknown"
  target: "Target's session name listed?" {shape: diamond}
  tmux: "yes: Live / no: Dead\nlisting failure: Inaccessible\nno target: Unmanaged"
  count: "active_agents\nprocess Live OR tmux Live" {class: good}
  pid -> process
  target -> tmux
  process -> count
  tmux -> count
 }
 graph: "session graph" {
  native: "Native node\nstored agent_live.status" {class: gap}
  shell: "Shell node\nany Live -> live\nelse any Dead -> dead\nelse stored status or unknown"
  pane: "Exact %pane alive\ncan override shell to live"
  shell -> pane
 }
 ui: "Instant graph" {
  recency: "local active\nlast_activity or started >= cutoff" {class: fact}
  filter: "active-only\nstate == live OR live descendant" {class: fact}
 }
 screenshot: "Instant tmux table\nattached = tmux client count > 0\nopen = frontend tab exists\nproc = command label" {class: gap}
 evidence -> runtime.pid
 evidence -> runtime.target
 evidence -> graph.native
 evidence -> ui.recency
 evidence -> screenshot
 runtime.process -> graph.shell
 runtime.tmux -> graph.shell
 graph.native -> ui.filter
 graph.pane -> ui.filter
 gap: "EXIT DISPLAY GAP\nwrapper writes detached or closed\nfinished_ts SQL considers only dead" {class: bad}
 graph.native -> gap

# D2 theme kit: midnight / categorical
vars: { d2-config: { theme-id: 200; theme-overrides: { N1: "#edf3f8"; N2: "#edf3f8"; N3: "#edf3f8"; N4: "#c0cce0"; N5: "#1d252d"; N6: "#1d252d"; N7: "#101820"; B1: "#67d5e8"; B2: "#67d5e8"; B3: "#67d5e8"; B4: "#1d252d"; B5: "#1d252d"; B6: "#1d252d"; AA2: "#ffc66d"; AA4: "#1d252d"; AA5: "#1d252d"; AB4: "#1d252d"; AB5: "#1d252d" } } }
style.fill: "#101820"
classes: {
  kit_c0: { style.fill: "#1b313a"; style.stroke: "#67d5e8"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c0: { style.stroke: "#67d5e8"; style.font-color: "#67d5e8"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c1: { style.fill: "#2f2f2a"; style.stroke: "#ffc66d"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c1: { style.stroke: "#ffc66d"; style.font-color: "#ffc66d"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c2: { style.fill: "#212e3d"; style.stroke: "#91c4ff"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c2: { style.stroke: "#91c4ff"; style.font-color: "#91c4ff"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c3: { style.fill: "#23312d"; style.stroke: "#a5dc83"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c3: { style.stroke: "#a5dc83"; style.font-color: "#a5dc83"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c4: { style.fill: "#2f292d"; style.stroke: "#ff9784"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c4: { style.stroke: "#ff9784"; style.font-color: "#ff9784"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c5: { style.fill: "#2c302d"; style.stroke: "#e5d482"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c5: { style.stroke: "#e5d482"; style.font-color: "#e5d482"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c6: { style.fill: "#1d3133"; style.stroke: "#75d9b0"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c6: { style.stroke: "#75d9b0"; style.font-color: "#75d9b0"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c7: { style.fill: "#272f39"; style.stroke: "#c0cce0"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c7: { style.stroke: "#c0cce0"; style.font-color: "#c0cce0"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c8: { style.fill: "#2d2b2b"; style.stroke: "#f2ad78"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c8: { style.stroke: "#f2ad78"; style.font-color: "#f2ad78"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c9: { style.fill: "#1f2f37"; style.stroke: "#86cad4"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c9: { style.stroke: "#86cad4"; style.font-color: "#86cad4"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c10: { style.fill: "#26302a"; style.stroke: "#bcd16d"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c10: { style.stroke: "#bcd16d"; style.font-color: "#bcd16d"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c11: { style.fill: "#262c3c"; style.stroke: "#b7b5fb"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c11: { style.stroke: "#b7b5fb"; style.font-color: "#b7b5fb"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c12: { style.fill: "#2b2d31"; style.stroke: "#e2bda5"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c12: { style.stroke: "#e2bda5"; style.font-color: "#e2bda5"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c13: { style.fill: "#1e2c3b"; style.stroke: "#7eb5ef"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c13: { style.stroke: "#7eb5ef"; style.font-color: "#7eb5ef"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c14: { style.fill: "#293231"; style.stroke: "#d4dfa4"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c14: { style.stroke: "#d4dfa4"; style.font-color: "#d4dfa4"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c15: { style.fill: "#282f36"; style.stroke: "#c5c9cb"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c15: { style.stroke: "#c5c9cb"; style.font-color: "#c5c9cb"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_data: { style.fill: "#1b313a"; style.stroke: "#67d5e8"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_data: { style.stroke: "#67d5e8"; style.font-color: "#67d5e8"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_control: { style.fill: "#2f2f2a"; style.stroke: "#ffc66d"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_control: { style.stroke: "#ffc66d"; style.font-color: "#ffc66d"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_identity: { style.fill: "#212e3d"; style.stroke: "#91c4ff"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_identity: { style.stroke: "#91c4ff"; style.font-color: "#91c4ff"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_success: { style.fill: "#23312d"; style.stroke: "#a5dc83"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_success: { style.stroke: "#a5dc83"; style.font-color: "#a5dc83"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_failure: { style.fill: "#2f292d"; style.stroke: "#ff9784"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_failure: { style.stroke: "#ff9784"; style.font-color: "#ff9784"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_warning: { style.fill: "#2c302d"; style.stroke: "#e5d482"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_warning: { style.stroke: "#e5d482"; style.font-color: "#e5d482"; style.stroke-width: 3; style.stroke-dash: 3 }
  kit_storage: { style.fill: "#1d3133"; style.stroke: "#75d9b0"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_storage: { style.stroke: "#75d9b0"; style.font-color: "#75d9b0"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_inactive: { style.fill: "#272f39"; style.stroke: "#c0cce0"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_inactive: { style.stroke: "#c0cce0"; style.font-color: "#c0cce0"; style.stroke-width: 3; style.stroke-dash: 5 }
}

# Explicit group assignments
runtime.(pid -> process)[0].style.stroke: "#67d5e8"
runtime.(pid -> process)[0].style.stroke-width: 3
runtime.(pid -> process)[0].style.font-color: "#67d5e8"
runtime.(pid -> process)[0].style.stroke-dash: 0
runtime.(target -> tmux)[0].style.stroke: "#ffc66d"
runtime.(target -> tmux)[0].style.stroke-width: 3
runtime.(target -> tmux)[0].style.font-color: "#ffc66d"
runtime.(target -> tmux)[0].style.stroke-dash: 0
runtime.(process -> count)[0].style.stroke: "#91c4ff"
runtime.(process -> count)[0].style.stroke-width: 3
runtime.(process -> count)[0].style.font-color: "#91c4ff"
runtime.(process -> count)[0].style.stroke-dash: 0
runtime.(tmux -> count)[0].style.stroke: "#a5dc83"
runtime.(tmux -> count)[0].style.stroke-width: 3
runtime.(tmux -> count)[0].style.font-color: "#a5dc83"
runtime.(tmux -> count)[0].style.stroke-dash: 0
graph.(shell -> pane)[0].style.stroke: "#ff9784"
graph.(shell -> pane)[0].style.stroke-width: 3
graph.(shell -> pane)[0].style.font-color: "#ff9784"
graph.(shell -> pane)[0].style.stroke-dash: 0
(evidence -> runtime.pid)[0].style.stroke: "#e5d482"
(evidence -> runtime.pid)[0].style.stroke-width: 3
(evidence -> runtime.pid)[0].style.font-color: "#e5d482"
(evidence -> runtime.pid)[0].style.stroke-dash: 0
(evidence -> runtime.target)[0].style.stroke: "#e5d482"
(evidence -> runtime.target)[0].style.stroke-width: 3
(evidence -> runtime.target)[0].style.font-color: "#e5d482"
(evidence -> runtime.target)[0].style.stroke-dash: 0
(evidence -> graph.native)[0].style.stroke: "#e5d482"
(evidence -> graph.native)[0].style.stroke-width: 3
(evidence -> graph.native)[0].style.font-color: "#e5d482"
(evidence -> graph.native)[0].style.stroke-dash: 0
(evidence -> ui.recency)[0].style.stroke: "#e5d482"
(evidence -> ui.recency)[0].style.stroke-width: 3
(evidence -> ui.recency)[0].style.font-color: "#e5d482"
(evidence -> ui.recency)[0].style.stroke-dash: 0
(evidence -> screenshot)[0].style.stroke: "#e5d482"
(evidence -> screenshot)[0].style.stroke-width: 3
(evidence -> screenshot)[0].style.font-color: "#e5d482"
(evidence -> screenshot)[0].style.stroke-dash: 0
(runtime.process -> graph.shell)[0].style.stroke: "#75d9b0"
(runtime.process -> graph.shell)[0].style.stroke-width: 3
(runtime.process -> graph.shell)[0].style.font-color: "#75d9b0"
(runtime.process -> graph.shell)[0].style.stroke-dash: 0
(runtime.tmux -> graph.shell)[0].style.stroke: "#c0cce0"
(runtime.tmux -> graph.shell)[0].style.stroke-width: 3
(runtime.tmux -> graph.shell)[0].style.font-color: "#c0cce0"
(runtime.tmux -> graph.shell)[0].style.stroke-dash: 0
(graph.native -> ui.filter)[0].style.stroke: "#f2ad78"
(graph.native -> ui.filter)[0].style.stroke-width: 3
(graph.native -> ui.filter)[0].style.font-color: "#f2ad78"
(graph.native -> ui.filter)[0].style.stroke-dash: 0
(graph.pane -> ui.filter)[0].style.stroke: "#86cad4"
(graph.pane -> ui.filter)[0].style.stroke-width: 3
(graph.pane -> ui.filter)[0].style.font-color: "#86cad4"
(graph.pane -> ui.filter)[0].style.stroke-dash: 0
(graph.native -> gap)[0].style.stroke: "#f2ad78"
(graph.native -> gap)[0].style.stroke-width: 3
(graph.native -> gap)[0].style.font-color: "#f2ad78"
(graph.native -> gap)[0].style.stroke-dash: 0
```

## Proposed identity model

```d2
# Source audit: ../1_boop-session-state-audit.md
# Main: hafley-rs 2376a81f; Instant 9fca010e; 2026-09-13
vars: { d2-config: { theme-id: 201; layout-engine: elk; pad: 32 } }
classes: {
  fact: { style.fill: "#153b43"; style.stroke: "#63c7cd"; style.font-color: "#e1f5f5" }
  good: { style.fill: "#203e2b"; style.stroke: "#79b98b"; style.font-color: "#e5f3e8" }
  gap: { style.fill: "#473819"; style.stroke: "#d5b66e"; style.font-color: "#fff0c5" }
  bad: { style.fill: "#462629"; style.stroke: "#cb888a"; style.font-color: "#ffe2e2" }
}

direction: right
 banner: "PROPOSED CONTRACT\nNot implemented\nReview alongside implemented ER diagram" {class: gap}
 process: "ProcessKey\nHOST + PID + START TIME" {class: fact}
 pane: "PaneKey\nTMUX SERVER + PANE ID" {class: fact}
 session: "SessionKey\nHARNESS + NATIVE ID" {class: fact}
 binding: "SessionBinding\nROUTE + GENERATION\nopened_at / closed_at\nunresolved / bound / detached / closed" {class: good}
 trace: "Trace\nTRACE ID" {class: fact}
 lineage: "LineageEvent\nfrom session / to session\nfresh / clear / resume / fork\nobserved timestamp" {class: good}
 turn: "Turn observation\nunknown / idle / running / waiting\nturn ID + reason + observed_at" {class: fact}
 residency: "Supervisor observation\nunknown / running turn / parked / retired" {class: fact}
 process -> binding: "1 process : many successive bindings"
 session -> binding: "1 session : many resume bindings"
 pane -> binding: "optional; qualified by server"
 trace -> lineage: "1 : many events"
 session -> lineage: "explicit from/to references"
 binding -> turn: "independent observation"
 binding -> residency: "independent observation"
 guard: "EVENT OWNERSHIP\nvalidate generation + producer sequence\nretain unknown on observation failure" {class: gap}
 guard -> binding: "stale events cannot replace newer owner"

# D2 theme kit: midnight / categorical
vars: { d2-config: { theme-id: 200; theme-overrides: { N1: "#edf3f8"; N2: "#edf3f8"; N3: "#edf3f8"; N4: "#c0cce0"; N5: "#1d252d"; N6: "#1d252d"; N7: "#101820"; B1: "#67d5e8"; B2: "#67d5e8"; B3: "#67d5e8"; B4: "#1d252d"; B5: "#1d252d"; B6: "#1d252d"; AA2: "#ffc66d"; AA4: "#1d252d"; AA5: "#1d252d"; AB4: "#1d252d"; AB5: "#1d252d" } } }
style.fill: "#101820"
classes: {
  kit_c0: { style.fill: "#1b313a"; style.stroke: "#67d5e8"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c0: { style.stroke: "#67d5e8"; style.font-color: "#67d5e8"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c1: { style.fill: "#2f2f2a"; style.stroke: "#ffc66d"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c1: { style.stroke: "#ffc66d"; style.font-color: "#ffc66d"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c2: { style.fill: "#212e3d"; style.stroke: "#91c4ff"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c2: { style.stroke: "#91c4ff"; style.font-color: "#91c4ff"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c3: { style.fill: "#23312d"; style.stroke: "#a5dc83"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c3: { style.stroke: "#a5dc83"; style.font-color: "#a5dc83"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c4: { style.fill: "#2f292d"; style.stroke: "#ff9784"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c4: { style.stroke: "#ff9784"; style.font-color: "#ff9784"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c5: { style.fill: "#2c302d"; style.stroke: "#e5d482"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c5: { style.stroke: "#e5d482"; style.font-color: "#e5d482"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c6: { style.fill: "#1d3133"; style.stroke: "#75d9b0"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c6: { style.stroke: "#75d9b0"; style.font-color: "#75d9b0"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c7: { style.fill: "#272f39"; style.stroke: "#c0cce0"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c7: { style.stroke: "#c0cce0"; style.font-color: "#c0cce0"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c8: { style.fill: "#2d2b2b"; style.stroke: "#f2ad78"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c8: { style.stroke: "#f2ad78"; style.font-color: "#f2ad78"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c9: { style.fill: "#1f2f37"; style.stroke: "#86cad4"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c9: { style.stroke: "#86cad4"; style.font-color: "#86cad4"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c10: { style.fill: "#26302a"; style.stroke: "#bcd16d"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c10: { style.stroke: "#bcd16d"; style.font-color: "#bcd16d"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c11: { style.fill: "#262c3c"; style.stroke: "#b7b5fb"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c11: { style.stroke: "#b7b5fb"; style.font-color: "#b7b5fb"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c12: { style.fill: "#2b2d31"; style.stroke: "#e2bda5"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c12: { style.stroke: "#e2bda5"; style.font-color: "#e2bda5"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c13: { style.fill: "#1e2c3b"; style.stroke: "#7eb5ef"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c13: { style.stroke: "#7eb5ef"; style.font-color: "#7eb5ef"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c14: { style.fill: "#293231"; style.stroke: "#d4dfa4"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c14: { style.stroke: "#d4dfa4"; style.font-color: "#d4dfa4"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_c15: { style.fill: "#282f36"; style.stroke: "#c5c9cb"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_c15: { style.stroke: "#c5c9cb"; style.font-color: "#c5c9cb"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_data: { style.fill: "#1b313a"; style.stroke: "#67d5e8"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_data: { style.stroke: "#67d5e8"; style.font-color: "#67d5e8"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_control: { style.fill: "#2f2f2a"; style.stroke: "#ffc66d"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_control: { style.stroke: "#ffc66d"; style.font-color: "#ffc66d"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_identity: { style.fill: "#212e3d"; style.stroke: "#91c4ff"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_identity: { style.stroke: "#91c4ff"; style.font-color: "#91c4ff"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_success: { style.fill: "#23312d"; style.stroke: "#a5dc83"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_success: { style.stroke: "#a5dc83"; style.font-color: "#a5dc83"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_failure: { style.fill: "#2f292d"; style.stroke: "#ff9784"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_failure: { style.stroke: "#ff9784"; style.font-color: "#ff9784"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_warning: { style.fill: "#2c302d"; style.stroke: "#e5d482"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_warning: { style.stroke: "#e5d482"; style.font-color: "#e5d482"; style.stroke-width: 3; style.stroke-dash: 3 }
  kit_storage: { style.fill: "#1d3133"; style.stroke: "#75d9b0"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_storage: { style.stroke: "#75d9b0"; style.font-color: "#75d9b0"; style.stroke-width: 3; style.stroke-dash: 0 }
  kit_inactive: { style.fill: "#272f39"; style.stroke: "#c0cce0"; style.font-color: "#edf3f8"; style.stroke-width: 2 }
  kit_edge_inactive: { style.stroke: "#c0cce0"; style.font-color: "#c0cce0"; style.stroke-width: 3; style.stroke-dash: 5 }
}

# Explicit group assignments
(process -> binding)[0].style.stroke: "#67d5e8"
(process -> binding)[0].style.stroke-width: 3
(process -> binding)[0].style.font-color: "#67d5e8"
(process -> binding)[0].style.stroke-dash: 0
(session -> binding)[0].style.stroke: "#ffc66d"
(session -> binding)[0].style.stroke-width: 3
(session -> binding)[0].style.font-color: "#ffc66d"
(session -> binding)[0].style.stroke-dash: 0
(pane -> binding)[0].style.stroke: "#91c4ff"
(pane -> binding)[0].style.stroke-width: 3
(pane -> binding)[0].style.font-color: "#91c4ff"
(pane -> binding)[0].style.stroke-dash: 0
(trace -> lineage)[0].style.stroke: "#a5dc83"
(trace -> lineage)[0].style.stroke-width: 3
(trace -> lineage)[0].style.font-color: "#a5dc83"
(trace -> lineage)[0].style.stroke-dash: 0
(session -> lineage)[0].style.stroke: "#ffc66d"
(session -> lineage)[0].style.stroke-width: 3
(session -> lineage)[0].style.font-color: "#ffc66d"
(session -> lineage)[0].style.stroke-dash: 0
(binding -> turn)[0].style.stroke: "#ff9784"
(binding -> turn)[0].style.stroke-width: 3
(binding -> turn)[0].style.font-color: "#ff9784"
(binding -> turn)[0].style.stroke-dash: 0
(binding -> residency)[0].style.stroke: "#ff9784"
(binding -> residency)[0].style.stroke-width: 3
(binding -> residency)[0].style.font-color: "#ff9784"
(binding -> residency)[0].style.stroke-dash: 0
(guard -> binding)[0].style.stroke: "#e5d482"
(guard -> binding)[0].style.stroke-width: 3
(guard -> binding)[0].style.font-color: "#e5d482"
(guard -> binding)[0].style.stroke-dash: 0
```
