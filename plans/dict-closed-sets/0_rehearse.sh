#!/usr/bin/env bash
# Run from this lane after building the changed store and boop. All database
# handles below address copies under scratch; the backup is only copied.
set -euo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd -P)
proof=$root/plans/dict-closed-sets
scratch=$root/scratch
backup=${BOOP_REHEARSAL_BACKUP:-/Users/chrishafley/backups/boop/boop-20261001-162007.db}
: "${BOOP_BEFORE_BIN:?pre-change boop binary required}"
: "${BOOP_BIN:?schema 40 boop binary required}"
: "${STORE_TEST_BIN:?boop-store unit test executable required}"
: "${MIGRATE_BIN:?3_migrate executable required}"
db=$scratch/rehearsal.db
timing=$scratch/timing.db
mkdir -p "$scratch/home" "$scratch/mail" "$scratch/codex" "$scratch/claude"
trap 'rm -f "$db" "$db-wal" "$db-shm" "$timing" "$timing-wal" "$timing-shm" "$scratch"/open-before-* "$scratch"/open-after-*' EXIT
rm -f "$db" "$db-wal" "$db-shm"
cp "$backup" "$db"
[ "$(sqlite3 "$db" 'PRAGMA user_version')" = 38 ]
printf 'stage\tschema_version\n38\t38\n' > "$proof/2_versions.tsv"
# Snapshot grouped decoded values, including NULL, independently for every
# affected column. Table counts include every row, without dictionary joins.
snapshot() {
  local phase=$1 version=$2 table old dict new
  printf 'table\trows\n' > "$proof/3_counts_$phase.tsv"
  printf 'table\tcolumn\tdecoded_value\trows\n' > "$proof/4_values_$phase.tsv"
  while IFS=$'\t' read -r table old dict new; do
    [ "$table" = table ] && continue
    if [ "$version" -lt 40 ]; then
      sqlite3 -tabs "$db" "SELECT '$table','$new',quote(d.value),count(*) FROM $table t LEFT JOIN $dict d ON d.id=t.$old GROUP BY d.value ORDER BY d.value" >> "$proof/4_values_$phase.tsv"
    else
      sqlite3 -tabs "$db" "SELECT '$table','$new',quote($new),count(*) FROM $table GROUP BY $new ORDER BY $new" >> "$proof/4_values_$phase.tsv"
    fi
  done < "$proof/1_columns.tsv"
  local last=''
  while IFS=$'\t' read -r table old dict new; do
    [ "$table" = table ] && continue
    [ "$last" = "$table" ] && continue
    sqlite3 -tabs "$db" "SELECT '$table',count(*) FROM $table" >> "$proof/3_counts_$phase.tsv"
    last=$table
  done < "$proof/1_columns.tsv"
}
snapshot 38 38
session=$(sqlite3 "$db" 'SELECT session_id FROM agent_turn GROUP BY session_id HAVING count(*)>=100 LIMIT 1')
declare -A open_counts
# Open dictionaries cross both migrations with the same ids and full row values.
open_dicts=$(sqlite3 "$db" "SELECT name FROM sqlite_master WHERE type='table' AND name LIKE 'dict_%' AND name NOT IN ($(while IFS=$'\t' read -r table old dict new; do [ "$table" = table ] || printf "'%s'," "$dict"; done < "$proof/1_columns.tsv")'dict_mood_name') ORDER BY name")
printf 'table\tbefore_rows\tafter_rows\trow_multiset_equal\n' > "$proof/5_open_dicts.tsv"
for table in $open_dicts; do open_counts[$table]=$(sqlite3 "$db" "SELECT count(*) FROM $table"); sqlite3 -tabs "$db" "SELECT * FROM $table ORDER BY id" > "$scratch/open-before-$table"; done
BOOP_REHEARSAL_DB=$db "$STORE_TEST_BIN" --ignored --exact closed_sets_tests::rehearsal_schema39 > "$proof/6_stage39.log"
[ "$(sqlite3 "$db" 'PRAGMA user_version')" = 39 ]
printf '39\t39\n' >> "$proof/2_versions.tsv"
snapshot 39 39
cmp "$proof/3_counts_38.tsv" "$proof/3_counts_39.tsv"
cmp "$proof/4_values_38.tsv" "$proof/4_values_39.tsv"
"$MIGRATE_BIN" "$db" > "$proof/6_stage40.log"
[ "$(sqlite3 "$db" 'PRAGMA user_version')" = 40 ]
printf '40\t40\n' >> "$proof/2_versions.tsv"
snapshot 40 40
cmp "$proof/3_counts_39.tsv" "$proof/3_counts_40.tsv"
cmp "$proof/4_values_39.tsv" "$proof/4_values_40.tsv"
for table in $open_dicts; do
  sqlite3 -tabs "$db" "SELECT * FROM $table ORDER BY id" > "$scratch/open-after-$table"
  cmp "$scratch/open-before-$table" "$scratch/open-after-$table"
  after=$(sqlite3 "$db" "SELECT count(*) FROM $table")
  printf '%s\t%s\t%s\t1\n' "$table" "${open_counts[$table]}" "$after" >> "$proof/5_open_dicts.tsv"
done
# CLI invocation logging has its own isolated timing copy, so the preservation
# snapshots above contain only migration effects. Reader homes contain no sources.
run_boop() {
  env HOME="$scratch/home" BOOP_READER_HOME="$scratch/home" BOOP_DB="$timing" BOOP_MAIL_DIR="$scratch/mail" BOOP_CONFIG="$scratch/config.json" BOOP_CODEX_STATE_DB="$scratch/codex/state.db" BOOP_CLAUDE_SESSIONS_DIR="$scratch/claude" BOOP_NO_SYNC=1 "$@"
}
printf 'schema_version\tquery\trepetition\telapsed_ms\n' > "$proof/7_timings.tsv"
for version in 38 40; do
  rm -f "$timing" "$timing-wal" "$timing-shm"
  if [ "$version" = 38 ]; then cp "$backup" "$timing"; binary=$BOOP_BEFORE_BIN; role='r.value'; join='JOIN dict_role r ON r.id=t.role_id'; else cp "$db" "$timing"; binary=$BOOP_BIN; role='t.role'; join=''; fi
  for repetition in 1 2 3; do
    start=$EPOCHREALTIME
    run_boop "$binary" db search boop --limit 20 --format text > /dev/null
    finish=$EPOCHREALTIME
    elapsed=$(sqlite3 :memory: "SELECT round(($finish-$start)*1000,3)")
    printf '%s\tsearch\t%s\t%s\n' "$version" "$repetition" "$elapsed" >> "$proof/7_timings.tsv"
    start=$EPOCHREALTIME
    run_boop "$binary" db "SELECT t.turn,t.ts,$role AS role,t.said FROM agent_turn t $join WHERE t.session_id=$session ORDER BY t.turn LIMIT 100" --format text > /dev/null
    finish=$EPOCHREALTIME
    elapsed=$(sqlite3 :memory: "SELECT round(($finish-$start)*1000,3)")
    printf '%s\tagent_turn\t%s\t%s\n' "$version" "$repetition" "$elapsed" >> "$proof/7_timings.tsv"
  done
done
printf 'check\tresult\nrow_counts\tequal\ndecoded_value_multisets\tequal\nopen_dictionary_rows\tequal\nintegrity_check\t' > "$proof/8_checks.tsv"
sqlite3 "$db" 'PRAGMA integrity_check' >> "$proof/8_checks.tsv"
