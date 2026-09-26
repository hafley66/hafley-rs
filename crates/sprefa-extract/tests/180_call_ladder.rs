//! Call edges over `fixtures/call_ladder`, with one row per caller site and target.
//! `f` = ryi fast, `s` = ryi slow over the committed rust-analyzer `index.scip`.

#![cfg(feature = "cli")]

use std::process::Command;

const LADDER: &str = "tests/fixtures/call_ladder";

const TABLE: &str = "
with f as (select distinct replace(caller_path, rtrim(caller_path, replace(caller_path, '/', '')), '') file,
                  caller_site_start site, caller_name caller,
                  replace(callee_path, rtrim(callee_path, replace(callee_path, '/', '')), '') target_file,
                  callee_name target from resolved_edge),
     s as (select distinct replace(caller_path, rtrim(caller_path, replace(caller_path, '/', '')), '') file,
                  caller_site_start site, caller_name caller,
                  replace(callee_path, rtrim(callee_path, replace(callee_path, '/', '')), '') target_file,
                  callee_name target from slow.resolved_edge),
     u as (select * from f union select * from s)
select group_concat(line, char(10)) from (
  select printf('%-12s %4d %-18s -> %s:%-10s %s%s', file, site, caller, target_file, target,
                iif((file, site, caller, target_file, target) in (select * from f), 'f', '-'),
                iif((file, site, caller, target_file, target) in (select * from s), 's', '-')) line
  from u order by file, site, caller, target)";

fn ryi(args: &[&str]) {
    let output = Command::new(env!("CARGO_BIN_EXE_ryi"))
        .args(args)
        .env("RUST_LOG", "off")
        .output()
        .unwrap();
    assert!(output.status.success(), "ryi {args:?}: {}", String::from_utf8_lossy(&output.stderr));
}

#[test]
fn call_ladder_fast_and_slow() {
    let scratch = tempfile::tempdir().unwrap();
    let fast = scratch.path().join("fast.db").to_string_lossy().into_owned();
    let slow = scratch.path().join("slow.db").to_string_lossy().into_owned();
    let src = format!("{LADDER}/src");
    let index = format!("{LADDER}/index.scip");
    ryi(&["fast", &src, "--sqlite", &fast]);
    ryi(&["slow", &src, "--root", LADDER, "--scip-index", &index, "--no-checker", "--sqlite", &slow]);

    let conn = rusqlite::Connection::open(&fast).unwrap();
    conn.execute("attach ?1 as slow", [&slow]).unwrap();
    let table: String = conn.query_row(TABLE, [], |row| row.get(0)).unwrap();
    assert_eq!(table, "\
_2_one.rs     104 free_call          -> _0_types.rs:free_zero  fs
_2_one.rs     166 inherent_call      -> _0_types.rs:ping       fs
_2_one.rs     225 trait_impl_call    -> _0_types.rs:act        fs
_2_one.rs     289 trait_object_call  -> _0_types.rs:act        fs
_2_one.rs     350 generic_call       -> _0_types.rs:act        fs
_2_one.rs     395 ufcs_call          -> _0_types.rs:act        fs
_2_one.rs     471 closure@468        -> _0_types.rs:free_zero  fs
_2_one.rs     471 closure_call       -> _0_types.rs:free_zero  f-
_2_one.rs     546 deref_call         -> _0_types.rs:ping       fs
_2_one.rs     581 reexport_call      -> _0_types.rs:free_zero  fs
_2_one.rs     668 macro_call         -> _0_types.rs:free_zero  f-
_3_many.rs     89 many_calls         -> _0_types.rs:free_zero  fs
_3_many.rs    106 many_calls         -> _0_types.rs:free_one   fs
_3_many.rs    123 many_calls         -> _0_types.rs:free_two   fs
_3_many.rs    143 many_calls         -> _0_types.rs:make       fs
_3_many.rs    158 many_calls         -> _0_types.rs:ping       -s
_4_nested.rs  114 inner              -> _0_types.rs:act        fs
_4_nested.rs  140 nested_calls       -> _0_types.rs:make       fs
_4_nested.rs  160 nested_calls       -> _4_nested.rs:inner      fs
_4_nested.rs  184 closure@181        -> _0_types.rs:free_zero  fs
_4_nested.rs  184 nested_calls       -> _0_types.rs:free_zero  f-");
}
