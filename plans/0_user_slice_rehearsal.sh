#!/bin/bash
# Rehearse version 39 only on the named backup COPY inside this lane.
set -euo pipefail
lane=$(cd "$(dirname "$0")/.." && pwd -P)
: "${BOOP_BIN:?Set BOOP_BIN to the locally built boop}"
source /Users/chrishafley/projects/boop2-user/tests/0_sandbox.bash
export SB="$lane/scratch"
mkdir -p "$SB/home" "$SB/t" "$SB/mail" "$SB/config"
export HOME="$SB/home" BOOP_READER_HOME="$SB/home" BOOP_DB="$SB/rehearsal.db"
export BOOP_MAIL_DIR="$SB/mail" BOOP_CONFIG="$SB/config/config.json" BOOP_NO_SYNC=1
export BOOP_CODEX_STATE_DB="$SB/home/state_5.sqlite" BOOP_CLAUDE_SESSIONS_DIR="$SB/home/claude"
export CODEX_HOME="$SB/home/codex" PI_CODING_AGENT_DIR="$SB/home/omp" TMUX_TMPDIR="$SB/t"
unset TMUX TMUX_PANE
sandbox_guard
for artifact in "$BOOP_DB" "$BOOP_DB-wal" "$BOOP_DB-shm"; do
  if [ -e "$artifact" ]; then rm "$artifact"; fi
done
cp "$REAL_HOME/backups/boop/boop-20261001-162007.db" "$BOOP_DB"
# The backup has a WAL header but no sidecars. Materialize it only on the copy.
sqlite3 "$BOOP_DB" 'PRAGMA journal_mode=DELETE;' >/dev/null
counts() {
  for table in agent_favorite agent_tag agent_tag_link mood agent_turn_comment agent_turn_comment_target agent_turn_comment_fork; do
    printf '%s\t%s\n' "$table" "$(sqlite3 -readonly "$BOOP_DB" "SELECT count(*) FROM $table")"
  done
}
bodies() {
  sqlite3 -readonly -separator $'\t' "$BOOP_DB" 'SELECT favorite_id,hex(CAST(body AS BLOB)) FROM agent_favorite JOIN markdown_cache USING(markdown_id) ORDER BY favorite_id' |
  while IFS=$'\t' read -r favorite_id body_hex; do
    digest=$(printf '%s' "$body_hex" | xxd -r -p | shasum -a 256)
    printf '%s\t%s\n' "$favorite_id" "${digest%% *}"
  done
}
metadata() {
  sqlite3 -readonly -separator $'\t' "$BOOP_DB" "SELECT favorite_id,markdown_id,quote(note),hex($1),created_ts FROM agent_favorite ORDER BY favorite_id"
}
: > "$SB/tables-before.sha256"
: > "$SB/tables-after.sha256"
counts > "$SB/counts-before.tsv"
bodies > "$SB/bodies-before.tsv"
metadata source > "$SB/metadata-before.tsv"
sqlite3 -readonly -separator $'\t' "$BOOP_DB" "SELECT CASE WHEN source='' THEN 'empty' WHEN substr(source,1,instr(source,':')-1) IN ('turn','codex','session','agent_session','turn-range') THEN substr(source,1,instr(source,':')-1) ELSE 'other' END AS kind,count(*) FROM agent_favorite GROUP BY kind ORDER BY kind" > "$SB/kinds-before.tsv"
sqlite3 -readonly -separator $'\t' "$BOOP_DB" 'SELECT mood.id,name.value,hex(template) FROM mood JOIN dict_mood_name name ON name.id=mood.name_id ORDER BY mood.id' > "$SB/moods-before.tsv"
for table in agent_tag agent_tag_link agent_turn_comment agent_turn_comment_target agent_turn_comment_fork; do
  sqlite3 -readonly "$BOOP_DB" ".dump $table" | shasum -a 256 >> "$SB/tables-before.sha256"
done
before_version=$(sqlite3 -readonly "$BOOP_DB" 'PRAGMA user_version')
# This missing-id edit opens the schema owner but changes no favorite.
if "$BOOP_BIN" db favorite edit 9223372036854775807 --note rehearsal > "$SB/rehearsal.log" 2>&1; then
  echo 'Expected the missing favorite error' >&2; exit 1
fi
grep -q 'Error: no favorite 9223372036854775807' "$SB/rehearsal.log"
sqlite3 "$BOOP_DB" 'PRAGMA journal_mode=DELETE;' >/dev/null
after_version=$(sqlite3 -readonly "$BOOP_DB" 'PRAGMA user_version')
[ "$before_version" = 38 ] && [ "$after_version" = 39 ]
counts > "$SB/counts-after.tsv"
bodies > "$SB/bodies-after.tsv"
metadata source_text > "$SB/metadata-after.tsv"
sqlite3 -readonly -separator $'\t' "$BOOP_DB" 'SELECT source_kind,count(*) FROM agent_favorite GROUP BY source_kind ORDER BY source_kind' > "$SB/kinds-after.tsv"
sqlite3 -readonly -separator $'\t' "$BOOP_DB" 'SELECT id,name,hex(template) FROM mood ORDER BY id' > "$SB/moods-after.tsv"
for table in agent_tag agent_tag_link agent_turn_comment agent_turn_comment_target agent_turn_comment_fork; do
  sqlite3 -readonly "$BOOP_DB" ".dump $table" | shasum -a 256 >> "$SB/tables-after.sha256"
done
for proof in counts bodies metadata moods tables; do
  case "$proof" in tables) suffix=sha256 ;; *) suffix=tsv ;; esac
  cmp "$SB/$proof-before.$suffix" "$SB/$proof-after.$suffix"
done
[ "$(sqlite3 -readonly "$BOOP_DB" "SELECT count(*) FROM sqlite_master WHERE name='dict_mood_name'")" = 0 ]
printf 'version\t%s\t%s\n' "$before_version" "$after_version"
cat "$SB/counts-after.tsv" "$SB/kinds-before.tsv" "$SB/kinds-after.tsv"
printf 'identical favorite body hashes\t%s\n' "$(wc -l < "$SB/bodies-after.tsv" | tr -d ' ')"
cp "$SB/bodies-after.tsv" "$lane/plans/2_user_slice_body_sha256.tsv"
