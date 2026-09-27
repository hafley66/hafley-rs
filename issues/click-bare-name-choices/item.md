---
created: 2026-09-25
updated: 2026-09-26
type: bug
status: fixed
priority: normal
---

# ⌘-click: a bare filename lists every reachable match (repo, worktrees incl. .boop-worktrees, trunk, siblings)

## Description

Owner 2026-09-25: ⌘-click on `index.html` in a terminal pane landed far from the intended file. The file existed only untracked in a boop lane worktree (hafley-rxjs/.boop-worktrees/chore/md-heading-rail-proto/plans/2026-09-25-md-heading-rail/index.html).

Expected: resolve from the clicked pane's pid cwd (boop-mux pane_at), then show the choices file-tree table of every `index.html` reachable from there: the cwd's git repo, its worktrees (git worktree list and .boop-worktrees, untracked files included), the main trunk checkout, and sibling repos. Rows grouped by root, each marked trunk / worktree <branch> / sibling.

Same gap as the sibling-repo tail match: rank_exact only reads the doc/cwd repo's index (crates/boop-harness/src/click/_2_ladder.rs:288), and sibling_candidates only joins <sibling root>/<token> (click/_0_rungs.rs:304). The worktree rung needs to include untracked files in .boop-worktrees.

## Comments

### 2026-09-25T14:39:17Z · @claude

880eb92d moves the fs lookup into boop-mux (cmd_click_lookup, no boop-* deps); 4b2b1f92: bare names choose across trunk + worktrees incl untracked .boop-worktrees; directory tails reach sibling repos. Tests: boop-mux 50, boop-harness 235 + 11 integration pass; instant cargo check clean. Open: bare names do not rank siblings (existing test pins Miss); a pane-local file still wins over choices; same-relative-path worktree rows are deduped.

## Acceptance

- [x] Bare filenames from the trunk list matches in the pane checkout, linked worktrees, ignored `.boop-worktrees`, and sibling repositories. A linked-worktree pane lists its own checkout set.
- [x] A pane-local exact path wins before cross-root choices.
- [x] Same checkout-relative files tracked in other worktrees are deduplicated.
- [x] Choices carry the root or worktree label for each returned path.

## Repro receipt

2026-09-26: `cbe1b551` made sibling repositories part of every bare-name search. The regression reproduced as `a_ref_in_a_document_resolves_from_the_document` returning two sibling paths instead of `Miss`, and `the_lookup_from_a_linked_worktree_needs_only_the_filesystem` including a sibling result. `exact_groups` now adds sibling groups only when the pane is at the trunk checkout; linked worktrees retain their own checkout set. `CARGO_TARGET_DIR=/Users/chrishafley/.cache/boop/cargo-target cargo nextest run -p boop-harness -p boop-mux -j 2`: 311 passed, 2 skipped, 0 failed.
