#!/usr/bin/env bash
# Decide whether this tree may be installed over ~/.cargo/bin/boop.
#
# Two conditions, both about naming the binary later: HEAD must already be on
# origin/main, and no tracked file may differ from HEAD. Untracked files are
# reported and allowed: they are not part of the commit's content, and refusing
# on them would make the rail something drivers route around.
#
# Exit 0 to allow, 1 to refuse with the reason on stderr. The repo is $1 so a
# test can drive this on a fixture.
set -uo pipefail

repo="${1:-$PWD}"

if ! git -C "$repo" rev-parse --git-dir >/dev/null 2>&1; then
  echo "install-guard: not a git repository: $repo" >&2
  exit 1
fi

tracked=$(git -C "$repo" status --porcelain --untracked-files=no)
if [ -n "$tracked" ]; then
  echo "install-guard: refusing, tracked files differ from HEAD:" >&2
  printf '%s\n' "$tracked" >&2
  echo "install-guard: commit or stash them, then install from a sha on origin/main" >&2
  exit 1
fi

head=$(git -C "$repo" rev-parse HEAD 2>/dev/null)
if [ -z "$head" ]; then
  echo "install-guard: refusing, $repo has no HEAD commit" >&2
  exit 1
fi

if ! git -C "$repo" rev-parse --verify --quiet origin/main >/dev/null; then
  echo "install-guard: refusing, $repo has no origin/main to check HEAD against" >&2
  exit 1
fi

if ! git -C "$repo" merge-base --is-ancestor HEAD origin/main; then
  echo "install-guard: refusing, HEAD $head is not an ancestor of origin/main" >&2
  echo "install-guard: merge the work first; an installed binary must name a commit on main" >&2
  exit 1
fi

untracked=$(git -C "$repo" ls-files --others --exclude-standard)
if [ -n "$untracked" ]; then
  echo "install-guard: note, untracked files present and ignored by this check:"
  printf '%s\n' "$untracked" | sed 's/^/  /'
fi

echo "install-guard: HEAD $head is on origin/main and no tracked file differs"
