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
    if len(sys.argv) > 1 and sys.argv[1]:
        staging = Path(sys.argv[1]).resolve()
        server_out = staging / "server"
        client_out = staging / "client"
        proto_out = staging / "proto"
    else:
        server_out = CRATE / "src/bin/ryi/gen"
        client_out = CRATE.parent / "ryi/src/gen"
        proto_out = CRATE.parent / "ryi-proto/src/gen"
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
            server_out.mkdir(parents=True, exist_ok=True)
            client_out.mkdir(parents=True, exist_ok=True)
            proto_out.mkdir(parents=True, exist_ok=True)
            for name in ("cli_auto.rs", "server_auto.rs"):
                shutil.copy2(generated / name, server_out / name)
            for name in ("cli_auto.rs", "client_auto.rs"):
                shutil.copy2(generated / name, client_out / name)
            for name in ("ops_auto.rs", "daemon_auto.rs"):
                shutil.copy2(generated / name, proto_out / name)
            if (proto_out / "models").exists():
                shutil.rmtree(proto_out / "models")
            shutil.copytree(generated / "models", proto_out / "models", dirs_exist_ok=True)


if __name__ == "__main__":
    main()
