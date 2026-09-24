#!/usr/bin/env bash
# Hourly via ~/Library/LaunchAgents/com.hafley.cleanup.plist. DRY=1 prints only.
set -uo pipefail

P=$HOME/projects
LOW_GB=${CLEANUP_LOW_GB:-50}
DRY=${DRY:-}
free_gb() { /bin/df -g / | awk 'NR==2{print $4}'; }

hour=$(date +%H)
low=0; (( $(free_gb) < LOW_GB )) && low=1
# Full pass once a day at 04h, or any hour the disk is low.
[[ -z $DRY && $low == 0 && $hour != 04 ]] && exit 0

age=14d; sweep_days=7
(( low )) && { age=3d; sweep_days=2; }

worktrees=()
for d in "$P"/*-wt "$P"/*/.boop-worktrees "$P"/*/.worktrees "$P"/*/.claude/worktrees "$HOME"/.agent/lanes; do
  [[ -d $d ]] && worktrees+=("$d")
done

echo "$(date -Iseconds) free=$(free_gb)G low=$low age=$age sweep=${sweep_days}d"
if [[ -n $DRY ]]; then
  kondo --dry-run --older "$age" "${worktrees[@]}"
  cargo sweep --recursive --dry-run --time "$sweep_days" "$P"
else
  kondo --all --quiet --older "$age" "${worktrees[@]}"
  cargo sweep --recursive --time "$sweep_days" "$P"
  cargo cache --autoclean
  pnpm store prune
fi
for r in "$P"/*/; do
  [[ -e $r/.git ]] && git -C "$r" worktree prune ${DRY:+--dry-run --verbose}
done
echo "$(date -Iseconds) free=$(free_gb)G"
