# Plan: commit-as-message (boop commit push)

Author: Fable Plan agent, 2026-09-11. Read-only survey of hafley-rs at the dirty `main` of that date.

## Verified facts that shape the plan

- HeadWatch is not in the tree today. Added in `8c33671` (Aug 24: `struct HeadWatch`, `commit <lane> <old>..<new> dirty=<n>` rows, `HEAD_REWOUND` const, a 5 s poll), deleted in `1e06475` (Sep 9: "the git HEAD poll fires on a reported command rather than a 5s clock", nothing replaced the poll). Survivors: `head_sha` `crates/boop-proc/src/supervise.rs:1532`, `dirty_count` `:1551`, `idle_body` `:1568` (turn-end yield rows carry `head=<sha>`), `yield_to_parent` `:1624` called from `:940`, `:1057`, `:1240`, per-turn `ToolCallFact`s collected at `:892`. `MessageKind::HeadRewound` exists at `crates/boop-store/src/bus.rs:83` with no producer. Help text (`crates/boop/src/cli/mod.rs:187`, `:327`), the skill (`~/projects/claude-research/skills/agent-bus/SKILL.md:107`, `:120`) and a test hand-writing a commit body under `kind=yield` (`crates/boop/tests/wait_mail.rs:565-612`) all describe the deleted code.
- Progress rows: `lane_progress_row` = `supervisor_row && !lane_end_row` (`bus.rs:197-199`); the ladder returns `Rung::MailboxOnly` for them before any route lookup (`crates/boop-proc/src/deliver.rs:491-496`); the drain skips them (`deliver.rs:828-830`).
- Door budget: `DoorBudget` env fields `deliver.rs:229-262`; `door_verdict` counts pushes per window and rejects a same-body replay (`deliver.rs:296-334`); `door_gate` writes `agent_door_blowout` (`deliver.rs:342-378`); DDL `crates/boop-store/src/ident.rs:3783`.
- Expectations: `expect_commit_subject` / `expect_commits_at_least` `crates/boop/src/cli/job.rs:841-842`, checked `supervise.rs:1343-1380`, result rewritten to rc=4.
- Native TUI: `run_native_tui` `crates/boop/src/cli/control.rs:237`, writes `kind: "coordinator"` at `:353`, drains held mail through the door every tick at `:495-501`.
- Supervisor outbound: `mail_parent` (`supervise.rs:1638`) appends and calls `deliver_outbound` (`:1668`) which walks `deliver_hail`; mood render `render_mail` `:311`, `mood_template` `:321`.
- Store: `SCHEMA_VERSION = 29` (`ident.rs:55`), migration ladder `ident.rs:947-990`, per-version DDL constants like `DOOR_BLOWOUT_SCHEMA` (`ident.rs:3775`). `children_of` `crates/boop-proc/src/lane.rs:444`; `CHILDREN_ALIAS` `crates/boop/src/cli/mail.rs:129`.

Line numbers were read before the 2026-09-11 drain-deadline edits to `deliver.rs`; re-check offsets near `:780-900`.

## 1. Table of contents

1. Table of contents
2. Sequence diagram
3. Type signatures with pseudo-code
4. Instance lifetimes
5. Storage layout, read/write sequence, uniqueness
6. Subscription model
7. Commit message conventions
8. Brief and skill changes
9. Test plan
10. Migration and what stays untested

## 2. Sequence diagram

```mermaid
sequenceDiagram
    participant W as worker harness (lane)
    participant S as supervisor run loop (supervise.rs:869)
    participant H as HeadWatch (new headwatch.rs)
    participant DB as boop.db
    participant L as ladder land() (deliver.rs:480)
    participant C as coordinator door (claude socket / codex queue / opencode)
    W->>W: git commit -m "subject" -m "Boop-Status: wip"
    W-->>S: ToolCallFact{title:"Bash git commit ..."} (:892)
    S->>H: nudge(); tick(now)
    H->>H: stat <gitdir>/logs/HEAD mtime changed
    H->>H: git rev-parse HEAD, merge-base --is-ancestor old new
    H-->>S: HeadMove::Advanced (burst opens, quiet 3s)
    S->>H: tick(now+3s) quiet elapsed
    H-->>S: CommitFacts{old..new, n, subject, status, ask, dirty}
    S->>DB: commit_subscribers(lane): parent + agent_commit_subscription rows
    loop each subscriber
        S->>DB: INSERT agent_mail kind=commit (request when blocked)
        S->>L: deliver_hail(row)
        L->>DB: commit_push_mode(subscriber, lane), commit_already_pushed(new)
        alt mode=door and not pushed
            L->>DB: door_gate (window count, cool-off)
            L->>C: door.deliver(render_mail(mood, body))
            L->>DB: transition accepted-by-harness; INSERT agent_commit_push
        else mode=mailbox or duplicate
            L->>DB: transition held-in-mailbox (MailboxOnly)
        end
    end
    S->>DB: UPSERT agent_lane_head(lane, new)
    C->>C: review turn: git -C <worktree> log -p old..new
    C-->>W: (optional) boop beep <lane> "<review>" only when something is wrong
```

## 3. Type signatures (Rust), pseudo-code as comments

New file `crates/boop-proc/src/headwatch.rs` (re-lands the deleted struct from `8c33671` with a stat gate instead of a clock).

```rust
pub enum CommitStatus { Wip, Done, Blocked }
pub struct CommitFacts { pub old: String, pub new: String, pub count: u32, pub subject: String,
    pub status: CommitStatus, pub ask: Option<String>, pub check: Option<String>, pub dirty: usize }
pub enum HeadMove { Advanced(CommitFacts), Rewound { old: String, new: String } }
struct Burst { first_old: String, latest: String, last_change: Instant, count: u32 }
pub struct HeadWatch { reflog: PathBuf, last_mtime: Option<SystemTime>, reported: Option<String>,
    burst: Option<Burst>, quiet: Duration, nudged: bool }

impl HeadWatch {
    pub fn new(cwd: &Path, reported: Option<String>, quiet: Duration) -> Self
    // reflog = git rev-parse --git-path logs/HEAD (worktrees keep their own); reported seeded
    // from agent_lane_head so a revive (supervise.rs:776) never re-reports the last commit.
    pub fn nudge(&mut self)
    // set nudged=true; the next tick forks git regardless of mtime (tool-call trigger).
    pub fn tick(&mut self, cwd: &Path, now: Instant) -> Option<HeadMove>
    // 1. if !nudged and stat(reflog).mtime == last_mtime: fall to step 4.
    // 2. head = git rev-parse HEAD; if head == reported and burst none: return None.
    // 3. if reported is Some and !descends_from(reported, head): return Rewound (no burst).
    //    else burst = open or extend (latest=head, count+=rev-list old..new, last_change=now).
    // 4. if burst.some and now - last_change >= quiet: take burst, read_commit_facts, reported=latest, return Advanced.
    // 5. None.
}
pub fn is_git_write(tool: &ToolCallFact) -> bool
// title matches /\bgit (commit|merge|rebase|cherry-pick|reset|am|revert)\b/ and status == "completed".
pub fn read_commit_facts(cwd: &Path, old: &str, new: &str, count: u32) -> CommitFacts
// git log -1 --format=%s%x00%(trailers:key=Boop-Status,valueonly)%x00%(trailers:key=Boop-Ask,valueonly)%x00%(trailers:key=Boop-Check,valueonly) new
// status of a burst = Blocked if any commit in old..new says blocked, else the latest's trailer, default Wip.
pub fn commit_body(lane: &str, worktree: &Path, f: &CommitFacts) -> String
// "commit {lane} {old}..{new} n={count} status={status} subject={subject:?} dirty={dirty}
//  review: git -C {worktree} log -p {old}..{new}" ; blocked adds " ask={ask:?}"; check adds " check={check:?}".
```

`crates/boop-store/src/bus.rs` (MessageKind at `:69`, predicates `:157-199`):

```rust
pub enum MessageKind { /* existing */ Commit, /* HeadRewound stays */ }   // wire "commit"
impl MessageKind {
    pub fn supervisor_row(&self) -> bool   // add Commit
    pub fn lane_progress_row(&self) -> bool // unchanged formula, so Commit is a progress row
    pub fn commit_row(&self) -> bool        // matches!(self, Commit)
}
```

`crates/boop-store/src/ident.rs` (beside `latest_door_blowout` `:2980`):

```rust
pub struct CommitSubscriptionRow { pub subscriber: String, pub lane: String, pub mode: String, pub created_at: String }
impl Store {
    pub fn commit_subscriptions_for_lane(&self, lane: &str) -> Result<Vec<CommitSubscriptionRow>>  // lane = ?1 OR lane = '*'
    pub fn commit_subscription(&self, subscriber: &str, lane: &str) -> Result<Option<String>>       // exact, then '*'
    pub fn set_commit_subscription(&self, row: &CommitSubscriptionRow) -> Result<()>               // INSERT OR REPLACE
    pub fn drop_commit_subscription(&self, subscriber: &str, lane: &str) -> Result<usize>
    pub fn commit_push_exists(&self, lane: &str, subscriber: &str, head: &str) -> Result<bool>
    pub fn record_commit_push(&self, lane: &str, subscriber: &str, head: &str, message_id: &str, at_ms: u64) -> Result<()>
    pub fn lane_reported_head(&self, lane: &str) -> Result<Option<String>>
    pub fn set_lane_reported_head(&self, lane: &str, head: &str, at_ms: u64) -> Result<()>
}
```

`crates/boop-proc/src/deliver.rs` (edit `land` at `:480-496`, drain at `:828-830`):

```rust
pub enum CommitPush { Door, Mailbox }
pub fn commit_push_mode(store: &Store, routes: &BTreeMap<String, Route>, subscriber: &str, lane: &str) -> CommitPush
// exact row > '*' row > default: Door when routes[subscriber].kind in {coordinator, native} or mode == acpx; Mailbox for lane parents and unknown routes.
fn land(...)
// if message.kind.commit_row():
//     head = tail of body's "a..b" token
//     if commit_push_mode(..) == Mailbox: return MailboxOnly("commit row; subscriber reads the mailbox")
//     if store.commit_push_exists(from, to, head): return MailboxOnly("commit {head} already pushed to {to}")
//     fall through to the existing door path (door_gate at :555 applies, render_mail at :558)
// elif message.kind.lane_progress_row(): MailboxOnly as today
// after a carried_the_body landing for a commit row: store.record_commit_push(..)  (inside deliver_hail_budgeted next to the ack at :474)
pub fn drain_route_held_mail_budgeted(...)
// skip progress rows EXCEPT commit rows whose mode is Door (they retry after a cool-off like hails)
```

`crates/boop-proc/src/supervise.rs` (run loop `:869-892`, turn end `:1057`, `mail_parent` `:1638`, `record_result` `:1441`):

```rust
const COMMIT_QUIET_ENV: &str = "BOOP_COMMIT_QUIET_SECS";   // default 3
pub const COMMIT: &str = "commit";
fn report_head_move(lane: &LaneRun, store: &Store, worktree: &Path, mv: HeadMove)
// Rewound => mail_to_parent_kind(HEAD_REWOUND, ...) as 8c33671 did (mailbox-only stays).
// Advanced(f) =>
//   kind = match f.status { Blocked => "request", _ => COMMIT }   // request already takes the door and ends a `boop wait <lane>`
//   for (subscriber, _) in commit_subscribers(store, lane): mail_parent(lane, subscriber, kind, commit_body(..), Some(status word))
//   store.set_lane_reported_head(lane, f.new)
// NOTE: not gated by has_answered (:1607); a lane that already mailed a result and keeps committing still reports.
fn commit_subscribers(store: &Store, lane: &LaneRun) -> Vec<String>
// registered parent (registered_parent) + every agent_commit_subscription row for lane or '*', deduped, self excluded.
// in the event loop after turn_tools.extend (:892): if turn_tools.iter().any(is_git_write) { head_watch.nudge() }
// every loop pass (mid-turn :892 and parked :1264): if let Some(mv) = head_watch.tick(&lane.cwd, now) { report_head_move(..) }
// record_result (:1441): detail gains " head=<sha> commits_past_base=<n>" so the result push carries the review handle.
```

`crates/boop/src/cli/job.rs` (`LaneCreateArgs` `:820-845`) and `crates/boop/src/cli/mod.rs` (agent subcommands):

```rust
pub(crate) commit_push: Option<String>,   // lane create --commit-push door|mailbox ; writes a (parent, lane) subscription row
// boop beep agent subscribe <lane|children|*> [--mode door|mailbox] [--as <me>]
// boop beep agent unsubscribe <lane|children|*> [--as <me>]
// `children` expands through lane::children_of (lane.rs:444) plus a '*' row scoped by parent edge check in commit_subscribers.
```

## 4. Instance lifetimes

| type | created | dropped | owner |
|---|---|---|---|
| `HeadWatch` | once in `run` next to `ParentWatch::new` (`supervise.rs:765`), seeded from `agent_lane_head` | supervisor process exit | run loop frame |
| `Burst` | first head change after quiet | quiet elapsed or rewind | `HeadWatch` |
| `CommitFacts` / `HeadMove` | one per emitted event | after rows are written | `report_head_move` frame |
| `DoorBudget` | per delivery from env (`deliver.rs:253`) | end of call | existing |
| `CommitSubscriptionRow` | subscribe verb or `--commit-push` | unsubscribe, `lane delete`, `agent done` (cascade delete on lane name) | store |
| `agent_commit_push` row | first door landing per (lane, subscriber, head) | `lane delete` of that lane | store |
| `agent_lane_head` row | first report | `lane delete`; `lane create` on a dead name resets it | store |

## 5. Storage layout, sequence, uniqueness

New DDL constant `COMMIT_PUSH_SCHEMA` (pattern of `DOOR_BLOWOUT_SCHEMA`, `ident.rs:3775`), included in `SCHEMA` for fresh stores and applied at `< 30` in the ladder (`ident.rs:979` region). `SCHEMA_VERSION = 30`, help text "writes version 30".

| table | columns | key |
|---|---|---|
| `agent_commit_subscription` | subscriber TEXT, lane TEXT ('*' allowed), mode TEXT CHECK IN ('door','mailbox'), created_at TEXT | PRIMARY KEY (subscriber, lane) |
| `agent_commit_push` | lane TEXT, subscriber TEXT, head TEXT, message_id TEXT, at_ms INTEGER | PRIMARY KEY (lane, subscriber, head); index (subscriber, at_ms) |
| `agent_lane_head` | lane TEXT, reported_head TEXT, at_ms INTEGER | PRIMARY KEY (lane) |

No column on `agent_mail` or `agent_route`; the row's `kind` is `commit`, `detail` is the status word, `body` carries the range.

Read/write sequence per event:

1. `HeadWatch::tick` reads `logs/HEAD` mtime (syscall, every 700 ms tick, `supervise.rs:15`), forks git only on change or nudge.
2. `report_head_move` reads `agent_route.parent` and `agent_commit_subscription` (one query).
3. Per subscriber: INSERT `agent_mail` + `appended` transition (`mail_parent`, `:1638`), then `deliver_hail`.
4. `land`: read subscription mode, read `agent_commit_push` for (lane, subscriber, new head); door path reads `agent_delivery_transition` window count and `agent_door_blowout` (`:296-334`).
5. On `carried_the_body`: ack the mail row (`:474`), INSERT `agent_commit_push`.
6. UPSERT `agent_lane_head`.

Uniqueness rules:

| rule | mechanism |
|---|---|
| one push per commit per subscriber | `agent_commit_push` PK on (lane, subscriber, head); `land` reads it before the door; drain retries cannot double-push |
| one push per burst | `Burst` coalesces every HEAD move inside `BOOP_COMMIT_QUIET_SECS` (default 3) into one row `a..d n=3`; the latest commit's trailers win, `blocked` anywhere in the range wins |
| one push per event, never commit + result | `Boop-Status: done` rows land `Mailbox` mode regardless of subscription; the result row (already door-taking, `bus.rs:183-192`) pushes seconds later with the head in its detail |
| cool-off | commit rows pass `door_gate` (`:555`) and count toward the window; a `CoolOff` landing leaves the row unacked and the drain retries it after `cooldown`; the same-body replay guard (`:322`) is satisfied because a re-drain re-uses the same row and a new commit has a new body |
| rewind | `Rewound` resets `reported` to the new head and writes `head_rewound` (mailbox-only as today); commits after a rewind are new shas so the PK does not block them |
| supervisor restart / revive | `reported` seeded from `agent_lane_head`, so the last commit is not reported twice |

## 6. Subscription model

| listener | how | default mode |
|---|---|---|
| parent | implicit, from `agent_route.parent` (registered by `lane create`, `job.rs:1496` region) | `door` when the parent is `kind=coordinator` (native TUI, `control.rs:353`), `native`, or `mode=acpx`; `mailbox` when the parent is a lane (its supervisor injects at a turn boundary anyway, `deliver.rs:502-509`) |
| explicit subscriber | `boop beep agent subscribe <lane> [--mode door\|mailbox] [--as <me>]` | `door` |
| all children | `boop beep agent subscribe children` writes one row per current child and a `'*'` row; `commit_subscribers` admits the `'*'` row only when `routes[lane].parent == subscriber` | `door` |
| opt out of pushes | `boop beep agent subscribe '*' --mode mailbox --as <me>` (a wildcard mailbox row beats the parent default); per lane at spawn: `lane create --commit-push mailbox` | n/a |
| opt back in | `boop beep agent unsubscribe '*'` or `subscribe <lane> --mode door` | n/a |

Law 9 text (`mod.rs:327`) changes to: "yield, head_rewound and done-status commit rows stay in the mailbox; a wip commit and a blocked commit take the door for subscribed routes (parent by default)". `boop debug <lane>` section 2 already prints the rung per row, so the mode is visible without a new verb.

## 7. Commit message conventions for workers

| element | rule | why |
|---|---|---|
| subject | `<area>: <what changed>`, one line, matches `--expect-commit-subject` when the brief names one | the push body and the completion assertion read the same string (`supervise.rs:1359`) |
| `Boop-Status: wip` | default when absent; a checkpoint the coordinator may review | pushes through the door |
| `Boop-Status: done` | the brief's deliverable is complete | mailbox only; the result row pushes when the turn ends |
| `Boop-Status: blocked` | the worker cannot proceed | minted as `kind=request`, so it takes the door and ends a `boop wait <lane>` (request already counts as an answer, `supervise.rs:1607-1616`) |
| `Boop-Ask: <one question>` | required with `blocked`; the coordinator's reply is `boop beep <lane> "<answer>"` | the worker never calls boop; an empty commit (`git commit --allow-empty`) carries the question |
| `Boop-Check: <command> -> <result line>` | optional validation receipt | replaces the herder receipt's `validation:` line (`~/projects/claude-research/skills/herder/SKILL.md:38-45`) |
| `Boop-Files:` | not needed; `git show --stat` answers it | keeps trailers to three |

Example blocked commit: `git commit --allow-empty -m "boop-store: commit push schema" -m "Boop-Status: blocked" -m "Boop-Ask: schema v30 or fold into agent_route?"`.

## 8. Brief and skill changes

| document | remove | add |
|---|---|---|
| worker briefs (pattern in `plans/boop-quiet-yields.BRIEF.md` "Receipt" and "Failure: STOP, boop beep ...") | every `boop beep --as <lane> <parent> "..."` receipt and failure line | "Report by committing. Trailers: Boop-Status wip/done/blocked, Boop-Ask on blocked, Boop-Check for validation. Never run boop." |
| `~/projects/claude-research/skills/agent-bus/SKILL.md:104-108` (Supervision) | "every HEAD move (commit a..b)" claim (deleted code) | the commit push table from section 6 and the trailer table from section 7 |
| `agent-bus/SKILL.md:66-80` (Send and wait) | `boop wait --me &` as the way to see lane progress | "commit pushes arrive as your next prompt; `boop wait --me` is for mailbox-mode subscribers only" |
| `~/projects/claude-research/skills/herder/SKILL.md:36-45` (Receipt) | "Send status before edits and at checkpoints" | receipt = commit trailers; herder reads `git -C <worktree> log -p a..b` from the push body |
| `~/projects/claude-research/skills/herder/SKILL.md:80-84` | "Read queued parent replies at each checkpoint" for workers | workers read hails only when the coordinator sends one |
| `boop --help` (`mod.rs:187`, `:327`) | law 9 wording and the WAIT block sentence "A wait is the ONLY way a lane's progress reaches you" | COMMIT PUSH block: subscribe verbs, trailer table, `BOOP_COMMIT_QUIET_SECS` |

## 9. Test plan

| case | input | expected | why |
|---|---|---|---|
| stat gate | reflog mtime unchanged, no nudge | zero git forks, `tick` returns None | the 5 s clock was the reason `1e06475` deleted HeadWatch |
| nudge | ToolCallFact title "Bash git commit -m x", status completed | git forked on next tick even with equal mtime | tool-call trigger path |
| single commit | one commit after spawn, quiet 0 | one `kind=commit` row `a..b n=1 status=wip subject="x"` to parent | base case (re-lands `a_commit_mails_the_parent_the_sha_range` from `8c33671`) |
| burst | three commits inside quiet window | one row `a..d n=3`, latest subject | coalescing |
| blocked | trailer `Boop-Status: blocked` + `Boop-Ask` | row kind `request`, body carries ask, `boop wait <lane>` exits on it | no worker boop call for "need input" |
| done | trailer `Boop-Status: done` | commit row lands `MailboxOnly`; result row detail names head | one push per event |
| rewind | reset --hard to base then commit | `head_rewound` row then a new commit row, no PK conflict | non-descendant HEAD |
| parent coordinator | parent route kind=coordinator with live door (mock TUI, `crates/boop/tests/shout_interrupt.rs` fixture) | transition `accepted-by-harness`, `agent_commit_push` row | door push |
| parent lane | parent route kind=lane | `MailboxOnly` | lane parents keep turn-boundary delivery |
| dedupe | drain re-walks a commit row already pushed | `MailboxOnly("already pushed")`, still one `agent_commit_push` row | one push per commit per subscriber |
| explicit subscriber | `agent subscribe <lane> --as obs` | two rows per commit (parent, obs), self never subscribed | fan-out |
| wildcard opt out | `subscribe '*' --mode mailbox --as parent` | parent commit rows `MailboxOnly` | opt-out path |
| cool-off | 33 commits inside 60 s at floor 32 | 33rd lands `cooled-off`, one blowout row, drain pushes it after cooldown | budget interaction |
| revive | supervisor restarts with `agent_lane_head` = current HEAD | no row on first tick | no double report |
| migration | v29 store opened by the build | three tables exist, `user_version` 30, old rows intact | ladder step |
| wait_mail test at `:565-612` | rewrite fixture kind from `yield` to `commit` | `wait --me` still prints it (mailbox mode) | keeps the mailbox path exercised under the new kind |

## 10. Migration and what stays untested

Migration: `SCHEMA_VERSION` 29 -> 30 (`ident.rs:55`); `COMMIT_PUSH_SCHEMA` applied at `< 30` in the ladder; no data backfill (no producer existed, so no commit rows exist to convert). Old `kind=yield` rows whose body starts with `commit ` (pre-`1e06475` stores) stay as they are. Install order: build, `cargo install --path crates/boop --force`, restart coordinator panes (the drain tick in `control.rs:495` runs in the old wrapper until then), then respawn lanes (a running supervisor has no HeadWatch).

Sequencing: (1) store schema + `MessageKind::Commit` + predicates; (2) `headwatch.rs` with unit tests against a temp repo (helper `git_repo` from `8c33671` test module); (3) supervisor wiring; (4) ladder + drain edits; (5) CLI subscribe verbs and `--commit-push`; (6) help, skill, briefs.

Untested by design:

- Real claude/codex/opencode doors: covered only by the mock TUI recipe; a live coordinator review turn is manual.
- Trailer parsing across git versions older than 2.35 (`%(trailers:key=...)` syntax).
- Coordinator behaviour on the push (whether it reviews well); that is prompt content, not boop.
- Worktrees on a filesystem without reliable mtime; the nudge path is the fallback and is tested.

## 11. Addendum: post-PR toggle and PR push (user ask, 2026-09-11)

A lane can be told to finish by opening a PR, and any PR a lane or coordinator opens is pushed to that route's subscribers once.

### Toggle

| surface | spelling | effect |
|---|---|---|
| lane create | `--post-pr [--pr-base <branch>]`, `--no-post-pr` | writes `post_pr` and `pr_base` into `spawn.json` so a revive keeps them |
| config | `post_pr = true` globally or per preset in `boop/config.json`; `pr_base` default `main` | the default when the flag is absent |
| supervisor | appends one closing line to the brief text it opens the conversation with | `When the deliverable is committed and validated: git push -u origin HEAD, then gh pr create --fill --base <pr-base>. The PR is your final report; boop tells your parent.` |

Default off: the toggle makes a lane push to `origin` and open a GitHub PR, an outward-facing act.

### Types, then pseudo-code

```rust
// crates/boop-store/src/bus.rs
MessageKind::Pr                     // wire "pr"; NOT a supervisor_row, so the ladder pushes it like a hail
pub fn lane_subscribers(store: &Store, lane: &str) -> Vec<String>
// parent (agent_route.parent) + agent_commit_subscription rows for lane, a '*' row only when that
// subscriber is the lane's parent; dedupe; never the lane. Moved here from supervise.rs
// commit_subscribers so the ingest path shares one definition; supervise.rs calls this.

// crates/boop-store/src/ident.rs
pub fn claim_pr_notice(&self, pr_url: &str, lane: &str, at_ms: u64) -> Result<bool>
// INSERT OR IGNORE INTO agent_pr_notice; Ok(changes() == 1)
pub fn notify_pr(&self, lane: &str, pr_url: &str, title: Option<&str>) -> Result<Vec<Message>>
// if !claim_pr_notice: return []; for s in lane_subscribers: append kind=pr row,
// body "pr {lane} {url} title={title:?}\n review: gh pr diff {url}"; return the rows
fn add_pr(&self, session, turn, pr_url) -> Result<bool>   // was Result<()>: true when the row is new
// projection caller (ident.rs:3602 region): on true, route = agent_route row whose session_id == session;
// if found, self.notify_pr(route, url, None). The rows stay held; the next drain pushes them.

// crates/boop-proc/src/supervise.rs
fn is_pr_create(tool: &ToolCallFact) -> bool      // completed call whose title holds `gh pr create`
fn pr_view(cwd: &Path) -> Option<(String, String)> // `gh pr view --json url,title`, bounded 10 s by wait-timeout; warn on timeout
// in the turn loop: if turn_tools.iter().any(is_pr_create) { if let Some((url, title)) = pr_view(cwd) {
//     for row in store.notify_pr(lane, &url, Some(&title))? { deliver_hail(row) } } }
// brief assembly: if post_pr { brief.push_str(&post_pr_line(pr_base)) }
```

### Instance lifetimes

| type | created | dropped |
|---|---|---|
| `agent_pr_notice` row | first producer to claim a PR url | `lane delete` (extend `drop_lane_commit_state`) |
| `kind=pr` mail rows | one per subscriber at claim | acked on door landing, else held until drained |
| `post_pr` / `pr_base` | `lane create` into `spawn.json` | lane delete |

### Storage, sequence, uniqueness

`agent_pr_notice (pr_url TEXT PRIMARY KEY, lane TEXT NOT NULL, at_ms INTEGER NOT NULL) WITHOUT ROWID`, schema v31, same ladder pattern as v30.

| producer | sequence |
|---|---|
| lane supervisor (fast) | tool fact `gh pr create` completed -> `gh pr view` -> `claim_pr_notice` -> append rows -> `deliver_hail` each (door) |
| transcript ingest (covers coordinators and native subagents) | sync projection -> `add_pr` inserted -> route by session_id -> `claim_pr_notice` -> append rows held -> next drain (any sync-carrying verb, or the TUI wrapper tick every 5 s, `control.rs` `DRAIN_EVERY`) pushes |

| rule | mechanism |
|---|---|
| one notice per PR across both producers | `agent_pr_notice` PK on `pr_url`; only the claimer appends rows |
| one row per subscriber per PR | rows appended once, at claim |
| PR updates after open | not notified; commit push covers new commits |

### Tests

| case | input | expected | why |
|---|---|---|---|
| toggle line | `lane create --post-pr --pr-base dev --dry-run` | prints `post-pr: dev`; the supervisor's opening text ends with the post-PR line | the toggle reaches the worker |
| config default | preset `post_pr = true`, no flag | line appended; `--no-post-pr` removes it | config path |
| supervisor producer | fake `gh` on PATH printing `{"url":"https://github.com/a/b/pull/7","title":"t"}`, tool fact `gh pr create` completed | one `kind=pr` row per subscriber, door landing | fast path |
| ingest producer | `add_pr` for a lane's session | one `kind=pr` row per subscriber, held | db path |
| both producers | supervisor then ingest for the same url | one notice, one row per subscriber | uniqueness |
| pr kind pushes | `kind=pr` row to a coordinator | not `MailboxOnly` | a pr row takes the door |
| gh hangs | fake `gh` sleeping 30 s | returns None within 10 s, WARN logged, turn continues | external effect is bounded |

## Critical files

- `crates/boop-proc/src/supervise.rs`
- `crates/boop-proc/src/deliver.rs`
- `crates/boop-store/src/bus.rs`
- `crates/boop-store/src/ident.rs`
- `crates/boop/src/cli/job.rs`
