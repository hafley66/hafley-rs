//! One `.scm` query and its hand-written ast-grep YAML twin, run over the same
//! file. Equal rule trees and equal match sets are what "lowers 1-1" means; the
//! process exits nonzero when either differs.
use sprefa_extract::lang::{decode_ast_rule_yaml, lower_scm, query_ast_rule, AstRuleRequest};

const SCM: &str = r#"[(closure_expression)] @closure
[(function_item) (impl_item)] @scope
[(field_expression)] @receiver

((call_expression
   function: (field_expression field: (field_identifier) @name)) @m
 (#inside? @m scope)
 (#not-inside? @m closure)
 (#has? @m receiver "neighbor")
 (#match? @m "contains|starts_with|ends_with|find")
 (#not-match? @m "^is_"))
"#;

const YAML: &str = r#"id: twin
utils:
  closure:
    any: [{kind: closure_expression}]
  scope:
    any: [{kind: function_item}, {kind: impl_item}]
  receiver:
    any: [{kind: field_expression}]
rule:
  all:
    - all:
        - kind: call_expression
        - has:
            all:
              - kind: field_expression
              - has: {kind: field_identifier}
    - inside: {matches: scope, stopBy: end}
    - not: {inside: {matches: closure, stopBy: end}}
    - has: {matches: receiver}
    - regex: contains|starts_with|ends_with|find
    - not: {regex: "^is_"}
"#;

fn rows(path: &str, src: &[u8], request: &AstRuleRequest) -> Vec<(u32, u32, String)> {
    query_ast_rule(path, src, request)
        .expect("rule runs")
        .into_iter()
        .map(|found| {
            let start = found.span.start;
            let end = start + found.span.len;
            let text = String::from_utf8_lossy(&src[start as usize..end as usize]).to_string();
            (start, end, text.lines().next().unwrap_or("").to_string())
        })
        .collect()
}

fn sorted_utils(request: &AstRuleRequest) -> Vec<(String, String)> {
    let mut pairs: Vec<(String, String)> = request
        .utils
        .iter()
        .map(|util| (util.id.clone(), format!("{:?}", util.rule)))
        .collect();
    pairs.sort();
    pairs
}

fn main() {
    let path = std::env::args().nth(1).unwrap_or_else(|| "src/project.rs".into());
    let src = std::fs::read(&path).expect("read the target file");

    let lowered = lower_scm(SCM).expect(".scm lowers");
    let from_scm = AstRuleRequest {
        id: "twin".into(),
        rule: lowered.rule,
        utils: lowered.utils,
        constraints: lowered.constraints,
        fix: None,
    };
    let from_yaml = decode_ast_rule_yaml(YAML).expect("yaml decodes");

    let rules_equal = from_scm.rule == from_yaml.rule;
    let utils_equal = sorted_utils(&from_scm) == sorted_utils(&from_yaml);
    let from_a = rows(&path, &src, &from_scm);
    let from_b = rows(&path, &src, &from_yaml);

    println!("target {path}, {} bytes\n", src.len());
    println!("rule from .scm\n  {:?}\n", from_scm.rule);
    println!("rule from yaml\n  {:?}\n", from_yaml.rule);
    println!("rule trees equal: {rules_equal}");
    println!("utils equal:      {utils_equal}");
    println!("matches .scm:     {}", from_a.len());
    println!("matches yaml:     {}", from_b.len());
    println!("match sets equal: {}\n", from_a == from_b);
    for (start, end, text) in from_a.iter().take(12) {
        println!("  {start:>7}..{end:<7} {text}");
    }
    if from_a.len() > 12 {
        println!("  ... {} more", from_a.len() - 12);
    }

    let same = rules_equal && utils_equal && from_a == from_b;
    println!("\n{}", if same { "1-1" } else { "DIVERGED" });
    std::process::exit(!same as i32);
}
