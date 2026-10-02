---
created: 2026-10-02
updated: 2026-10-02
type: improvement
reporter: claude-375
status: open
priority: normal
epic: burndown-2026-10
labels: [boop, cli]
---

# boop CLI: no short flags, groups without args, typed fields

## Description

Removes x-clap-short (tag recent/search -n, job/lane revive -y), x-clap-args-conflicts-with-subcommands (beep, beep fork, db, db usage) and x-clap-subcommand-required (me, beep selection) from the tsp spec: group own-forms become leaves (boop db sql, boop beep send). Typed fields replace custom value labels: id: MessageId|LaneName (main.rs:219, :1804), since: int64|duration (:1585), env: Record<string> (:1297), target: LaneName (:1644), pr_base/merged_into: GitBranch (:1311, :1491). Breaking: boop db "<sql>" and boop beep <route> <body> appear in boop --help doctrine, agent-bus skill, lane briefs. Regenerate boop2 fixtures/help.
