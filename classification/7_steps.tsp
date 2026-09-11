import "./0_model.tsp";
using Games;

// Authored PortStep intent only: step identity, required source kinds, the
// extractor identity, expected artifact kinds and paths, the consumer, the
// proof gates by recipe name and the explicit TC39 stage. Observed revisions,
// hashes, counts and pass results are emitted by `9_steps.mjs` into
// `10_steps.json`; nothing here restates one.
const steps: Steps = #{
  pigeonFtCommonSourceImport: #{
    id: "pigeonFtCommonSourceImport",
    task: "P1",
    scope: "Pigeon common transitions extracted from the pinned Melee decomp",
    sourceKinds: #["melee-decomp-syntax-trees@pinned-revision"],
    extractor: "smash-import -- pigeon (smash/Cargo.toml, feature ingest)",
    artifacts: #[
      #{ kind: "source-rules-json", path: "smash/src/fighters/pigeon/generated/2_source_rules.json" },
      #{ kind: "source-chart-d2", path: "smash/src/fighters/pigeon/generated/3_source_chart.d2" },
      #{ kind: "source-chart-svg", path: "smash/src/fighters/pigeon/generated/3_source_chart.svg" },
    ],
    consumer: "smash/src/3_import.rs writes the generated files; classification/8_status.mjs reads the rules JSON in its SOURCE RULES section",
    gates: #[
      #{ name: "source-rules-check", command: "just source-rules-check" },
    ],
    stage: 2,
  },
};
