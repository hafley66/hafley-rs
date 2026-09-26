//! TypeScript type and call edges across the fast and scip-typescript tiers.
//! `f` is fast, `s` is slow; regen.sh refreshes the committed SCIP index.

#![cfg(feature = "cli")]

use std::process::Command;

const LADDER: &str = "tests/fixtures/ts_ladder";

const TABLE: &str = "
with f as (
  select distinct replace(owner_path, rtrim(owner_path, replace(owner_path, '/', '')), '') file,
    case when kind = 'impl' then 'heritage' else kind end kind, owner_name owner,
    replace(target_path, rtrim(target_path, replace(target_path, '/', '')), '') || ':' || target_name target
  from resolved_type_edge
  union
  select distinct replace(caller_path, rtrim(caller_path, replace(caller_path, '/', '')), ''),
    'call', caller_name,
    replace(callee_path, rtrim(callee_path, replace(callee_path, '/', '')), '') || ':' || callee_name
  from resolved_edge
), s as (
  select distinct replace(owner_path, rtrim(owner_path, replace(owner_path, '/', '')), '') file,
    case when kind = 'implements' then 'heritage' else kind end kind, owner_name owner,
    replace(target_path, rtrim(target_path, replace(target_path, '/', '')), '') || ':' || target_name target
  from slow.resolved_type_edge
  union
  select distinct replace(caller_path, rtrim(caller_path, replace(caller_path, '/', '')), ''),
    'call', caller_name,
    replace(callee_path, rtrim(callee_path, replace(callee_path, '/', '')), '') || ':' || callee_name
  from slow.resolved_edge
), u as (select * from f union select * from s)
select group_concat(line, char(10)) from (
  select printf('%-12s %-9s %-12s -> %-22s %s%s', file, kind, owner, target,
    iif((file, kind, owner, target) in (select * from f), 'f', '-'),
    iif((file, kind, owner, target) in (select * from s), 's', '-')) line
  from u order by file, kind, owner, target)";

fn ryi(args: &[&str]) {
    let output = Command::new(env!("CARGO_BIN_EXE_ryi"))
        .args(args)
        .env("RUST_LOG", "off")
        .env("RYI_MAX_MEM_MB", "2048")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "ryi {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn ts_ladder_fast_and_slow() {
    let scratch = tempfile::tempdir().unwrap();
    let fast = scratch.path().join("fast.db").to_string_lossy().into_owned();
    let slow = scratch.path().join("slow.db").to_string_lossy().into_owned();
    let src = format!("{LADDER}/src");
    let index = format!("{LADDER}/index.scip");
    ryi(&["fast", &src, "--sqlite", &fast]);
    ryi(&[
        "slow", &src, "--root", LADDER, "--scip-index", &index,
        "--no-checker", "--sqlite", &slow,
    ]);

    let conn = rusqlite::Connection::open(&fast).unwrap();
    conn.execute("attach ?1 as slow", [&slow]).unwrap();
    let table: String = conn.query_row(TABLE, [], |row| row.get(0)).unwrap();
    assert_eq!(
        table,
        "\
_0_types.ts  returns   makeService  -> _0_types.ts:Service    fs
_2_one.ts    call      one          -> _0_types.ts:Service    f-
_2_one.ts    call      one          -> _0_types.ts:ping       fs
_2_one.ts    param     one          -> _0_types.ts:Base       fs
_2_one.ts    returns   one          -> _0_types.ts:Base       fs
_3_many.ts   call      many         -> _0_types.ts:makeService fs
_3_many.ts   call      many         -> _0_types.ts:ping       fs
_3_many.ts   generic   many         -> _0_types.ts:Base       fs
_3_many.ts   heritage  Child        -> _0_types.ts:Service    fs
_3_many.ts   param     many         -> _0_types.ts:Box        fs
_3_many.ts   returns   many         -> _0_types.ts:Base       fs
_4_nested.ts call      reexported   -> _2_one.ts:one          fs
_4_nested.ts call      run          -> _0_types.ts:ping       fs
_4_nested.ts call      run          -> _3_many.ts:Child       f-
_4_nested.ts call      run          -> _3_many.ts:many        fs
_4_nested.ts heritage  Nested       -> _0_types.ts:Service    fs
_4_nested.ts param     reexported   -> _0_types.ts:Base       fs
_4_nested.ts returns   reexported   -> _0_types.ts:Base       fs"
    );
}
