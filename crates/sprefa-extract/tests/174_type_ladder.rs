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
_2_one.rs   generic  one_bound     -> _0_types.rs:T  fs
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
_3_many.rs  generic  many_bounds   -> _0_types.rs:T  fs
_3_many.rs  generic  many_bounds   -> _0_types.rs:U  fs
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
_4_nested.rs generic  assoc_bound   -> _0_types.rs:B  fs
_4_nested.rs generic  assoc_bound   -> _0_types.rs:V  fs
_4_nested.rs generic  gat_use       -> _0_types.rs:W  fs
_4_nested.rs generic  projection    -> _0_types.rs:V  fs
_4_nested.rs impl     Nest          -> _0_types.rs:V  fs
_4_nested.rs impl     Nest          -> _0_types.rs:W  fs
_4_nested.rs param    gat_use       -> _0_types.rs:C  fs
_4_nested.rs param    gat_use       -> _0_types.rs:Out fs
_4_nested.rs param    projection    -> _0_types.rs:Out fs
_4_nested.rs uses     Nest          -> _4_nested.rs:Nest fs"
    );
}

#[test]
fn type_scope_ladder_keeps_prelude_result_external() {
    let scratch = tempfile::tempdir().unwrap();
    let fast = scratch.path().join("fast.db").to_string_lossy().into_owned();
    let slow = scratch.path().join("slow.db").to_string_lossy().into_owned();
    let ladder = "tests/fixtures/type_ladder_scope";
    let src = format!("{ladder}/src");
    let index = format!("{ladder}/index.scip");
    ryi(&["fast", &src, "--sqlite", &fast]);
    ryi(&["slow", &src, "--root", ladder, "--scip-index", &index, "--no-checker", "--sqlite", &slow]);

    let conn = rusqlite::Connection::open(&fast).unwrap();
    conn.execute("attach ?1 as slow", [&slow]).unwrap();
    let rows: (i64, i64, i64, i64, i64, i64, i64, i64) = conn.query_row(
        "select
           (select count(*) from resolved_type_edge where owner_name = 'prelude_result' and target_name = 'Result'),
           (select count(*) from resolved_type_edge where owner_name = 'local_result' and target_name = 'Result'),
           (select count(*) from slow.resolved_type_edge where owner_name = 'local_result' and target_name = 'Result'),
           (select count(*) from resolved_type_edge where owner_name = 'external_output' and target_name = 'Output'),
           (select count(*) from resolved_type_edge where owner_name = 'bridged' and target_name = 'LocalThing'),
           (select count(*) from slow.resolved_type_edge where owner_name = 'bridged' and target_name = 'LocalThing'),
           (select count(*) from resolved_type_edge where owner_name = 'Generic' and target_name = 'Outer'),
           (select count(*) from resolved_type_edge where owner_name = 'nested' and target_name = 'Output' and target_path like '%/_6_nested.rs')",
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?, row.get(6)?, row.get(7)?)),
    ).unwrap();
    assert_eq!(rows, (0, 1, 1, 0, 1, 1, 0, 0));
}

#[test]
fn type_scope_ladder_preserves_declared_fixture_dependency() {
    let scratch = tempfile::tempdir().unwrap();
    let fast = scratch.path().join("fast.db").to_string_lossy().into_owned();
    let root = std::fs::canonicalize("tests/fixtures/type_ladder_dependency").unwrap();
    ryi(&["fast", root.to_str().unwrap(), "--sqlite", &fast]);
    let conn = rusqlite::Connection::open(&fast).unwrap();
    let rows: i64 = conn.query_row(
        "select count(*) from resolved_type_edge
         where owner_path like '%/type_ladder_dependency/%/src/lib.rs'
           and target_path like '%/0_collector/src/0_collector.rs'
           and target_name in ('BulkTrigger', 'RowChange')",
        [],
        |row| row.get(0),
    ).unwrap();
    assert_eq!(rows, 6);
}

#[test]
fn type_scope_ladder_keeps_uncrated_std_import_external() {
    let scratch = tempfile::tempdir().unwrap();
    let fast = scratch.path().join("fast.db").to_string_lossy().into_owned();
    ryi(&["fast", "tests/fixtures/type_ladder_uncrated", "--sqlite", &fast]);
    let conn = rusqlite::Connection::open(&fast).unwrap();
    let rows: i64 = conn.query_row(
        "select count(*) from resolved_type_edge where owner_name = 'probe' and target_name = 'Output'",
        [],
        |row| row.get(0),
    ).unwrap();
    assert_eq!(rows, 0);
}

#[test]
fn type_scope_ladder_finds_local_body_annotation() {
    let scratch = tempfile::tempdir().unwrap();
    let fast = scratch.path().join("fast.db").to_string_lossy().into_owned();
    let slow = scratch.path().join("slow.db").to_string_lossy().into_owned();
    let ladder = "tests/fixtures/type_ladder_scope";
    let src = format!("{ladder}/src");
    let index = format!("{ladder}/index.scip");
    ryi(&["fast", &src, "--sqlite", &fast]);
    ryi(&["slow", &src, "--root", ladder, "--scip-index", &index, "--no-checker", "--sqlite", &slow]);
    let conn = rusqlite::Connection::open(&fast).unwrap();
    conn.execute("attach ?1 as slow", [&slow]).unwrap();
    let rows: (i64, i64) = conn.query_row(
        "select
           (select count(*) from resolved_type_edge where owner_name = 'local_annotation'
              and target_name = 'LocalThing' and target_path like '%/_0_alias.rs'),
           (select count(*) from slow.resolved_type_edge where owner_name = 'local_annotation'
              and target_name = 'LocalThing' and target_path like '%/_0_alias.rs')",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    ).unwrap();
    assert_eq!(rows, (1, 1));
}

#[test]
fn type_scope_ladder_finds_required_trait_signature() {
    let scratch = tempfile::tempdir().unwrap();
    let fast = scratch.path().join("fast.db").to_string_lossy().into_owned();
    let slow = scratch.path().join("slow.db").to_string_lossy().into_owned();
    let ladder = "tests/fixtures/type_ladder_scope";
    let src = format!("{ladder}/src");
    let index = format!("{ladder}/index.scip");
    ryi(&["fast", &src, "--sqlite", &fast]);
    ryi(&["slow", &src, "--root", ladder, "--scip-index", &index, "--no-checker", "--sqlite", &slow]);
    let conn = rusqlite::Connection::open(&fast).unwrap();
    conn.execute("attach ?1 as slow", [&slow]).unwrap();
    let rows: (i64, i64) = conn.query_row(
        "select
           (select count(*) from resolved_type_edge where owner_name = 'read'
              and target_name = 'LocalThing' and target_path like '%/_0_alias.rs'),
           (select count(*) from slow.resolved_type_edge where owner_name = 'read'
              and target_name = 'LocalThing' and target_path like '%/_0_alias.rs')",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    ).unwrap();
    assert_eq!(rows, (2, 2));
}

#[test]
fn type_scope_ladder_finds_impl_associated_type() {
    let scratch = tempfile::tempdir().unwrap();
    let fast = scratch.path().join("fast.db").to_string_lossy().into_owned();
    let slow = scratch.path().join("slow.db").to_string_lossy().into_owned();
    let ladder = "tests/fixtures/type_ladder_scope";
    let src = format!("{ladder}/src");
    let index = format!("{ladder}/index.scip");
    ryi(&["fast", &src, "--sqlite", &fast]);
    ryi(&["slow", &src, "--root", ladder, "--scip-index", &index, "--no-checker", "--sqlite", &slow]);
    let conn = rusqlite::Connection::open(&fast).unwrap();
    conn.execute("attach ?1 as slow", [&slow]).unwrap();
    let rows: (i64, i64) = conn.query_row(
        "select
           (select count(*) from resolved_type_edge where owner_name = 'Item'
              and owner_path like '%/_9_assoc.rs' and target_name = 'LocalThing'
              and target_path like '%/_0_alias.rs'),
           (select count(*) from slow.resolved_type_edge where owner_name = 'Item'
              and owner_path like '%/_9_assoc.rs' and target_name = 'LocalThing'
              and target_path like '%/_0_alias.rs')",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    ).unwrap();
    assert_eq!(rows, (1, 1));
}

#[test]
fn type_scope_ladder_finds_qualified_variant_field() {
    let scratch = tempfile::tempdir().unwrap();
    let fast = scratch.path().join("fast.db").to_string_lossy().into_owned();
    let slow = scratch.path().join("slow.db").to_string_lossy().into_owned();
    let ladder = "tests/fixtures/type_ladder_scope";
    let src = format!("{ladder}/src");
    let index = format!("{ladder}/index.scip");
    ryi(&["fast", &src, "--sqlite", &fast]);
    ryi(&["slow", &src, "--root", ladder, "--scip-index", &index, "--no-checker", "--sqlite", &slow]);
    let conn = rusqlite::Connection::open(&fast).unwrap();
    conn.execute("attach ?1 as slow", [&slow]).unwrap();
    let rows: (i64, i64) = conn.query_row(
        "select
           (select count(*) from resolved_type_edge where owner_name = 'Item'
              and owner_path like '%/_10_variant.rs' and target_name = 'LocalThing'
              and target_path like '%/_0_alias.rs'),
           (select count(*) from slow.resolved_type_edge where owner_name = 'Item'
              and owner_path like '%/_10_variant.rs' and target_name = 'LocalThing'
              and target_path like '%/_0_alias.rs')",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    ).unwrap();
    assert_eq!(rows, (1, 1));
}

#[test]
fn type_scope_ladder_follows_crate_module_reexport() {
    let scratch = tempfile::tempdir().unwrap();
    let fast = scratch.path().join("fast.db").to_string_lossy().into_owned();
    let absolute_fast = scratch.path().join("absolute.db").to_string_lossy().into_owned();
    let slow = scratch.path().join("slow.db").to_string_lossy().into_owned();
    let ladder = "tests/fixtures/type_ladder_scope";
    let src = format!("{ladder}/src");
    let index = format!("{ladder}/index.scip");
    ryi(&["fast", &src, "--sqlite", &fast]);
    let absolute_src = std::fs::canonicalize(&src).unwrap();
    ryi(&["fast", absolute_src.to_str().unwrap(), "--sqlite", &absolute_fast]);
    ryi(&["slow", &src, "--root", ladder, "--scip-index", &index, "--no-checker", "--sqlite", &slow]);
    let conn = rusqlite::Connection::open(&fast).unwrap();
    conn.execute("attach ?1 as slow", [&slow]).unwrap();
    conn.execute("attach ?1 as absolute", [&absolute_fast]).unwrap();
    let rows: (i64, i64, i64, i64) = conn.query_row(
        "select
           (select count(*) from resolved_type_edge where owner_name = 'reexport_chain'
              and target_name = 'PubThing' and target_path like '%/_11_reexport.rs'),
           (select count(*) from slow.resolved_type_edge where owner_name = 'reexport_chain'
              and target_name = 'PubThing' and target_path like '%/_11_reexport.rs'),
           (select count(*) from absolute.resolved_type_edge where owner_name = 'reexport_chain'
              and target_name = 'PubThing' and target_path like '%/_11_reexport.rs'),
           (select count(*) from absolute.resolved_type_edge where owner_name = 'external_shadow'
              and target_name = 'String' and target_path like '%/_12_other.rs')",
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
    ).unwrap();
    assert_eq!(rows, (1, 1, 1, 0));
}
