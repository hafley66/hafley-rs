# boop-types

Yaml, not code. Every document here is generated from a running boop or a live
store, never hand-edited.

## Documents

| file | holds | generator |
| --- | --- | --- |
| `cli.yaml` | OpenAPI 3.1 `paths`, 113 commands, 48 positionals, 284 options, every clap help string verbatim | `gen/cli_yaml.py` |
| `relational.yaml` | OpenAPI 3.1 schemas, 69 tables, 307 columns, 29 `dict_*` marked `x-boop-interned-by` | `gen/relational_yaml.py` |
| `queries.yaml` | one operation per SQL statement, 327 of them; inputs are the binds, outputs are the prepared columns | `gen/queries_yaml.py` |
| `sql.yaml` | 373 call sites by statement verb | `gen/boundaries_yaml.py` |
| `env.yaml` | 29 environment names with ops and owning files | `gen/boundaries_yaml.py` |
| `crates.yaml` | 8 crates, dependency edges, effects owned | `gen/boundaries_yaml.py` |
| `clusters.yaml` | per-table CRUD, rows, columns, grouped into families | `gen/er_d2.py` |
| `er.d2` | family to vocabulary ER board | `gen/er_d2.py` |

## Regenerate

From the repository root, with `boop` installed and `~/.agent/boop.db` present:

```bash
python3 crates/boop-types/gen/cli_yaml.py
python3 crates/boop-types/gen/relational_yaml.py
python3 crates/boop-types/gen/queries_yaml.py .
python3 crates/boop-types/gen/boundaries_yaml.py .
python3 crates/boop-types/gen/er_d2.py
```

## Gate

```bash
cd crates/boop-types && npx @redocly/cli lint
```

`redocly.yaml` turns off two rules that assume an HTTP server, each with its
reason inline: a CLI has no auth scheme, and a schema-only document has no
operations to reference its components.

## What the generators refuse to guess

`queries.yaml` output columns come from sqlite preparing the statement, because
`SELECT *` and expression aliases carry no column list in their text.

`relational_yaml.py` emits `x-boop-defect` when a column declares `x-dict` but
is not an integer, since an interned reference that is not an integer cannot
join its dictionary. That assertion found `agent_route.session_id` and
`dict_request`; both are written up in
`crates/boop/plans/2026-09-19-boop-store-schema-audit.md`.

## Not covered

34 SQL statements are built with `format!` and have no fixed text to prepare, so
they are absent from `queries.yaml`. Any claim that a table is never written is
bounded by that gap.
