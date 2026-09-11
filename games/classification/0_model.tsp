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

// Per-action status. The stable expected identity/order and the required
// observation axes are authored here. Membership, order, mapped phases, chart
// transitions and live animation selection are observed from the executable
// Rust export; payload hashes and frame counts come from the retained ingest
// manifest. Nothing in this model restates an observed value.
alias ActionAxis =
  "payload" | "catalog" | "phase" | "chart" | "live" | "restore" | "native" | "fidelity";

model ExpectedAction {
  id: int32;
  action: string;
  // Mechanic family name shared with Games.Mechanic.family.
  family: string;
}

// One character's status sources. Artifact paths locate the generated catalog
// (ordered membership and declared frame counts), baked frames, runtime role
// bindings and the retained ingest manifest that owns payload hashes. TypeSpec
// only locates those files; every observed value is read from them, and the
// generated catalog remains each character's action inventory authority.
model CharacterStatus {
  runtime: string;
  display: string;
  catalog: string;
  baked: string;
  roles: string;
  manifest: string;
}

model Status {
  expected: Record<ExpectedAction>;
  axes: ActionAxis[];
  characters: Record<CharacterStatus>;
}

// Stable interchange shape for source extractors. Values are emitted from
// syntax trees and pinned source revisions; TypeSpec does not author them.
alias SourceOp = "<" | "<=" | ">" | ">=" | "==" | "!=";

model SourceRef {
  repository: string;
  revision: string;
  path: string;
  line: uint32;
  symbol: string;
}

model SourceGuard {
  lhs: string;
  operator: SourceOp;
  rhs: string;
}

model SourceRule {
  from: string;
  event: string;
  to: string;
  guard: SourceGuard;
  source: SourceRef;
}

model SourceUnresolved {
  symbol: string;
  source: SourceRef;
  reason: string;
}

model SourceImport {
  rules: SourceRule[];
  unresolved: SourceUnresolved[];
}

// PortStep backbone: authored step intent. Receipts (observed revisions, hashes,
// counts and pass results) are emitted by tools into generated JSON; nothing
// here restates an observed value or a test outcome.
model StepArtifact {
  kind: string;
  // Repository-relative path; presence is checked by the tool.
  path: string;
}

model StepProof {
  name: string;
  // Recipe or command identity the tool executes; it emits the pass result.
  command: string;
}

model Step {
  id: string;
  task: string;
  scope: string;
  sourceKinds: string[];
  extractor: string;
  artifacts: StepArtifact[];
  consumer: string;
  gates: StepProof[];
  stage: Stage;
}

alias Steps = Record<Step>;

// Permanent source/runtime coverage join. The profile and mappings are the
// only authored claims; observations, counts and receipt status are generated
// by `15_coverage.mjs`.
alias PortAxis = "state mapping" | "transition/callback mapping" | "ordered guard qualification" | "live reachability" | "rollback" | "source fidelity";

model PortProfile {
  id: string;
  name: string;
  target: string;
  sourceInventory: string;
  runtimeInventory: string;
  sourceRevision: string;
  axes: PortAxis[];
}

model SourceRuntimeMapping {
  sourceId: string;
  runtimeState: string;
  reason: string;
  evidence: string[];
}

model CallbackRuntimeMapping {
  // Stable source association ID, not the parent callback ID. One source
  // callback can serve multiple state/action associations.
  sourceId: string;
  // Parent source callback identity is carried separately for provenance.
  sourceCallbackId: string;
  runtimeCallback: string;
  reason: string;
  evidence: string[];
}

model GuardRuntimeMapping {
  sourceId: string;
  runtimePartition: string;
  reason: string;
  evidence: string[];
}

model ExclusionRule {
  id: string;
  namePrefixes: string[];
  expectedMatches: int32;
  reason: string;
  evidence: string[];
}

model Port {
  profile: PortProfile;
  mappings: SourceRuntimeMapping[];
  callbackMappings: CallbackRuntimeMapping[];
  guardMappings: GuardRuntimeMapping[];
  exclusions: ExclusionRule[];
}
