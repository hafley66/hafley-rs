# tell-parent / tell-children

A child already carries its own identity and the edge that names its parent, so
neither end of that edge is worth spelling in a prompt.

```
boop tell-parent [--kind completion|yield|note] [--body TEXT]
boop tell-children --body TEXT
```

| step | where it comes from |
|---|---|
| the sender | the identity ladder (`boop whoami`): `BOOP_LANE`/`BOOP_SESSION`, else the registered pane, else the harness process |
| the recipient | the caller's registry route `parent`, written by `lane create --parent` and `agent register --parent` |
| the fallback | the one registered coordinator route, when the route records no parent |
| delivery | a harness-native session or child endpoint; lane mail remains in the durable mailbox for its supervisor |

`--kind` is the mail row's kind. `--body` is required for `completion` and
`note`. `yield` alone has a default, `yield <lane> rc=0 branch=<branch>
head=<sha>`, read from the route's worktree, and it is a turn boundary: the
lane stays alive and can be hailed again.

A caller the ladder cannot name, and a caller with no parent edge and no lone
coordinator, are each an error naming the caller and a non-zero exit. Neither
writes a row.

`tell-children` sends one body to every route registered as a child of the
caller and prints one line per target:

```
landed feature-a m-02be8593 (harness control)
no-route feature-b (unsupported child control)
```

A child is reachable when its parent session and typed child address are
registered. Missing, completed, and unsupported children retain their mailbox
row with a typed queued reason. A pane identifies a process for observation;
it never carries this mail.
