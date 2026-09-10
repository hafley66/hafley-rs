default:
    @just --list

# Evaluate live grounded guards and render the colocated static chart.
ground-chart:
    cargo run --locked --offline -j2 --manifest-path crates/fighter/Cargo.toml --example ground_chart -- --write
    d2 --layout elk crates/fighter/5_ground_chart.d2 crates/fighter/5_ground_chart.svg

ground-chart-preview output="/private/tmp/falcon-ground-chart.png":
    d2 --layout elk crates/fighter/5_ground_chart.d2 {{quote(output)}}

# D2 0.7.1; commit source and rendered SVG together.
map:
    node classification/2_registry.mjs generate
    d2 fmt 1_roadmap.d2
    d2 --layout elk --pad 24 1_roadmap.d2 1_roadmap.svg

test-map:
    node classification/2_registry.mjs check
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
    node classification/2_registry.mjs status

tsp:
    node classification/2_registry.mjs generate

test:
    node --test classification/2_registry.test.mjs
    just test-map
