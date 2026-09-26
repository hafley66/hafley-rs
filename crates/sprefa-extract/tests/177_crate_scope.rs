//! Every resolved edge out of `fixtures/crate_scope/app`: a bare name binds only
//! inside `app` and its path dependency `lib_a`, never `lib_b` or `app`'s fixture tree.

#![cfg(feature = "cli")]

use std::process::Command;

const TABLE: &str = "
with rows as (
  select 'type' plane, owner_path src, owner_name owner, kind, target_path dst, target_name target, resolution_origin origin
  from resolved_type_edge
  union all
  select 'call', caller_path, caller_name, kind, callee_path, callee_name, resolution_origin from resolved_edge)
select group_concat(line, char(10)) from (
  select printf('%-4s %-19s %-14s -> %-27s %s', plane, substr(src, instr(src, 'crate_scope/') + 12), owner,
                substr(dst, instr(dst, 'crate_scope/') + 12) || ':' || target, origin) line
  from rows where src like '%crate_scope/app/src/%' order by 1)";

#[test]
fn a_bare_name_binds_only_inside_the_crate_and_its_dependencies() {
    let scratch = tempfile::tempdir().unwrap();
    let db = scratch.path().join("scope.db").to_string_lossy().into_owned();
    let output = Command::new(env!("CARGO_BIN_EXE_ryi"))
        .args(["fast", "tests/fixtures/crate_scope", "--pattern", "*.rs", "--sqlite", &db])
        .env("RUST_LOG", "off")
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let conn = rusqlite::Connection::open(&db).unwrap();
    let table: String = conn.query_row(TABLE, [], |row| row.get(0)).unwrap();
    assert_eq!(
        table,
        "\
call app/src/lib.rs      calls       -> lib_a/src/lib.rs:only_a_fn  corpus_unique
call app/src/lib.rs      calls       -> lib_a/src/lib.rs:shared_fn  corpus_unique
type app/src/lib.rs      hidden_twin -> lib_a/src/lib.rs:Shared     corpus_unique
type app/src/lib.rs      one         -> lib_a/src/lib.rs:OnlyA      corpus_unique
type app/src/lib.rs      same_file_wins -> app/src/lib.rs:Twice        same_file"
    );
}
