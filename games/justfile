default:
    @just --list

# Evaluate live grounded guards and render the colocated static chart.
ground-chart:
    cargo run --locked --offline -j2 --manifest-path crates/fighter/Cargo.toml --example ground_chart -- --write
    d2 --layout elk crates/fighter/5_ground_chart.d2 crates/fighter/5_ground_chart.svg

ground-chart-preview output="/private/tmp/pigeon-ground-chart.png":
    d2 --layout elk crates/fighter/5_ground_chart.d2 {{quote(output)}}

# Extract source-backed Pigeon guards from the pinned Melee decomp.
source-rules:
    cargo run --locked --offline -j2 --manifest-path smash/Cargo.toml --features ingest --bin smash-import -- pigeon
    cargo run --locked --offline -j2 --manifest-path smash/Cargo.toml --features ingest --bin smash-import -- catalog
    cargo run --locked --offline -j2 --manifest-path smash/Cargo.toml --features ingest --bin smash-import -- attributes
    d2 --layout elk smash/src/fighters/pigeon/generated/3_source_chart.d2 smash/src/fighters/pigeon/generated/3_source_chart.svg
    d2 --layout elk smash/src/fighters/pigeon/generated/4_common_inventory.d2 smash/src/fighters/pigeon/generated/4_common_inventory.svg

source-rules-check:
    cargo run --locked --offline -j2 --manifest-path smash/Cargo.toml --features ingest --bin smash-import -- pigeon --check
    cargo run --locked --offline -j2 --manifest-path smash/Cargo.toml --features ingest --bin smash-import -- catalog --check
    cargo run --locked --offline -j2 --manifest-path smash/Cargo.toml --features ingest --bin smash-import -- attributes --check
    d2 validate smash/src/fighters/pigeon/generated/3_source_chart.d2
    d2 validate smash/src/fighters/pigeon/generated/4_common_inventory.d2

# Static port progress dashboard: classification/6_progress.html.
progress:
    node classification/5_progress.mjs generate

# PortStep intent and tool-emitted receipts: classification/10_steps.json.
steps:
    node classification/9_steps.mjs generate

# D2 0.7.1; commit source and rendered SVG together.
map:
    node classification/2_registry.mjs generate
    node classification/5_progress.mjs generate
    d2 fmt 1_roadmap.d2
    d2 --layout elk --pad 24 1_roadmap.d2 1_roadmap.svg

test-map:
    node classification/2_registry.mjs check
    node classification/5_progress.mjs check
    d2 fmt --check 1_roadmap.d2
    d2 validate 1_roadmap.d2
    node classification/2_registry.mjs check-svg

map-watch:
    node classification/2_registry.mjs generate
    d2 --watch --layout elk --pad 24 1_roadmap.d2 1_roadmap.svg

map-png:
    just map
    rsvg-convert -w 1500 1_roadmap.svg -o /tmp/hafley-games-roadmap.png

status:
    just source-rules-check
    node classification/8_status.mjs

tsp:
    node classification/2_registry.mjs generate

test:
    node --test classification/2_registry.test.mjs
    node --test classification/5_progress.test.mjs
    node --test classification/8_status.test.mjs
    node --test classification/9_steps.test.mjs
    just test-map
