//! Every type edge of `fixtures/type_ladder` (files ladder 0, 1, many, nested), marked by tier:
//! `f` = ryi fast has it, `s` = ryi slow (committed index.scip) has it. `regen.sh` rebuilds the index.

#![cfg(feature = "cli")]

use std::process::Command;

const LADDER: &str = "tests/fixtures/type_ladder";

const TABLE: &str = "
with f as (select distinct replace(owner_path, rtrim(owner_path, replace(owner_path, '/', '')), '') file,
                  kind, owner_name, replace(target_path, rtrim(target_path, replace(target_path, '/', '')), '') || ':' || target_name target
           from resolved_type_edge),
     s as (select distinct replace(owner_path, rtrim(owner_path, replace(owner_path, '/', '')), '') file,
                  kind, owner_name, replace(target_path, rtrim(target_path, replace(target_path, '/', '')), '') || ':' || target_name target
           from slow.resolved_type_edge),
     u as (select * from f union select * from s)
select group_concat(line, char(10)) from (
  select printf('%-11s %-8s %-13s -> %-14s %s%s', file, kind, owner_name, target,
                iif((file, kind, owner_name, target) in (select * from f), 'f', '-'),
                iif((file, kind, owner_name, target) in (select * from s), 's', '-')) line
  from u order by file, kind, owner_name, target)";

fn ryi(args: &[&str]) {
    let output = Command::new(env!("CARGO_BIN_EXE_ryi")).args(args).env("RUST_LOG", "off").output().unwrap();
    assert!(output.status.success(), "ryi {args:?}: {}", String::from_utf8_lossy(&output.stderr));
}

#[test]
fn type_ladder_fast_and_slow() {
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
    assert_eq!(
        table,
        "\
_1_none.rs  uses     NoField       -> _1_none.rs:NoField f-
_2_one.rs   field    OneField      -> _0_types.rs:A  fs
_2_one.rs   field    OneVariant    -> _0_types.rs:A  fs
_2_one.rs   generic  OneBound      -> _0_types.rs:T  fs
_2_one.rs   generic  OneField      -> _0_types.rs:A  fs
_2_one.rs   impl     OneField      -> _0_types.rs:T  fs
_2_one.rs   param    from          -> _0_types.rs:A  fs
_2_one.rs   param    one_param     -> _0_types.rs:A  fs
_2_one.rs   returns  one_return    -> _0_types.rs:A  fs
_2_one.rs   uses     OneAlias      -> _0_types.rs:A  fs
_2_one.rs   uses     OneField      -> _2_one.rs:OneField f-
_3_many.rs  field    ManyFields    -> _0_types.rs:A  fs
_3_many.rs  field    ManyFields    -> _0_types.rs:B  fs
_3_many.rs  field    ManyFields    -> _0_types.rs:C  fs
_3_many.rs  field    ManyVariants  -> _0_types.rs:A  fs
_3_many.rs  field    ManyVariants  -> _0_types.rs:B  fs
_3_many.rs  field    ManyVariants  -> _0_types.rs:C  fs
_3_many.rs  generic  ManyBounds    -> _0_types.rs:T  fs
_3_many.rs  generic  ManyBounds    -> _0_types.rs:U  fs
_3_many.rs  impl     ManyFields    -> _0_types.rs:T  fs
_3_many.rs  impl     ManyFields    -> _0_types.rs:U  fs
_3_many.rs  param    many_params   -> _0_types.rs:A  fs
_3_many.rs  param    many_params   -> _0_types.rs:B  fs
_3_many.rs  param    many_params   -> _0_types.rs:C  fs
_3_many.rs  param    method        -> _0_types.rs:B  fs
_3_many.rs  returns  many_returns  -> _0_types.rs:A  fs
_3_many.rs  returns  many_returns  -> _0_types.rs:B  fs
_3_many.rs  returns  method        -> _0_types.rs:C  fs
_3_many.rs  uses     ManyAliasA    -> _0_types.rs:A  fs
_3_many.rs  uses     ManyAliasB    -> _0_types.rs:B  fs
_3_many.rs  uses     ManyFields    -> _3_many.rs:ManyFields f-
_4_nested.rs field    Nest          -> _0_types.rs:A  fs
_4_nested.rs field    Nest          -> _0_types.rs:B  fs
_4_nested.rs field    Nest          -> _0_types.rs:C  fs
_4_nested.rs impl     Nest          -> _0_types.rs:V  fs
_4_nested.rs impl     Nest          -> _0_types.rs:W  fs
_4_nested.rs param    gat_use       -> _0_types.rs:C  fs
_4_nested.rs param    gat_use       -> _0_types.rs:Out -s
_4_nested.rs param    projection    -> _0_types.rs:Out -s
_4_nested.rs uses     Nest          -> _4_nested.rs:Nest fs"
    );
}
