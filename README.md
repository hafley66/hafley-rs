# hafley-rs

A Cargo workspace of reusable Rust crates.

## Crates

| crate | description |
|---|---|
| [`boop`](crates/boop) | cross-harness agent transcript reader: tail agent events from every harness on this machine as one stream |
| [`boop-mux`](crates/boop-mux) | the tmux multiplexer seam boop drives: one trait, one implementation over `tmux_interface` |

## License

Dual licensed under MIT or Apache-2.0, at your option. See [`LICENSE-MIT`](LICENSE-MIT) and [`LICENSE-APACHE`](LICENSE-APACHE).
