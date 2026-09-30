// The comparative bench: pinned repos, targets, tools and scored rows (bench.db).
// Field names are the column names in SQL, JSON and every runner.
namespace Bench;

enum LabKind { recon, bench }
enum Operation { rename, cleave }
enum Status { pass, correct_refusal, tool_failed, typecheck_failed, unexpected_files, timeout, unavailable }
enum Expect { normal, collision_expected, cross_package_refusal }
enum Topology { single_package, monorepo_many_packages, rust_workspace }
enum Runtime { rust, node, mcp, python }

/** One pinned checkout. A new commit is a new repo row. */
model Repo {
  @Entity.pk id: int64;
  @Entity.unique(Repo.name, Repo.commit_sha) name: string;
  purl: string;
  url: string;
  commit_sha: string;
  topology: Topology;
  baseline_command: string;
  /** Scored only at 0; a repo with baseline errors is refused when added. */
  baseline_errors: int32;
  baseline_seconds: float64;
}

model Target {
  @Entity.pk id: int64;
  @Entity.unique(Target.repo, Target.operation, Target.file, Target.name, Target.at_byte) repo: string;
  operation: Operation;
  file: string;
  name: string;
  at_byte: int64;
  dest: string | null;
  dest_new: boolean;
  cross_package: boolean;
  expect: Expect;
  /** JSON array of repo-relative paths the oracle's edit touches. */
  allowed_files: string;
}

/** ryi_fast, ryi_slow, rust_analyzer, ts_oracle, serena, tokensave. */
model Adapter {
  @Entity.pk id: int64;
  @Entity.unique(Adapter.name) name: string;
  runtime: Runtime;
}

model Lab {
  @Entity.pk id: int64;
  @Entity.unique(Lab.name) name: string;
  kind: LabKind;
  brief: string;
  /** The model a recon lab runs on; empty for a scored bench. */
  `model`: string;
}

/** One scored attempt. A rebuilt tool has a new bin_sha, so its rows sit beside the old ones. */
model Row {
  @Entity.pk id: int64;
  @Entity.unique(Row.lab, Row.repo, Row.operation, Row.tool, Row.target, Row.bin_sha) lab: string;
  repo: string;
  operation: Operation;
  tool: string;
  /** file#name@at_byte */
  target: string;
  /** sha256 of the tool binary; empty for tools ryi does not build. */
  bin_sha: string;
  status: Status;
  /** The first compiler error or the tool's refusal, verbatim. */
  cause: string | null;
  files_touched: int32;
  allowed_files: int32;
  seconds: float64;
  timeout_seconds: float64;
  after_errors: int32;
}
