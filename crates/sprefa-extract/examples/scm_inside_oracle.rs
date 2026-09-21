// `ryi query` refuses `#inside?` (2_source_query.rs validate_predicates), so the
// lowering is driven directly for the counts hafley_scm's corpus test pins.
use sprefa_extract::lang::{lower_scm, query_ast_rule, AstRuleRequest};

fn main() {
    let path = std::env::args().nth(1).expect("PATH");
    let src = std::fs::read(&path).expect("read");
    for scm in [
        "(function_item) @fnitem\n((identifier) @x (#inside? @x fnitem \"end\"))",
        "(function_item) @fnitem\n((identifier) @x (#not-inside? @x fnitem \"end\"))",
    ] {
        let program = lower_scm(scm).expect("lowers");
        let request = AstRuleRequest {
            id: "oracle".into(),
            rule: program.rule.clone(),
            utils: program.utils.clone(),
            constraints: program.constraints.clone(),
            fix: None,
        };
        match query_ast_rule(&path, &src, &request) {
            Ok(rows) => println!("{} -> {}", scm, rows.len()),
            Err(error) => println!("{} -> ERROR {:?}", scm, error),
        }
    }
}
