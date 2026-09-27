use sprefa_lab_scopegraph::{analyze, query, sprefa_query};
use std::path::Path;
use tree_sitter::Language;

fn rust() -> Language {
    Language::new(tree_sitter_rust::LANGUAGE)
}

#[test]
fn typescript_reuses_the_scope_engine_without_language_engine_edits() {
    let source = "function greet(name: string): string { return name; }";
    let query_source = format!(
        "{}\n{}\n{}",
        include_str!("../queries/_typescript/locals.scm"),
        include_str!("../queries/ecma/locals.scm"),
        include_str!("../queries/typescript/locals.scm")
    );
    let language = Language::new(tree_sitter_typescript::LANGUAGE_TYPESCRIPT);
    let graph = analyze(language, source, &query_source, Path::new("greet.ts")).unwrap();
    let name_reference = graph
        .references
        .iter()
        .find(|reference| reference.name == "name")
        .unwrap();
    let definition = &graph.definitions[name_reference.definition.unwrap()];
    assert_eq!(definition.role, "variable.parameter");
    assert_eq!(definition.capture.text, "name");
}

#[test]
fn sprefa_dependency_supplies_the_existing_parse_and_query_entry() {
    let spans = sprefa_query("rust", "(identifier) @name", b"fn f() {} ").unwrap();
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].captures[0].text, "f");
}

#[test]
fn vendored_kotlin_locals_query_compiles_with_its_declared_grammar() {
    let language = Language::new(tree_sitter_kotlin_sg::LANGUAGE);
    let source = include_str!("../queries/kotlin/locals.scm");
    tree_sitter::Query::new(&language, source).unwrap();
}

#[test]
fn kotlin_query_additions_capture_calls_imports_and_alias_definitions() {
    let source = "import com.acme.Widget as Alias\nfun use(value: String) { val local = value; consume(local); Alias() }";
    let language = Language::new(tree_sitter_kotlin_sg::LANGUAGE);
    let captures = query::run_query(
        language,
        source,
        include_str!("../queries/kotlin/locals.scm"),
        Path::new("imports.kt"),
    )
    .unwrap()
    .into_iter()
    .flat_map(|matched| matched.captures.into_iter())
    .collect::<Vec<_>>();
    assert!(captures
        .iter()
        .any(|capture| capture.label == "local.definition.namespace" && capture.text == "Alias"));
    assert!(captures
        .iter()
        .any(|capture| capture.label == "local.reference" && capture.text == "com.acme.Widget"));
    assert!(captures
        .iter()
        .any(|capture| capture.label == "local.reference" && capture.text == "consume"));
}

#[test]
fn kotlin_corpus_resolves_explicit_and_wildcard_imports_and_same_package_names() {
    let mut corpus = sprefa_lab_scopegraph::corpus::Corpus::build_kotlin([
        (
            "model/Widget.kt".into(),
            "package com.acme.model\nfun build() {}\nfun lone() {}".into(),
        ),
        (
            "app/Main.kt".into(),
            "package com.acme.app\nimport com.acme.model.build as make\nimport com.acme.model.*\nfun main() { make(); lone() }".into(),
        ),
        (
            "app/Helper.kt".into(),
            "package com.acme.app\nfun helper() {}\nfun use() { helper() }".into(),
        ),
    ])
    .unwrap();
    corpus.resolve();
    let app = &corpus.units["app/Main.kt"];
    let resolved = app
        .graph
        .references
        .iter()
        .filter_map(|reference| {
            reference
                .external
                .as_ref()
                .map(|external| (reference, external))
        })
        .collect::<Vec<_>>();
    assert!(resolved.iter().any(|(reference, external)| {
        reference.name == "make" && external.name == "build" && external.path == "model/Widget.kt"
    }));
    assert!(resolved.iter().any(|(reference, external)| {
        reference.name == "lone" && external.path == "model/Widget.kt"
    }));
    let helper = &corpus.units["app/Helper.kt"];
    assert!(helper.graph.references.iter().any(|reference| {
        reference
            .external
            .as_ref()
            .is_some_and(|target| target.name == "helper" && target.resolution == "same_package")
    }));
}

#[test]
fn qualified_symbol_stack_pushes_the_tail_before_its_package() {
    assert_eq!(
        sprefa_lab_scopegraph::corpus::symbol_stack("a.b"),
        [
            sprefa_lab_scopegraph::corpus::SymbolOp::Push("b".into()),
            sprefa_lab_scopegraph::corpus::SymbolOp::Push("a".into()),
            sprefa_lab_scopegraph::corpus::SymbolOp::Pop("a".into()),
            sprefa_lab_scopegraph::corpus::SymbolOp::Pop("b".into()),
        ]
    );
}

#[test]
fn kotlin_scope_tree_resolves_parameters_and_nested_shadowing() {
    let source = r#"fun outer(input: String): String {
    val value = input
    val nested = run {
        val input = "inner"
        value + input
    }
    return value
}"#;
    let language = Language::new(tree_sitter_kotlin_sg::LANGUAGE);
    let graph = analyze(
        language,
        source,
        include_str!("../queries/kotlin/locals.scm"),
        Path::new("scope.kt"),
    )
    .unwrap();
    let input_definitions = graph
        .definitions
        .iter()
        .filter(|definition| definition.name == "input")
        .collect::<Vec<_>>();
    let input_references = graph
        .references
        .iter()
        .filter(|reference| reference.name == "input")
        .collect::<Vec<_>>();
    assert_eq!(input_definitions.len(), 2);
    assert_eq!(input_references.len(), 2);
    assert_eq!(
        graph.definitions[input_references[0].definition.unwrap()]
            .capture
            .start,
        input_definitions[0].capture.start
    );
    assert_eq!(
        graph.definitions[input_references[1].definition.unwrap()]
            .capture
            .start,
        input_definitions[1].capture.start
    );
    assert!(graph
        .references
        .iter()
        .any(|reference| reference.name == "run" && reference.unresolved.is_some()));
}

const SOURCE: &str =
    "fn outer() { let text = \"a\"; text.contains(\"a\"); let f = || text.contains(\"b\"); }";

#[test]
fn inside_predicate_filters_closure_calls_and_uses_match_grouping() {
    let query_source = r#"
[(closure_expression)] @closure
((call_expression) @call (#inside? @call closure))
"#;
    let matches = query::run_query(rust(), SOURCE, query_source, Path::new("inside.rs")).unwrap();
    assert_eq!(
        matches
            .iter()
            .flat_map(|matched| matched.captures.iter())
            .filter(|capture| capture.label == "call")
            .map(|capture| capture.text.as_str())
            .collect::<Vec<_>>(),
        ["text.contains(\"b\")"]
    );
}

#[test]
fn has_precedes_and_follows_are_evaluated_through_general_predicates() {
    let has = query::run_query(
        rust(),
        SOURCE,
        "[(field_expression)] @field\n((call_expression) @call (#has? @call field))",
        Path::new("has.rs"),
    )
    .unwrap();
    assert_eq!(
        has.iter()
            .flat_map(|matched| matched.captures.iter())
            .filter(|capture| capture.label == "call")
            .count(),
        2
    );

    let follows = query::run_query(
        rust(),
        SOURCE,
        "[(let_declaration)] @decl\n((expression_statement) @stmt (#follows? @stmt decl))",
        Path::new("follows.rs"),
    )
    .unwrap();
    assert_eq!(
        follows
            .iter()
            .flat_map(|matched| matched.captures.iter())
            .filter(|capture| capture.label == "stmt")
            .map(|capture| capture.text.as_str())
            .collect::<Vec<_>>(),
        ["text.contains(\"a\");"]
    );

    let precedes = query::run_query(
        rust(),
        SOURCE,
        "[(let_declaration)] @decl\n((expression_statement) @stmt (#precedes? @stmt decl))",
        Path::new("precedes.rs"),
    )
    .unwrap();
    assert_eq!(
        precedes
            .iter()
            .flat_map(|matched| matched.captures.iter())
            .filter(|capture| capture.label == "stmt")
            .map(|capture| capture.text.as_str())
            .collect::<Vec<_>>(),
        ["text.contains(\"a\");"]
    );
}

#[test]
fn matches_preserves_multiple_captures_in_one_query_match() {
    let matches = query::run_query(
        rust(),
        SOURCE,
        "(call_expression function: (field_expression) @callee arguments: (arguments) @args)",
        Path::new("grouping.rs"),
    )
    .unwrap();
    assert_eq!(matches.len(), 2);
    assert!(matches.iter().all(|matched| matched.captures.len() == 2));
}

#[test]
fn match_limit_is_a_named_error_with_the_query_path() {
    let error = query::run_query_with_limit(
        rust(),
        "fn f() { a(); b(); c(); }",
        "(identifier) @name\n(identifier) @other",
        Path::new("fixture.rs"),
        1,
    )
    .unwrap_err();
    assert_eq!(
        error,
        query::QueryFailure::MatchLimit {
            path: "fixture.rs".into()
        }
    );
}

#[test]
fn unknown_general_predicate_is_a_named_error() {
    let error = query::run_query(
        rust(),
        "fn f() {}",
        "((identifier) @name (#unknown? @name \"x\"))",
        Path::new("unknown.scm"),
    )
    .unwrap_err();
    assert_eq!(
        error,
        query::QueryFailure::UnknownPredicate("unknown?".into())
    );
}
