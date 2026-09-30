#!/usr/bin/env bash
# usage: run.sh <name>  -- luna6 high recon: reads anything, writes only under recon/<name>/
R=~/.cache/lanes/claude-375/recon; mkdir -p $R/$1
cat $R/rules.md $R/$1.md > $R/$1/prompt.md
codex exec -m gpt-6-luna -c model_reasoning_effort=high -s workspace-write --skip-git-repo-check \
  -c sandbox_workspace_write.network_access=true -C $R/$1 -o $R/$1/FINAL.md "$(cat $R/$1/prompt.md)" < /dev/null > $R/$1/log.txt 2>&1
echo "$1 rc=$?"; head -c 1500 $R/$1/REPORT.md 2>/dev/null || cat $R/$1/FINAL.md
