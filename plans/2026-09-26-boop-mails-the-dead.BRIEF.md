# Brief: bug/the-gang-mails-the-dead

Do `issues/boop-broadcast-mails-the-dead/item.md` in full (Want + Acceptance Criteria).

Rules:
- Load the `hafley-rs-repo` skill first. Commit scoped to your files; other agents have work staged on main.
- One golden test only (the one in the card). Delete narrower tests it supersedes in `crates/boop/tests/shout_*`; list the deletions in the commit message.
- Time cap: no command over 300s. `cargo test -p boop <name>`, never the whole workspace.
- Commit on the branch; do not push; do not merge.

Report (mail back), 3 lines: the liveness proof per route kind; the golden's stdout snapshot; commits.
