#!/bin/sh
# Builds the throwaway `extract diff` fixture repo in $1 and prints the three
# commit shas on stdout, one per line.
#
# Identity, dates and the git config are all pinned: the shas are therefore
# identical on every machine, which is what lets the expected JSONL files carry
# them as literals.
set -eu

dir=${1:?usage: make_repo.sh DIR}
rm -rf "$dir"
mkdir -p "$dir"
cd "$dir"

export GIT_CONFIG_NOSYSTEM=1
export GIT_CONFIG_GLOBAL=/dev/null
export GIT_AUTHOR_NAME="extract diff fixture"
export GIT_AUTHOR_EMAIL="diff@example.invalid"
export GIT_COMMITTER_NAME="extract diff fixture"
export GIT_COMMITTER_EMAIL="diff@example.invalid"
export GIT_AUTHOR_DATE="2020-01-01T00:00:00+0000"
export GIT_COMMITTER_DATE="2020-01-01T00:00:00+0000"

git init -q -b main .

commit() {
    git -c commit.gpgsign=false add -A
    git -c commit.gpgsign=false commit -q -m "$1"
}

# commit 1: baseline. beta calls nothing; gamma is called by useGamma in c.ts.
cat >a.ts <<'EOF'
export function alpha(): number {
  return 1;
}
EOF
cat >b.ts <<'EOF'
export function beta(): number {
  return 2;
}
EOF
cat >c.ts <<'EOF'
export function gamma(): number {
  return 3;
}

export function useGamma(): number {
  return gamma();
}
EOF
cat >d.ts <<'EOF'
export const dvalue: number = 4;
EOF
commit baseline

# commit 2: beta gains a call to alpha (one edge added); gamma is renamed to
# delta (one edge removed, one added); b.ts and c.ts blobs both change.
cat >b.ts <<'EOF'
export function beta(): number {
  return alpha();
}
EOF
cat >c.ts <<'EOF'
export function delta(): number {
  return 3;
}

export function useGamma(): number {
  return delta();
}
EOF
commit "call alpha from beta, rename gamma to delta"

# commit 3: whitespace only in d.ts. The digest changes and no fact does.
cat >d.ts <<'EOF'
export const dvalue: number  =  4;
EOF
commit "whitespace only in d.ts"

git rev-parse HEAD~2
git rev-parse HEAD~1
git rev-parse HEAD
