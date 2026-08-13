# boop

Cross-harness agent transcript reader. Drive agents with `beep`, read what they
did with `db`. Tails agent events from every harness on this machine (claude,
codex, kimi, opencode) as one stream, backed by a SQLite store at
`~/.agent/boop.db`.

## Usage

```
boop beep lane create --branch feature/<name> --brief <abs-path>
boop db "<sql>"
boop whoami
```

Run `boop --help` for the full usage contract.

## License

Dual licensed under MIT or Apache-2.0, at your option.
