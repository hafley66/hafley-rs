"""Compile ryi's TypeSpec contract through the alloy-rs emitter."""

import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile


CRATE = Path(__file__).resolve().parents[2]
TSP = Path(os.environ.get("HAFLEY_TSP", Path.home() / "projects/hafley-tsp")).expanduser().resolve()
RUST = TSP / "packages/rust"
EMITTER = RUST / "dist/emitter/06_on-emit.js"
FIXTURES = RUST / "test/fixtures"
DECORATOR = '"@hafley/typespec-decorator-def"'
STAGED_DECORATOR = '"../../../../decorator-def/lib/main.tsp"'


def main() -> None:
    if not EMITTER.is_file():
        raise SystemExit(f"missing {EMITTER}; build @hafley/alloy-rs in HAFLEY_TSP")
    out = Path(sys.argv[1] if len(sys.argv) > 1 else "src/bin/ryi/gen")
    if not out.is_absolute():
        out = CRATE / out
    with tempfile.TemporaryDirectory(prefix="ryi_contract_", dir=FIXTURES) as source_dir:
        source = Path(source_dir)
        for name in ("domain.tsp", "ops.tsp"):
            contract = (CRATE / "schema/cli" / name).read_text()
            (source / name).write_text(contract.replace(DECORATOR, STAGED_DECORATOR))
        with tempfile.TemporaryDirectory(prefix="ryi_generated_") as generated_dir:
            subprocess.run(
                ["pnpm", "exec", "tsp", "compile", str(source / "ops.tsp"),
                 "--emit", str(EMITTER), "--output-dir", generated_dir],
                cwd=TSP,
                env={**os.environ, "NODE_OPTIONS": "--max-old-space-size=2048"},
                check=True,
            )
            generated = Path(generated_dir)
            out.mkdir(parents=True, exist_ok=True)
            for name in ("cli_auto.rs", "ops_auto.rs", "http_auto.rs"):
                shutil.copy2(generated / name, out / name)
            shutil.copytree(generated / "models", out / "models", dirs_exist_ok=True)


if __name__ == "__main__":
    main()
