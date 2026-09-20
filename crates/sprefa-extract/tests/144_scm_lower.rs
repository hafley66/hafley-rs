//! `.scm` surface lowering, through the library and the real query grammar.
//! Every end-to-end case runs `query_ast_rule`, which is ast-grep's own matcher.

use sprefa_extract::lang::{
    lower_scm, query_ast_rule, scm_language, AstRule, AstRuleRequest, NamedAstRule, ScmLowerError,
    StopBy,
};

const RUST_SRC: &str = r#"fn outer(name: &str) -> bool {
    let needle = "ab";
    name.contains(needle)
}

fn other(items: &[u8]) -> bool {
    items.contains(&3)
}
"#;

const TS_SRC: &str = r#"function outer(name: string): boolean {
  const needle = "ab";
  return name.includes(needle);
}

const other = (items: number[]) => items.includes(3);
"#;

const RUST_SCOPE_SCM: &str = r#"[(function_item) (closure_expression) (block)] @local.scope

((call_expression
   function: (field_expression field: (field_identifier) @m))
 (#inside? @m local.scope))
"#;

const TS_SCOPE_SCM: &str = r#"[(function_declaration) (arrow_function) (statement_block)] @local.scope

((call_expression
   function: (member_expression property: (property_identifier) @m))
 (#inside? @m local.scope))
"#;

fn kind(name: &str) -> AstRule {
    AstRule::Kind(name.into())
}

fn has(rule: AstRule) -> AstRule {
    AstRule::Has {
        rule: Box::new(rule),
        stop_by: None,
    }
}

fn inside_to_end(rule: AstRule) -> AstRule {
    AstRule::Inside {
        rule: Box::new(rule),
        stop_by: Some(StopBy::End("end".into())),
    }
}

/// `(path, text)` for every match, which is what a span assertion reads.
fn run(path: &str, source: &str, scm: &str) -> Vec<(usize, usize, String)> {
    let program = lower_scm(scm).expect("scm lowers");
    let request = AstRuleRequest {
        id: "scm".into(),
        rule: program.rule,
        utils: program.utils,
        fix: None,
    };
    query_ast_rule(path, source.as_bytes(), &request)
        .expect("ast-grep evaluates")
        .into_iter()
        .map(|found| {
            let start = found.span.start as usize;
            let end = start + found.span.len as usize;
            (start, end, source[start..end].to_string())
        })
        .collect()
}

#[test]
fn the_query_grammar_abi_sits_inside_the_runtime_window() {
    let abi = scm_language().abi_version();
    assert!(
        (13..=15).contains(&abi),
        "tree-sitter-tsquery ABI {abi} is outside the 13..=15 window tree-sitter 0.25 accepts"
    );
}

#[test]
fn a_bare_named_node_lowers_to_kind() {
    let program = lower_scm("(function_item)").expect("scm lowers");
    assert_eq!(program.rule, kind("function_item"));
    assert_eq!(program.utils, Vec::new());
}

#[test]
fn a_list_lowers_to_any() {
    let program = lower_scm("[(function_item) (block)]").expect("scm lowers");
    assert_eq!(
        program.rule,
        AstRule::Any(vec![kind("function_item"), kind("block")])
    );
    assert_eq!(program.utils, Vec::new());
}

#[test]
fn a_nested_field_pattern_lowers_to_kind_plus_has() {
    let program =
        lower_scm("(call_expression function: (field_expression))").expect("scm lowers");
    assert_eq!(
        program.rule,
        AstRule::All(vec![kind("call_expression"), has(kind("field_expression"))])
    );
    assert_eq!(program.utils, Vec::new());
}

#[test]
fn a_grouping_lowers_to_all() {
    let program = lower_scm("((function_item) (block))").expect("scm lowers");
    assert_eq!(
        program.rule,
        AstRule::All(vec![kind("function_item"), kind("block")])
    );
}

#[test]
fn a_negated_field_lowers_to_not_has() {
    let program = lower_scm("(function_item !body)").expect("scm lowers");
    assert_eq!(
        program.rule,
        AstRule::All(vec![
            kind("function_item"),
            AstRule::Not(Box::new(has(kind("body")))),
        ])
    );
}

#[test]
fn a_named_reference_lowers_to_matches_plus_one_util() {
    let program = lower_scm(RUST_SCOPE_SCM).expect("scm lowers");
    assert_eq!(
        program.utils,
        vec![NamedAstRule {
            id: "local.scope".into(),
            rule: AstRule::Any(vec![
                kind("function_item"),
                kind("closure_expression"),
                kind("block"),
            ]),
        }]
    );
    assert_eq!(
        program.rule,
        AstRule::All(vec![
            kind("field_identifier"),
            inside_to_end(AstRule::All(vec![
                kind("call_expression"),
                has(AstRule::All(vec![
                    kind("field_expression"),
                    has(kind("field_identifier")),
                ])),
            ])),
            inside_to_end(AstRule::Matches("local.scope".into())),
        ])
    );
}

#[test]
fn follows_and_precedes_lower_to_their_relations() {
    let follows = lower_scm("(block) @s\n\n((call_expression) @m (#follows? @m s))")
        .expect("scm lowers");
    assert_eq!(
        follows.rule,
        AstRule::All(vec![
            kind("call_expression"),
            AstRule::Follows {
                rule: Box::new(AstRule::Matches("s".into())),
                stop_by: Some(StopBy::End("end".into())),
            },
        ])
    );

    let precedes = lower_scm("(block) @s\n\n((call_expression) @m (#precedes? @m s))")
        .expect("scm lowers");
    assert_eq!(
        precedes.rule,
        AstRule::All(vec![
            kind("call_expression"),
            AstRule::Precedes {
                rule: Box::new(AstRule::Matches("s".into())),
                stop_by: Some(StopBy::End("end".into())),
            },
        ])
    );
}

#[test]
fn a_has_predicate_lowers_to_has_and_a_match_predicate_to_regex() {
    let program = lower_scm("(block) @s\n\n((call_expression) @m (#has? @m s))").expect("scm lowers");
    assert_eq!(
        program.rule,
        AstRule::All(vec![
            kind("call_expression"),
            AstRule::Has {
                rule: Box::new(AstRule::Matches("s".into())),
                stop_by: Some(StopBy::End("end".into())),
            },
        ])
    );

    let regex = lower_scm("((call_expression) @m (#match? @m \"^self\\.\"))").expect("scm lowers");
    assert_eq!(
        regex.rule,
        AstRule::All(vec![
            kind("call_expression"),
            AstRule::Regex("^self\\.".into()),
        ])
    );
}

#[test]
fn the_rust_scope_query_matches_both_method_names() {
    assert_eq!(
        run("probe.rs", RUST_SRC, RUST_SCOPE_SCM),
        vec![
            (63, 71, "contains".to_string()),
            (126, 134, "contains".to_string()),
        ]
    );
}

#[test]
fn the_ts_scope_query_matches_both_method_names() {
    let found = run("probe.ts", TS_SRC, TS_SCOPE_SCM);
    assert_eq!(found.len(), 2, "ts scope query matches: {found:?}");
    assert_eq!(
        found.iter().map(|row| row.2.as_str()).collect::<Vec<_>>(),
        vec!["includes", "includes"]
    );
}

#[test]
fn an_unmapped_predicate_is_an_error() {
    assert_eq!(
        lower_scm("(#nope? @a b)"),
        Err(ScmLowerError::UnknownPredicate("nope?".into()))
    );
}

#[test]
fn an_identifier_argument_naming_no_definition_is_an_error() {
    assert_eq!(
        lower_scm("(#inside? @m no_such)"),
        Err(ScmLowerError::UnboundReference("no_such".into()))
    );
}

#[test]
fn a_wrong_parameter_count_is_an_error() {
    assert_eq!(
        lower_scm("(block) @s\n(#inside? @m)"),
        Err(ScmLowerError::PredicateArity {
            operator: "inside?".into(),
            got: 1,
        })
    );
}

#[test]
fn an_error_node_never_lowers_to_a_partial_rule() {
    assert_eq!(
        lower_scm("(function_item"),
        Err(ScmLowerError::Syntax {
            row: 0,
            message: "unparsed `.scm` text: (function_item".into(),
        })
    );
}

#[test]
fn a_duplicate_top_level_label_is_an_error() {
    assert_eq!(
        lower_scm("(block) @s\n(function_item) @s"),
        Err(ScmLowerError::DuplicateLabel("s".into()))
    );
}

#[test]
fn two_predicates_on_two_captures_are_an_error() {
    let scm = "(block) @s\n\n((call_expression (identifier) @x) @y\n (#inside? @x s)\n (#inside? @y s))";
    assert_eq!(
        lower_scm(scm),
        Err(ScmLowerError::FocusConflict {
            first: "x".into(),
            second: "y".into(),
        })
    );
}

#[test]
fn a_file_of_only_labelled_patterns_matches_every_label() {
    let program = lower_scm("(function_item) @local.scope").expect("scm lowers");
    assert_eq!(
        program.rule,
        AstRule::Any(vec![AstRule::Matches("local.scope".into())])
    );
    assert_eq!(
        program.utils,
        vec![NamedAstRule {
            id: "local.scope".into(),
            rule: kind("function_item"),
        }]
    );
}

#[test]
fn a_not_prefix_wraps_every_relation_in_not() {
    let inside = lower_scm("(block) @s\n\n((call_expression) @m (#not-inside? @m s))")
        .expect("scm lowers");
    assert_eq!(
        inside.rule,
        AstRule::All(vec![
            kind("call_expression"),
            AstRule::Not(Box::new(inside_to_end(AstRule::Matches("s".into())))),
        ])
    );

    let follows = lower_scm("(block) @s\n\n((call_expression) @m (#not-follows? @m s))")
        .expect("scm lowers");
    assert_eq!(
        follows.rule,
        AstRule::All(vec![
            kind("call_expression"),
            AstRule::Not(Box::new(AstRule::Follows {
                rule: Box::new(AstRule::Matches("s".into())),
                stop_by: Some(StopBy::End("end".into())),
            })),
        ])
    );

    let regex = lower_scm("((identifier) @m (#not-match? @m \"^_\"))").expect("scm lowers");
    assert_eq!(
        regex.rule,
        AstRule::All(vec![
            kind("identifier"),
            AstRule::Not(Box::new(AstRule::Regex("^_".into()))),
        ])
    );
}

#[test]
fn an_unmapped_not_predicate_reports_its_unpeeled_spelling() {
    assert_eq!(
        lower_scm("(block) @s\n\n((call_expression) @m (#not-nope? @m s))"),
        Err(ScmLowerError::UnknownPredicate("not-nope?".into()))
    );
}

#[test]
fn not_inside_and_inside_partition_the_same_corpus() {
    let scope = "[(closure_expression)] @scope\n\n";
    let inside = run(
        "probe.rs",
        RUST_SRC,
        &format!("{scope}((field_identifier) @m (#inside? @m scope))"),
    );
    let outside = run(
        "probe.rs",
        RUST_SRC,
        &format!("{scope}((field_identifier) @m (#not-inside? @m scope))"),
    );
    let all = run("probe.rs", RUST_SRC, "(field_identifier) @m");
    assert_eq!(inside.len() + outside.len(), all.len());
    assert_eq!(inside, Vec::new());
    assert_eq!(outside, all);
}
