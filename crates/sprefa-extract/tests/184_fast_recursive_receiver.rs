#![cfg(feature = "cli")]

use std::process::Command;

use rusqlite::Connection;

const SOURCE: &str = r#"
fn probe(s: String, v: Vec<String>, needle: String) {
    let _ = s.contains(&needle);
    let _ = v.contains(&needle);
    let unknown = source();
    let _ = unknown.contains(&needle);
}
"#;

const RECEIVER_FIXPOINT: &str = r#"
WITH RECURSIVE
direct_type(start, end, ty) AS (
    SELECT binding.span__start, binding.span__end, type_name.name
    FROM param AS binding
    JOIN node AS function_call
      ON function_call.family = 'call'
     AND function_call.kind = 'function'
     AND function_call.span__start <= binding.span__start
     AND function_call.span__end >= binding.span__end
    JOIN node AS function_type
      ON function_type.family = 'type'
     AND function_type.kind = 'function'
     AND function_type.name = function_call.name
    JOIN sig
      ON sig.family = 'type'
     AND sig.slot = 'param'
     AND sig.pos = binding.pos
     AND sig.owner_start = function_type.span__start
     AND sig.owner_end = function_type.span__end
    JOIN node AS parameter
      ON parameter.family = 'cst'
     AND parameter.kind = 'parameter'
    JOIN edge AS binding_child
      ON binding_child.family = 'cst'
     AND binding_child.kind = 'child'
     AND binding_child.from__start = parameter.span__start
     AND binding_child.from__end = parameter.span__end
     AND binding_child.to__start = binding.span__start
     AND binding_child.to__end = binding.span__end
    JOIN edge AS type_child
      ON type_child.family = 'cst'
     AND type_child.kind = 'child'
     AND type_child.from__start = parameter.span__start
     AND type_child.from__end = parameter.span__end
     AND NOT (type_child.to__start = binding.span__start
          AND type_child.to__end = binding.span__end)
    JOIN node AS type_root
      ON type_root.family = 'cst'
     AND type_root.span__start = type_child.to__start
     AND type_root.span__end = type_child.to__end
    JOIN node AS type_name
      ON type_name.family = 'cst'
     AND type_name.kind = 'type_identifier'
     AND type_name.name IS NOT NULL
     AND type_name.span__start = type_root.span__start
    WHERE binding.family = 'df'
),
typed(start, end, ty) AS (
    SELECT start, end, ty FROM direct_type
    UNION
    SELECT flow.to__start, flow.to__end, typed.ty
    FROM typed
    JOIN edge AS flow
      ON flow.family = 'df'
     AND flow.kind = 'direct'
     AND flow.from__start = typed.start
     AND flow.from__end = typed.end
     AND flow.to_kind IN ('var_read', 'let_bind')
),
contains_site(site_start, site_end, receiver_start, receiver_end) AS (
    SELECT site.span__start, site.span__end, arg.arg__start, arg.arg__end
    FROM site
    JOIN arg
      ON arg.family = 'df'
     AND arg.pos = -1
     AND arg.call__start = site.span__start
     AND arg.call__end = site.span__end
    WHERE site.family = 'call'
      AND site.callee = 'contains'
),
resolved(site_start, site_end, ty, symbol) AS (
    SELECT contains_site.site_start, contains_site.site_end, typed.ty,
           CASE typed.ty
             WHEN 'String' THEN 'str::contains'
             WHEN 'Vec' THEN 'vec::contains'
           END
    FROM contains_site
    JOIN typed
      ON typed.start = contains_site.receiver_start
     AND typed.end = contains_site.receiver_end
    WHERE typed.ty IN ('String', 'Vec')
),
unresolved(site_start, site_end, reason) AS (
    SELECT contains_site.site_start, contains_site.site_end,
           CASE WHEN EXISTS (
               SELECT 1 FROM typed
               WHERE typed.start = contains_site.receiver_start
                 AND typed.end = contains_site.receiver_end
           ) THEN 'unsupported_receiver_type'
             ELSE 'no_syntactic_receiver_type'
           END
    FROM contains_site
    LEFT JOIN resolved
      ON resolved.site_start = contains_site.site_start
     AND resolved.site_end = contains_site.site_end
    WHERE resolved.site_start IS NULL
)
SELECT site_start, site_end, ty, symbol, NULL AS reason FROM resolved
UNION ALL
SELECT site_start, site_end, NULL AS ty, NULL AS symbol, reason FROM unresolved
ORDER BY site_start
"#;

#[test]
fn string_vec_contains_split_and_untyped_decline_are_visible_in_recursive_sqlite_oracle() {
    let scratch = tempfile::tempdir().unwrap();
    let source = scratch.path().join("recursive_receiver.rs");
    let database = scratch.path().join("fast.sqlite");
    std::fs::write(&source, SOURCE).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_ryii"))
        .args([
            "fast",
            source.to_str().unwrap(),
            "--sqlite",
            database.to_str().unwrap(),
        ])
        .env("RUST_LOG", "off")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "ryii fast failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let connection = Connection::open(database).unwrap();
    let rows = connection
        .prepare(RECEIVER_FIXPOINT)
        .unwrap()
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
            ))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    assert_eq!(
        rows,
        vec![
            (
                69,
                77,
                Some("String".into()),
                Some("str::contains".into()),
                None
            ),
            (
                102,
                110,
                Some("Vec".into()),
                Some("vec::contains".into()),
                None
            ),
            (
                169,
                177,
                None,
                None,
                Some("no_syntactic_receiver_type".into())
            ),
        ]
    );
}
