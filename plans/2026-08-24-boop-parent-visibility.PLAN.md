# boop: parent visibility, blocking push, command-surface cut

- [Incident evidence](#incident-evidence)
- [Defects](#defects)
- [Deliverable 1: `boop push`](#deliverable-1-boop-push)
- [Deliverable 2: `boop debug <lane>` answers "what went wrong"](#deliverable-2-boop-debug-lane)
- [Deliverable 3: command surface audit](#deliverable-3-command-surface-audit)
- [Receipts](#receipts)

## Incident evidence

Lane `feature-generic-graph-rxjs-renderers`, opencode, model `openrouter/deepseek/deepseek-v4-pro-0813`,
session `ses_fc9db2240ffeXH41Dc1Grco8l9`, cwd `hafley-rxjs/.boop-worktrees/feature/generic-graph-rxjs-renderers`,
parent `codex-0` (a codex lane row, `shell-3:0.0`).

| time (UTC) | mail id | from | kind | body | outcome |
|---|---|---|---|---|---|
| 23:39:38 | m-68b13cd9 | coordinator | dispatch | brief `/private/tmp/hafley-rxjs-generic-graph-brief.md` | lane started |
| 23:41:17 | m-656398fb | codex-0 | instruction | "run `boop tell-parent --kind yield` after each checkpoint" | no yield ever arrived |
| 23:53:45 | m-23989685 | codex-0 | instruction | "send update now for 672e02a" | no reply |
| 23:55:51 | m-46a71efb | codex-0 | instruction | "pause, report 3 SHAs" | no reply; `beep hail` printed nothing |

Ground truth found only by `git log` in the worktree: 4 commits (`e6afaa6`, `e32064d`, `6330971`, `4fe4266`).

What the parent had:

| probe | output | what the parent needed |
|---|---|---|
| `boop beep hail <lane> --body ...` | empty stdout | delivery outcome line (injected / queued / unreachable) |
| `boop beep ps <lane>` | pid 77752, rss 706 MB, cpu 0.0 | last turn ts, last commit, last tool call |
| `boop db chat list` | 123 turns, assistant/tool bodies empty | the lane's last 3 assistant turns as text |
| `boop beep lane get <lane>` | route JSON, no activity | same |
| `boop debug` | not tried by codex; codex could not find it | one verb that answers "what happened" |

`opencode run` takes its prompt from argv (skill law): mid-flight hails reach nothing, and `beep hail`
does not say so. That is the root defect: silent non-delivery.

## Defects

| id | defect | where |
|---|---|---|
| D1 | `beep hail` to an `opencode run` lane returns rc 0 with no output; row kind `agent_delivery` outcome not printed | `crates/boop/src/cli/mail.rs` run_hail |
| D2 | no blocking send: parent cannot wait for ack from a child in one command | `boop wait` exists but is a second verb with a pasted id |
| D3 | `boop db chat list` projects empty assistant/tool bodies for opencode sessions | `crates/boop-store` opencode ingest |
| D4 | `boop debug` not discoverable from `beep` family, not lane-scoped in help | `crates/boop/src/main.rs` |
| D5 | lane liveness = process alive only; no "last activity" column | `beep ps`, `beep lane list` |
| D6 | codex-as-parent: `codex-0` registered as `lane` kind with tmux target `shell-3:0.0`; tell-parent target resolution untested for codex parents | `crates/boop-harness/src/door/codex.rs` (dirty in tree), `lane::tell_parent_target` |

## Deliverable 1: `boop push`

```
boop push <lane|parent> --body TEXT [--timeout <s>] [--kind note|instruction]
```

Signature-first:

```rust
pub(crate) fn run_push(registry: &Registry, to: &str, body: &str, kind: Kind, timeout: Duration, mail_dir: Option<&Path>) -> Result<PushOutcome>
// 1. write mail row (same as run_hail)
// 2. deliver through door; print ONE line: `delivered <route> <transport> m-<id>` or `unreachable <route>: <reason>` (rc 2)
// 3. block: poll bus.ndjson for reply_to == id  OR  route turn-end event  OR  route death
// 4. exit 0 on ack/reply (print reply body), 124 on timeout, 3 on route death
// LAST stdout line is always the next command to run (debug <lane> on failure)
```

`tell-parent` gets `--wait` that routes through the same function. `boop wait` stays as the resume verb.
An `opencode run` lane (argv prompt) is refused by name at step 2 with rc 2 and the text
`opencode run lane: argv prompt, cannot receive mid-flight; use lane wait or re-dispatch -s <session>`.

## Deliverable 2: `boop debug <lane>`

Extend the existing `debug` verb to take a lane id and print, in order:

1. route: harness, model, session, cwd, parent, pid alive, last turn ts (from store), idle seconds
2. last 5 mail rows to/from the lane with delivery outcome
3. worktree: `git log -5 --oneline`, `git status --short` count
4. last 3 assistant turns text (needs D3 fixed) and last 3 tool calls
5. WARN/ERROR trail rows (current behaviour)

Empty section prints `none`, never blank.

## Deliverable 3: command surface audit

Current tree (measured with `--help`, 2026-08-24): 17 top-level verbs, `beep` 7, `beep lane` 11, `db` 16, `inbox` 2, `agent` 2.

Candidates the lane must defend or fold, one row each in the REPORT:

| verb | overlaps with | proposed |
|---|---|---|
| `beep hail` / `tell-parent` / `tell-children` / `beep message` / `wait` / `push` | five ways to send or wait on mail | `push` (send+wait), `tell-*` as sugar over push, `beep message ack` deleted (bulk-mark is not proof of anything) |
| `beep ps` / `beep pstree` / `beep lane list` / `db status` / `agent summary` / `agent sessions` | six liveness views | `beep lane list` with last-activity column; `db status` for cost; fold the rest |
| `db session` / `turn` / `chat` / `touch` / `command` / `fetch` / `skill` / `pr` / `span` / `edge` | 10 table dumps over `boop db "<sql>"` | keep `db chat`, `db usage`, `db sync`, `db status`; the rest are SQL |
| `beep lane run` / `patch` / `route` / `pane` / `get` | internal or introspection | `get` absorbs `route`+`pane --screen`; `run` hidden |
| `tui` / `codex` / `shell-init` / `me` / `agent register` | four ways to register a pane | one `boop tui <harness>` |
| `concatmap` / `host` | DL6 runtime, not agent bus | move to own binary or hide |
| `whoami` / `config` | keep | keep |

Rule for the lane: a verb survives only with a written one-line use that no other verb covers. Undefended verbs go in a `hidden = true` clap attribute in this PR and a deletion list in the REPORT; no deletions of code paths in this PR.

## Receipts

- `cargo build --release -p boop` clean, `cargo test -p boop -p boop-harness -p boop-store` green, paste counts.
- `boop push feature-prolog-rehome-dl6 --body ping --timeout 30` prints one delivery line and one exit line; paste both.
- `boop push <opencode-run-lane> ...` exits 2 with the argv-prompt text; paste.
- `boop debug feature-generic-graph-rxjs-renderers` prints all 5 sections; paste.
- `boop db chat list` for session `ses_fc9db2240ffeXH41Dc1Grco8l9` shows non-empty assistant text; paste 2 rows.
- REPORT at `TASKS/boop-parent-visibility.REPORT.md`: verb table with defend/fold per row.
