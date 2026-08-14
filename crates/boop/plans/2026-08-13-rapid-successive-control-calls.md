# Rapid successive control calls obscure lane state

## Observed

While dispatching and checking `refactor-unified-row-type-ir`, the coordinator
had to issue these Boop calls in rapid succession:

1. `boop --help`
2. `boop beep lane create --help`
3. `boop beep lane create ... --dry-run`
4. `boop beep lane create ...`
5. retry `lane create` with elevated filesystem permission
6. `boop beep ps ...`
7. direct `boop db` schema probes after guessing unavailable status columns
8. `boop db status --help`
9. `boop db chat --help`
10. `boop beep lane route ...`
11. attempted transcript sync and chat lookup
12. `boop beep hail ...` for a status request

The first sandboxed `lane create` partially attempted registration and then
failed while creating the Git ref. The elevated retry succeeded. A later
sandboxed `ps` reported PID 0 because tmux was inaccessible; the elevated
retry showed the lane alive. The worktree contained no changes, and retrieving
the lane's current activity required separate route, database, and chat calls.

## Wanted

One command should return an authoritative lane status containing:

- process and tmux liveness, with permission failure distinct from dead;
- current harness session and last activity timestamp;
- worktree dirty state;
- latest assistant/tool activity or a stable transcript-tail reference;
- spawn phase and any partial-create failure;
- completion result when available.

`lane create` should be transactional across purpose registration, Git branch
and worktree creation, route registration, and process spawn. A failure should
either roll back completed steps or print exact recovery state and command.

## Reproduction

Run `lane create` where Boop can reach its own database but cannot write the
repository Git refs or tmux socket. Then call `beep ps` without tmux access.
The observed output can represent an inaccessible live lane as PID 0, forcing
an elevated retry and additional route/database queries.

## Acceptance

- A permission-denied tmux probe reports `inaccessible`, never `dead` or PID 0.
- A partially failed create leaves no branch, worktree, route, or purpose row,
  or reports each retained resource explicitly.
- `boop beep lane get <lane>` supplies the combined operational status above.
- One status call replaces the rapid sequence of `ps`, `route`, database
  schema discovery, chat lookup, and status hail.
