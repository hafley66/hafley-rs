import "./0_model.tsp";
using Games;

const entries: Record<Entry> = #{
  `game-fighter`: #{
    stage: 2.7, destination: "library", task: "S3",
    scope: "Live statig grounded decisions and turn jump permission; crouch lifecycle migration and source-game timing equivalence pending",
    manifest: "crates/fighter/Cargo.toml", targets: #["crates/fighter"],
    evidence: #["4_tasks.md", "crates/fighter/4_graph.md", "crates/fighter/tests/1_chart.rs"],
  },
  `game-content`: #{
    stage: 2.7, destination: "library", task: "A1",
    scope: "Shared action types, offline decoder/baker and checked Rust transition-table emission; Falcon and Kirby fixtures",
    manifest: "crates/content/Cargo.toml", targets: #["crates/content"],
    evidence: #["crates/content/0_readme.md", "3_tasks.md"],
  },
  `game-ftcommon`: #{
    stage: 2.7, destination: "library", task: "S11",
    scope: "Generated C-to-Rust port of the pinned ftCommon movement callbacks; typed query/effect boundary plus deterministic fixtures against the emitted source rules. External engine calls and p_ftCommonData values stay explicit; no live consumer.",
    manifest: "crates/ftcommon/Cargo.toml", targets: #["crates/ftcommon"],
    evidence: #["crates/ftcommon/src/generated/0_ftcommon.rs", "crates/ftcommon/tests/1_source_rules.rs", "crates/content/src/5_source.rs"],
  },
  redux: #{
    stage: 3, destination: "library", task: "A1",
    scope: "Reducer composition and snapshot-owned statechart qualification",
    manifest: "crates/redux/Cargo.toml", targets: #["crates/redux"],
    evidence: #["shared/0_reuse.md", "blender-godot-sqlite-proof/falcon-lab/109_tasks.md"],
  },
  `game-input`: #{
    stage: 3, destination: "library", task: "A1",
    scope: "Snapshot-owned button buffering; held and repressed input proofs. Browser last-pressed direction resolution and first-connected generic joypad mapping stay Godot-shell adapters, not this crate; no GameCube-specific adapter exists.",
    manifest: "crates/input/Cargo.toml", targets: #["crates/input"],
    evidence: #["shared/0_reuse.md", "blender-godot-sqlite-proof/falcon-lab/109_tasks.md"],
  },
  rollback: #{
    stage: 3, destination: "library", task: "A1",
    scope: "GGRS adapter and full-state replay qualification harness; isolated synctest restore and replay pass. Live native-vs-WASM match and netplay remain unqualified.",
    manifest: "crates/rollback/Cargo.toml", targets: #["crates/rollback"],
    evidence: #["shared/0_reuse.md", "blender-godot-sqlite-proof/falcon-lab/109_tasks.md"],
  },
  `game-capture`: #{
    stage: 3, destination: "library", task: "A1",
    scope: "Reusable wgpu recording, labels and presentation repetition",
    manifest: "crates/capture/Cargo.toml", targets: #["crates/capture"],
    evidence: #["shared/0_reuse.md", "blender-godot-sqlite-proof/falcon-lab/109_tasks.md"],
  },
  `core-labs`: #{
    stage: 2.7, destination: "split", task: "A1",
    scope: "Collision, physics rollback and SQLite proofs; optional Ozz remains unqualified",
    manifest: "blender-godot-sqlite-proof/core-labs/Cargo.toml",
    targets: #["crates/physics", "crates/presentation", "smash"],
    evidence: #["blender-godot-sqlite-proof/18_results.md"],
  },
  `trace-gpu-host`: #{
    stage: 2.7, destination: "split", task: "A1",
    scope: "Trace rendering host; separate reusable renderer from executable setup",
    manifest: "blender-godot-sqlite-proof/render-lab/Cargo.toml",
    targets: #["crates/presentation", "smash"],
    evidence: #["blender-godot-sqlite-proof/18_results.md"],
  },
  `falcon-lab`: #{
    stage: 2.7, destination: "split", task: "S5",
    scope: "Falcon importer, boundary tooling and observed live phase graph; complete legal-edge graph and promotion split pending",
    manifest: "blender-godot-sqlite-proof/falcon-lab/Cargo.toml",
    targets: #["crates/content", "crates/presentation", "smash"],
    evidence: #["4_tasks.md", "blender-godot-sqlite-proof/falcon-lab/109_tasks.md"],
  },
  falcon_web: #{
    stage: 2.7, destination: "split", task: "Q3",
    scope: "Godot web adapter and app composition; keyboard and synthetic touch proofs exist. Cross-target match, physical devices and online play unqualified; last deploy receipt predates current source.",
    manifest: "blender-godot-sqlite-proof/falcon-lab/94_web/Cargo.toml",
    targets: #["crates/presentation", "smash"],
    evidence: #["blender-godot-sqlite-proof/falcon-lab/88_live_results.md"],
  },
  smash: #{
    stage: 2.7, destination: "app", task: "A1",
    scope: "22-action Falcon catalog and PM numeric ingest; shared locomotion and restore tests; keyboard, touch and first-joypad adapters live in the Godot shell; UI route ownership not yet extracted; full callbacks and finite stage pending",
    manifest: "smash/Cargo.toml",
    targets: #["smash"], evidence: #["smash/0_readme.md", "3_tasks.md"],
  },
  `game-ui`: #{
    stage: 1, destination: "library", task: "U1",
    scope: "Proposed pure Redux-compatible URL/statechart router from the proven Location/Nav/NavCmd model: route history, query-like dialog gate, URI encode/decode and host-neutral view projection. egui-kit is reused as renderer helpers, never UI authority. No crate or manifest exists at stage 1.",
    targets: #["crates/ui"],
    evidence: #["4_tasks.md", "2_roadmap.md"],
  },
};
