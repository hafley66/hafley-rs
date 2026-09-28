use sprefa_extract::tsi::{Arg, FactOut};
use sprefa_extract::FlatFact;
use std::process::Command;

fn rows(file: &str) -> Vec<FactOut> {
    let output = Command::new(env!("CARGO_BIN_EXE_ryii"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(["--witness", "--kinds", "type", file])
        .output()
        .expect("ryii runs");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8(output.stdout)
        .expect("UTF-8 rows")
        .lines()
        .filter_map(|line| match serde_json::from_str::<FlatFact>(line).expect("fact row") {
            FlatFact::Fact(fact) => Some(fact),
            _ => None,
        })
        .collect()
}

fn scope_id(rows: &[FactOut], file: &str, written: &str) -> u32 {
    let source = std::fs::read(format!("{}/{}", env!("CARGO_MANIFEST_DIR"), file)).unwrap();
    let products: Vec<u32> = rows.iter().filter(|row| row.relation == "tsi.product")
        .filter_map(|row| match row.args.first() { Some(Arg::Id(id)) => Some(*id), _ => None })
        .collect();
    let found: Vec<u32> = rows.iter().filter(|row| row.relation == "tsi.origin")
        .filter_map(|row| match row.args.as_slice() {
            [Arg::Id(id), _, Arg::Span(_, start, end)] if products.contains(id)
                && source[*start as usize..*end as usize] == *written.as_bytes() => Some(*id),
            _ => None,
        }).collect();
    assert_eq!(found.len(), 1, "scope `{written}` has {} product origins", found.len());
    found[0]
}

#[test]
fn rust_nested_scopes_and_shadow_versions() {
    let rows = rows("tests/fixtures/lift/scope.rs");
    let products = rows.iter().filter(|row| row.relation == "tsi.product").count();
    let x_versions = rows.iter().filter(|row| {
        row.relation == "tsi.edge" && matches!(row.args.get(2), Some(Arg::Text(name)) if name == "x")
    }).count();
    assert!(products >= 3, "expected function and closure scope products; got {products}");
    assert!(x_versions >= 3, "expected shadowed x versions; got {x_versions}");
    let closure = scope_id(&rows, "tests/fixtures/lift/scope.rs", "|| x.len()");
    let captured: Vec<&FactOut> = rows.iter().filter(|row| row.relation == "tsi.edge")
        .filter(|row| matches!(row.args.get(1), Some(Arg::Id(id)) if *id == closure))
        .filter(|row| matches!(row.args.get(2), Some(Arg::Text(name)) if name == "x"))
        .collect();
    assert_eq!(captured.len(), 1);
    assert!(matches!(captured[0].args.get(4), Some(Arg::Int(41))));
}

#[test]
fn ts_arrow_env_and_var_cell() {
    let rows = rows("tests/fixtures/lift/scope.ts");
    let products = rows.iter().filter(|row| row.relation == "tsi.product").count();
    let x_versions = rows.iter().filter(|row| {
        row.relation == "tsi.edge" && matches!(row.args.get(2), Some(Arg::Text(name)) if name == "x")
    }).count();
    assert!(products >= 2, "expected function and arrow scope products; got {products}");
    assert!(x_versions >= 2, "expected var cell and reassignment version; got {x_versions}");
    let arrow = scope_id(&rows, "tests/fixtures/lift/scope.ts", "() => x");
    let captures = rows.iter().filter(|row| row.relation == "tsi.edge")
        .filter(|row| matches!(row.args.get(1), Some(Arg::Id(id)) if *id == arrow))
        .filter(|row| matches!(row.args.get(2), Some(Arg::Text(name)) if name == "x"))
        .count();
    assert_eq!(captures, 1);
}
