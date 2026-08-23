#!/bin/bash
# Live door check, plan §8 rows "claude door live" and "codex door live", plus
# opencode. One TUI per harness in a throwaway tmux session, launched through
# `boop tui` so the session exists before the TUI draws; the hail is the
# session's first prompt. No keystrokes are typed into any pane.
# Needs: tmux, the installed `boop`, `claude`, `codex`, `opencode`, and network.
# Usage: crates/boop/scripts/door-e2e.sh [claude codex opencode]
set -u
HARNESSES=${*:-claude codex opencode}
SESSION=boop-door-e2e-$$
tmux new-session -d -s "$SESSION" -c "$HOME/projects" 'sleep 1200'
pass=0; fail=0
for h in $HARNESSES; do
  extra=""
  if [ "$h" = codex ]; then
    # A codex thread is resumable only after a turn, and the daemon has no
    # handle on a TUI's thread before its first prompt; resume the newest
    # thread of this cwd so the hail is the next prompt, typed by nobody.
    thread=${BOOP_E2E_CODEX_THREAD:-$(sqlite3 -readonly "$HOME/.codex/state_5.sqlite" "select id from threads where archived=0 and cwd='$HOME/projects' order by created_at desc limit 1")}
    extra="-- resume $thread"
  fi
  tmux new-window -d -t "$SESSION" -n "$h" -c "$HOME/projects" "boop tui $h $extra; sleep 600"
  pane=$(tmux display -p -t "$SESSION:$h" '#{pane_id}' | tr -d %)
  route="$h-$pane"
  for _ in $(seq 1 40); do   # route appears once the TUI is up
    boop beep lane list 2>/dev/null | grep -q "^live $route " && break
    sleep 1
  done
  sleep 5
  # The hail blocks until the recipient's turn ends (door idle notice) or a
  # reply mail lands; the "turn ended" line is the push the caller gets.
  waited=$(boop beep hail "$route" --body "door receipt $h: reply with the single word ACK" --from door-e2e --wait-timeout 90 2>/dev/null | grep -E 'turn ended|->' | tail -1)
  echo "  wait: ${waited:-timed out}"
  for _ in $(seq 1 20); do
    tmux capture-pane -p -t "$SESSION:$h" | grep -v 'door receipt' | grep -qE '\bACK\b' && break
    sleep 1
  done
  outcome=$(sqlite3 -readonly "$HOME/.agent/boop.db" "select outcome from agent_delivery where route='$route' order by at_ms desc limit 1")
  if tmux capture-pane -p -t "$SESSION:$h" | grep -v 'door receipt' | grep -qE '\bACK\b' && [[ "$waited" == *"turn ended"* ]]; then
    echo "PASS $route ledger=$outcome"; pass=$((pass+1))
  else
    echo "FAIL $route ledger=$outcome"; tmux capture-pane -p -t "$SESSION:$h" | grep -v '^\s*$' | tail -8; fail=$((fail+1))
  fi
  tmux kill-window -t "$SESSION:$h"
  boop beep lane delete "$route" --route-only >/dev/null 2>&1
done
tmux kill-session -t "$SESSION"
echo "pass=$pass fail=$fail"
[ "$fail" = 0 ]
