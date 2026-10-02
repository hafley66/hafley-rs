# TS dogfood gate

Cases = defects D1-D25 in plans/2026-10-01-ryi-ts-utility.md (repo root plans/), one script per defect: `D01.sh` … `D25.sh`,
plus JSX/rtkq cases `J*.sh`. Each script runs in the corpus root, uses $RYII, $STATE, exits 0 when the measured
result equals the plan's "expected" column, and prints one summary line. Verification is the plan's method:
zero new `error TS` lines vs `.dogfood/base.<pkg>.txt`, tsserver parity via `.dogfood/tsrefs.cjs`.

    cargo build --release --bin ryii --features cli,ts-checker,typespec
    RYII=$PWD/target/release/ryii CORPUS=/path/outside/repo/rxjs-corpus dogfood/ts/run.sh
