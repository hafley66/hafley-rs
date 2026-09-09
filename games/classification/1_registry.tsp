import "./0_model.tsp";
using Games;

const entries: Record<Entry> = #{
  redux: #{
    stage: 3, destination: "library", task: "A1",
    scope: "Reducer composition and snapshot-owned statechart qualification",
    manifest: "shared/redux/Cargo.toml", targets: #["crates/redux"],
    evidence: #["shared/0_reuse.md", "blender-godot-sqlite-proof/falcon-lab/109_tasks.md"],
  },
  `game-input`: #{
    stage: 3, destination: "library", task: "A1",
    scope: "Snapshot-owned button buffering; held and repressed input proofs",
    manifest: "shared/input/Cargo.toml", targets: #["crates/input"],
    evidence: #["shared/0_reuse.md", "blender-godot-sqlite-proof/falcon-lab/109_tasks.md"],
  },
  rollback: #{
    stage: 3, destination: "library", task: "A1",
    scope: "GGRS adapter and full-state replay qualification harness",
    manifest: "shared/rollback/Cargo.toml", targets: #["crates/rollback"],
    evidence: #["shared/0_reuse.md", "blender-godot-sqlite-proof/falcon-lab/109_tasks.md"],
  },
  `game-capture`: #{
    stage: 3, destination: "library", task: "A1",
    scope: "Reusable wgpu recording, labels and presentation repetition",
    manifest: "shared/capture/Cargo.toml", targets: #["crates/capture"],
    evidence: #["shared/0_reuse.md", "blender-godot-sqlite-proof/falcon-lab/109_tasks.md"],
  },
  `falcon-simulation`: #{
    stage: 2.7, destination: "split", task: "Q2",
    scope: "Seven-action Falcon slice; shared mechanics and game policy need separation",
    manifest: "blender-godot-sqlite-proof/simulation-core/Cargo.toml",
    targets: #["crates/simulation", "smash"],
    evidence: #["blender-godot-sqlite-proof/falcon-lab/109_tasks.md"],
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
    stage: 2.7, destination: "split", task: "A1",
    scope: "Falcon importer, boundary tooling and app scenarios; promotion split pending",
    manifest: "blender-godot-sqlite-proof/falcon-lab/Cargo.toml",
    targets: #["crates/content", "crates/presentation", "smash"],
    evidence: #["blender-godot-sqlite-proof/falcon-lab/109_tasks.md"],
  },
  falcon_web: #{
    stage: 2.7, destination: "split", task: "Q3",
    scope: "Godot web adapter and app composition; cross-target match unqualified",
    manifest: "blender-godot-sqlite-proof/falcon-lab/94_web/Cargo.toml",
    targets: #["crates/presentation", "smash"],
    evidence: #["blender-godot-sqlite-proof/falcon-lab/88_live_results.md"],
  },
  smash: #{
    stage: 1, destination: "app", task: "A1",
    scope: "Walk, jump, crouch; directional attacks/specials; side smash, down tilt, nair/fair/bair",
    targets: #["smash"], evidence: #["smash/0_readme.md", "3_tasks.md"],
  },
};
