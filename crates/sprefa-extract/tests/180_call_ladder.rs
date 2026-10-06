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
    let output = Command::new(env!("CARGO_BIN_EXE_ryii"))
        .args(args)
        .env("RUST_LOG", "off")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "ryi {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn qualified_new_uses_its_declaring_type() {
    let scratch = tempfile::tempdir().unwrap();
    let fast = scratch
        .path()
        .join("fast.db")
        .to_string_lossy()
        .into_owned();
    ryi(&[
        "fast",
        "tests/fixtures/call_ladder_qualified/src",
        "--sqlite",
        &fast,
    ]);
    let conn = rusqlite::Connection::open(&fast).unwrap();
    let rows: Vec<String> = conn
        .prepare(
            "select caller_name,
           replace(callee_path, rtrim(callee_path, replace(callee_path, '/', '')), '') target_file,
           callee_name from resolved_edge
         where caller_path like '%/_2_qualified.rs'
         order by caller_site_start, target_file, callee_name",
        )
        .unwrap()
        .query_map([], |row| {
            Ok(format!(
                "{} -> {}:{}",
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?
            ))
        })
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(rows.join("\n"), "choose_left -> _0_left.rs:new");
}

#[path = "support/0_rust_names_call_contract.rs"]
mod names_call_contract;

#[test]
fn std_associated_calls_names_and_types() {
    let cases = [
        ("tests/fixtures/call_ladder_qualified", "_3_default.rs", None,
         "drop _3_default.rs:190 Default::default no_corpus_def\ndrop _3_default.rs:92 Default::default no_corpus_def\nedge _3_default.rs:37 default_probe -> _0_left.rs:82 Defaults name_resolve module_plane",
         "edge _3_default.rs:190 typed_default_probe -> _0_left.rs:145 default checker_resolve checker\nedge _3_default.rs:37 default_probe -> _0_left.rs:82 Defaults checker_resolve checker\nedge _3_default.rs:92 default_probe -> _0_left.rs:145 default checker_resolve checker"),
        ("tests/fixtures/rust_findings/paths3/crate_a", "lib.rs", Some(471),
         "drop lib.rs:471 Gem::from no_corpus_def",
         "edge lib.rs:471 lib_user -> gem.rs:102 from checker_resolve checker"),
    ];
    for (root, file, site, names, types) in cases {
        for (checker, expected) in [(false, names), (true, types)] {
            let mut command = Command::new(env!("CARGO_BIN_EXE_ryii"));
            command
                .args(["--resolve", "--arms", "call", "--root", root])
                .arg(format!("{root}/src"))
                .env("RUST_LOG", "off");
            if checker {
                command.arg("--rust-checker");
            }
            let output = command.output().unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let facts = String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
                .filter(|row| {
                    let (path, start) = if row["record"] == "resolved_edge" {
                        (&row["caller_path"], &row["caller_site_start"])
                    } else {
                        (&row["path"], &row["span"]["start"])
                    };
                    path.as_str().is_some_and(|path| path.ends_with(file))
                        && site.is_none_or(|site| start.as_u64() == Some(site))
                })
                .collect::<Vec<_>>();
            assert_eq!(
                names_call_contract::call_contract(&facts),
                expected,
                "{root} checker={checker}"
            );
        }
    }
}

#[test]
fn call_ladder_fast_and_slow() {
    let scratch = tempfile::tempdir().unwrap();
    let fast = scratch
        .path()
        .join("fast.db")
        .to_string_lossy()
        .into_owned();
    let slow = scratch
        .path()
        .join("slow.db")
        .to_string_lossy()
        .into_owned();
    let src = format!("{LADDER}/src");
    let index = format!("{LADDER}/index.scip");
    ryi(&["fast", &src, "--sqlite", &fast]);
    ryi(&[
        "slow",
        &src,
        "--root",
        LADDER,
        "--scip-index",
        &index,
        "--no-checker",
        "--sqlite",
        &slow,
    ]);

    let conn = rusqlite::Connection::open(&fast).unwrap();
    conn.execute("attach ?1 as slow", [&slow]).unwrap();
    let table: String = conn.query_row(TABLE, [], |row| row.get(0)).unwrap();
    let foreign_output: i64 = conn
        .query_row(
            "select count(*) from resolved_edge where caller_name = 'external_receiver' \
         and callee_name = 'output' and callee_path like '%/_5_foreign/src/lib.rs'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(foreign_output, 0);
    let local_probes: (i64, i64) = conn.query_row(
        "select
           (select count(*) from resolved_edge where caller_name in ('local_probe_six', 'local_probe_seven') and caller_path = callee_path and callee_name = 'rows'),
           (select count(*) from slow.resolved_edge where caller_name in ('local_probe_six', 'local_probe_seven') and caller_path = callee_path and callee_name = 'rows')",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    ).unwrap();
    assert_eq!(local_probes, (0, 2));
    let trait_adapter: (i64, i64) = conn.query_row(
        "select
           (select count(*) from resolved_edge where caller_path like '%/_8_trait.rs' and callee_path like '%/_0_types.rs' and callee_name = 'ping'),
           (select count(*) from resolved_edge where caller_path like '%/_8_trait.rs' and callee_path like '%/_8_trait.rs' and callee_name = 'ping')",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    ).unwrap();
    assert_eq!(trait_adapter, (0, 0));
    assert_eq!(
        table,
        "\
_10_self_constructor.rs   91 first              -> _10_self_constructor.rs:First      f-
_10_self_constructor.rs  226 first_rows         -> _10_self_constructor.rs:First      f-
_10_self_constructor.rs  360 second             -> _10_self_constructor.rs:Second     f-
_10_self_constructor.rs  498 second_rows        -> _10_self_constructor.rs:Second     f-
_11_path_module.rs   33 path_module_probe  -> lib.rs:exit       f-
_2_one.rs     104 free_call          -> _0_types.rs:free_zero  fs
_2_one.rs     166 inherent_call      -> _0_types.rs:ping       -s
_2_one.rs     225 trait_impl_call    -> _0_types.rs:act        -s
_2_one.rs     289 trait_object_call  -> _0_types.rs:act        -s
_2_one.rs     350 generic_call       -> _0_types.rs:act        -s
_2_one.rs     395 ufcs_call          -> _0_types.rs:act        fs
_2_one.rs     471 closure@468        -> _0_types.rs:free_zero  fs
_2_one.rs     471 closure_call       -> _0_types.rs:free_zero  f-
_2_one.rs     546 deref_call         -> _0_types.rs:ping       -s
_2_one.rs     581 reexport_call      -> _0_types.rs:free_zero  fs
_2_one.rs     668 macro_call         -> _0_types.rs:free_zero  f-
_3_many.rs     89 many_calls         -> _0_types.rs:free_zero  fs
_3_many.rs    106 many_calls         -> _0_types.rs:free_one   fs
_3_many.rs    123 many_calls         -> _0_types.rs:free_two   fs
_3_many.rs    143 many_calls         -> _0_types.rs:make       fs
_3_many.rs    158 many_calls         -> _0_types.rs:ping       -s
_4_nested.rs  114 inner              -> _0_types.rs:act        -s
_4_nested.rs  140 nested_calls       -> _0_types.rs:make       fs
_4_nested.rs  160 nested_calls       -> _4_nested.rs:inner      fs
_4_nested.rs  184 closure@181        -> _0_types.rs:free_zero  fs
_4_nested.rs  184 nested_calls       -> _0_types.rs:free_zero  f-
_6_local.rs   119 local_probe_six    -> _6_local.rs:rows       -s
_7_local.rs   121 local_probe_seven  -> _7_local.rs:rows       -s
_8_trait.rs   123 ping               -> _0_types.rs:ping       -s"
    );
}

#[test]
fn self_struct_constructors_bind_to_the_enclosing_impl_type() {
    let scratch = tempfile::tempdir().unwrap();
    let fast = scratch
        .path()
        .join("fast.db")
        .to_string_lossy()
        .into_owned();
    ryi(&["fast", &format!("{LADDER}/src"), "--sqlite", &fast]);
    let conn = rusqlite::Connection::open(&fast).unwrap();
    let rows: Vec<(String, String)> = conn
        .prepare("select distinct caller_name, callee_name from resolved_edge where caller_path like '%/_10_self_constructor.rs' and callee_path like '%/_10_self_constructor.rs' and caller_name in ('first', 'second') order by caller_name, callee_name")
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(
        rows,
        [
            ("first".into(), "First".into()),
            ("second".into(), "Second".into())
        ]
    );
}

#[test]
fn path_module_super_calls_bind_to_the_declaring_parent() {
    let relative = format!("{LADDER}/src");
    let absolute = std::fs::canonicalize(&relative)
        .unwrap()
        .to_string_lossy()
        .into_owned();
    for src in [relative, absolute] {
        let scratch = tempfile::tempdir().unwrap();
        let fast = scratch
            .path()
            .join("fast.db")
            .to_string_lossy()
            .into_owned();
        ryi(&["fast", &src, "--sqlite", &fast]);
        let conn = rusqlite::Connection::open(&fast).unwrap();
        let rows: Vec<(String, String)> = conn
            .prepare("select distinct callee_path, callee_name from resolved_edge where caller_path like '%/deep/_11_path_module.rs' and caller_name = 'path_module_probe' order by callee_path, callee_name")
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(rows.len(), 1, "{src}");
        assert!(
            rows[0].0.ends_with("/call_ladder/src/lib.rs"),
            "{src}: {rows:?}"
        );
        assert_eq!(rows[0].1, "exit");
    }
}

#[test]
fn same_named_methods_on_local_types_need_types() {
    let scratch = tempfile::tempdir().unwrap();
    let fast = scratch
        .path()
        .join("fast.db")
        .to_string_lossy()
        .into_owned();
    ryi(&["fast", &format!("{LADDER}/src"), "--sqlite", &fast]);
    let conn = rusqlite::Connection::open(&fast).unwrap();
    let rows: Vec<(String, i64)> = conn
        .prepare("select caller_name, callee_start from resolved_edge where caller_path like '%/_10_self_constructor.rs' and callee_name = 'rows' and caller_name in ('first_rows', 'second_rows') order by caller_name")
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(rows, Vec::<(String, i64)>::new());
    let drops: Vec<(String, String)> = conn.prepare("select detail, reason from unresolved where path like '%/_10_self_constructor.rs' and detail = 'rows' order by span__start")
        .unwrap().query_map([], |row| Ok((row.get(0)?, row.get(1)?))).unwrap().collect::<Result<_, _>>().unwrap();
    assert_eq!(drops, [("rows".into(), "needs_types".into()), ("rows".into(), "needs_types".into())]);
}

#[test]
fn cargo_target_and_dependency_call_boundaries() {
    let relative = "tests/fixtures/rust_visibility".to_string();
    let absolute = std::fs::canonicalize(&relative)
        .unwrap()
        .to_string_lossy()
        .into_owned();
    for source in [relative, absolute] {
        let scratch = tempfile::tempdir().unwrap();
        let fast = scratch
            .path()
            .join("fast.db")
            .to_string_lossy()
            .into_owned();
        ryi(&["fast", &source, "--sqlite", &fast]);
        let conn = rusqlite::Connection::open(&fast).unwrap();
        let count = |caller: &str, callee: &str, source_suffix: &str, target_suffix: &str| -> i64 {
            conn.query_row(
                "select count(*) from resolved_edge where caller_name = ?1 and callee_name = ?2
                   and caller_path like '%' || ?3 and callee_path like '%' || ?4",
                rusqlite::params![caller, callee, source_suffix, target_suffix],
                |row| row.get(0),
            )
            .unwrap()
        };
        let lib = "/rust_visibility/app/src/lib.rs";
        assert_eq!(
            count(
                "library_target_probe",
                "bin_helper",
                lib,
                "/app/src/bin/0_tool.rs"
            ),
            0
        );
        assert_eq!(
            count(
                "library_target_probe",
                "nested_helper",
                lib,
                "/app/tests/data/nested/src/lib.rs"
            ),
            0
        );
        assert_eq!(
            count(
                "library_target_probe",
                "normal_call",
                lib,
                "/normal_dep/src/lib.rs"
            ),
            1
        );
        assert_eq!(
            count(
                "library_target_probe",
                "dev_call",
                lib,
                "/dev_dep/src/lib.rs"
            ),
            0
        );
        assert_eq!(
            count(
                "library_target_probe",
                "build_call",
                lib,
                "/build_dep/src/lib.rs"
            ),
            0
        );
        assert_eq!(
            count(
                "main",
                "build_call",
                "/rust_visibility/app/build.rs",
                "/build_dep/src/lib.rs"
            ),
            1
        );
        assert_eq!(
            count(
                "dev_probe",
                "dev_call",
                "/rust_visibility/app/tests/0_dev.rs",
                "/dev_dep/src/lib.rs"
            ),
            1
        );
        assert_eq!(
            count(
                "main",
                "bin_helper",
                "/rust_visibility/app/src/bin/0_tool.rs",
                "/app/src/bin/0_tool.rs"
            ),
            1
        );
    }
}
