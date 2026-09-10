namespace Games;

// Project adaptation of TC39 stages. Advancement is an explicit decision.
alias Stage = 0 | 1 | 2 | 2.7 | 3 | 4;
alias Destination = "library" | "app" | "split" | "undecided";

model Entry {
  stage: Stage;
  destination: Destination;
  scope: string;
  task: string;
  // Existing code uses the Cargo package name as registry key.
  // Proposals may omit manifest only at stages 0 or 1.
  manifest?: string;
  targets: string[];
  evidence: string[];
}

alias Registry = Record<Entry>;

// Progress view. Author only dispositions that cannot be derived from the
// registry, the Cargo metadata or a fighter's retained ingest manifest.
alias Coverage = "qualified" | "partial" | "pending" | "unqualified" | "absent";

model Reference {
  label: string;
  path: string;
  note: string;
}

// One character's offline ingest. File list, hashes and frame counts stay in
// the fighter's own manifest; this record only locates it and its consumers.
model IngestSource {
  character: string;
  // Registry key owning the payload.
  package: string;
  task: string;
  manifest: string;
  // Untracked mirrors differ per checkout; the selection denominator stays unknown.
  catalogApi: string;
  tests: Reference[];
}

// Semantic disposition of one mechanic family. Axes are independent; no single
// percentage is derived from them.
model Mechanic {
  // Family name as written in crates/fighter/4_graph.md.
  family: string;
  package: string;
  task: string;
  inventory: Coverage;
  chart: Coverage;
  live: Coverage;
  restore: Coverage;
  fidelity: Coverage;
  evidence: string[];
  gate: string;
}

model Progress {
  ingest: IngestSource;
  mechanics: Record<Mechanic>;
  references: Reference[];
}
