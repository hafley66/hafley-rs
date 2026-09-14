# hafley-observe OTLP proof receipt

2026-09-14 loopback delivery proof:

- Collector: official `otelcol` v0.160.0 darwin-arm64
- Archive SHA-256: `a56143a40a2c205cdd63da4af0249f2534691785c14cdd2a0c532becb4335521`
- Endpoint: `http://127.0.0.1:4318/v1/traces`
- Executed test: `HAFLEY_OTLP_PROOF_ENDPOINT=http://127.0.0.1:4318/v1/traces BOOP_NO_SYNC=1 cargo test -p hafley-observe --features otlp -j 1 http_exporter_sends_resource_and_parent_child_spans -- --ignored`

The collector debug exporter received `service.name=hafley-observe-proof`,
`service.version=0.1.0-proof`, and two spans sharing one trace ID. The
`proof.child` parent ID equaled the `proof.parent` span ID. Exported attributes
included `tick=7` and `bodies=2`.

The collector bound only `127.0.0.1:4318`, received Ctrl-C after the proof, and
reported `Shutdown complete.` The downloaded archive was placed under a
temporary `/private/tmp` directory; no collector daemon or installation remains.
