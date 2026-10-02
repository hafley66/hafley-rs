use hafley_scm::MatchArena;

const RUST: &str = "fn host() { a(); let x = 0; b(); c(); }";
const TS: &str = "function host() { a(); let x = 0; b(); c(); }";

struct Row {
    name: &'static str,
    args: &'static str,
}

fn matches(language: &tree_sitter::Language, source: &str, scm: &str) -> String {
    let query = hafley_scm::build(language, scm).expect(scm);
    let tree = hafley_scm::cst::parse(language, source.as_bytes()).unwrap();
    let mut arena = MatchArena::default();
    hafley_scm::run(
        &query,
        "fixture",
        source.as_bytes(),
        &tree,
        u32::MAX,
        &mut arena,
    )
    .unwrap();
    arena
        .rows
        .iter()
        .map(|row| {
            arena.spans[row.spans.start as usize..row.spans.end as usize]
                .iter()
                .map(|span| {
                    format!(
                        "{}={}",
                        query.names[span.name as usize],
                        &source[span.bytes.start as usize..span.bytes.end as usize]
                    )
                })
                .collect::<Vec<_>>()
                .join(", ")
        })
        .collect::<Vec<_>>()
        .join(" | ")
}

#[test]
fn relational_siblings() {
    let rows = [
        // expression_statement:has(+ expression_statement)
        Row {
            name: "precedes_neighbor",
            args: "#precedes? @c expression_statement neighbor",
        },
        // expression_statement:has(~ expression_statement)
        Row {
            name: "precedes_end",
            args: "#precedes? @c expression_statement end",
        },
        // expression_statement + expression_statement
        Row {
            name: "follows_neighbor",
            args: "#follows? @c expression_statement neighbor",
        },
        // expression_statement ~ expression_statement
        Row {
            name: "follows_end",
            args: "#follows? @c expression_statement end",
        },
        // expression_statement:not(:has(+ expression_statement))
        Row {
            name: "not_precedes_neighbor",
            args: "#not-precedes? @c expression_statement neighbor",
        },
        // expression_statement:not(:has(~ expression_statement))
        Row {
            name: "not_precedes_end",
            args: "#not-precedes? @c expression_statement end",
        },
        // expression_statement:not(expression_statement + expression_statement)
        Row {
            name: "not_follows_neighbor",
            args: "#not-follows? @c expression_statement neighbor",
        },
        // expression_statement:not(expression_statement ~ expression_statement)
        Row {
            name: "not_follows_end",
            args: "#not-follows? @c expression_statement end",
        },
    ];
    let actual = [
        (
            "rust",
            tree_sitter::Language::new(tree_sitter_rust::LANGUAGE),
            RUST,
        ),
        (
            "ts",
            tree_sitter::Language::new(tree_sitter_typescript::LANGUAGE_TYPESCRIPT),
            TS,
        ),
    ]
    .iter()
    .flat_map(|(name, language, source)| {
        rows.iter().map(move |row| {
            let scm = format!("((expression_statement) @c ({}))", row.args);
            format!("{name}/{}: {}", row.name, matches(language, source, &scm))
        })
    })
    .collect::<Vec<_>>()
    .join("\n");
    assert_eq!(actual, "rust/precedes_neighbor: c=b();\nrust/precedes_end: c=a(); | c=b();\nrust/follows_neighbor: c=c();\nrust/follows_end: c=b(); | c=c();\nrust/not_precedes_neighbor: c=a(); | c=c();\nrust/not_precedes_end: c=c();\nrust/not_follows_neighbor: c=a(); | c=b();\nrust/not_follows_end: c=a();\nts/precedes_neighbor: c=b();\nts/precedes_end: c=a(); | c=b();\nts/follows_neighbor: c=c();\nts/follows_end: c=b(); | c=c();\nts/not_precedes_neighbor: c=a(); | c=c();\nts/not_precedes_end: c=c();\nts/not_follows_neighbor: c=a(); | c=b();\nts/not_follows_end: c=a();");
}

#[test]
fn relational_nth_child() {
    let rows = [
        // expression_statement:nth-child(3)
        Row {
            name: "nth_child",
            args: "#nth-child? @c 3",
        },
        // expression_statement:nth-child(2 of expression_statement)
        Row {
            name: "nth_child_of",
            args: "#nth-child? @c 2 of expression_statement",
        },
        // expression_statement:not(:nth-child(3))
        Row {
            name: "not_nth_child",
            args: "#not-nth-child? @c 3",
        },
        // expression_statement:not(:nth-child(2 of expression_statement))
        Row {
            name: "not_nth_child_of",
            args: "#not-nth-child? @c 2 of expression_statement",
        },
    ];
    let actual = [
        (
            "rust",
            tree_sitter::Language::new(tree_sitter_rust::LANGUAGE),
            RUST,
        ),
        (
            "ts",
            tree_sitter::Language::new(tree_sitter_typescript::LANGUAGE_TYPESCRIPT),
            TS,
        ),
    ]
    .iter()
    .flat_map(|(name, language, source)| {
        rows.iter().map(move |row| {
            let scm = format!("((expression_statement) @c ({}))", row.args);
            format!("{name}/{}: {}", row.name, matches(language, source, &scm))
        })
    })
    .collect::<Vec<_>>()
    .join("\n");
    assert_eq!(actual, "rust/nth_child: c=b();\nrust/nth_child_of: c=b();\nrust/not_nth_child: c=a(); | c=c();\nrust/not_nth_child_of: c=a(); | c=c();\nts/nth_child: c=b();\nts/nth_child_of: c=b();\nts/not_nth_child: c=a(); | c=c();\nts/not_nth_child_of: c=a(); | c=c();");
}

#[test]
fn relational_patterns_and_options() {
    struct PatternRow {
        name: &'static str,
        candidate: &'static str,
        args: &'static str,
    }
    let rows = [
        // expression_statement:has(call_expression > identifier[field="function"])
        PatternRow { name: "has_pattern", candidate: "(expression_statement) @c", args: "#has? @c (call_expression function: (identifier) @callee)" },
        // expression_statement:has(> call_expression)
        PatternRow { name: "has_neighbor", candidate: "(expression_statement) @c", args: "#has? @c (call_expression) stopBy: neighbor" },
        // expression_statement:has(identifier[field="function"])
        PatternRow { name: "has_field", candidate: "(expression_statement) @c", args: "#has? @c (identifier) field: function stopBy: end" },
        // expression_statement:has(call_expression), inclusive boundary
        PatternRow { name: "has_stop_inclusive", candidate: "(expression_statement) @c", args: "#has? @c (call_expression) stopBy: (call_expression)" },
        // expression_statement:has(> identifier)
        PatternRow { name: "has_pattern_neighbor_miss", candidate: "(expression_statement) @c", args: "#has? @c (identifier) stopBy: neighbor" },
        // expression_statement:has(identifier):not(:has(> call_expression)); fixture bounded walk
        PatternRow { name: "has_stop_before", candidate: "(expression_statement) @c", args: "#has? @c (identifier) stopBy: (call_expression)" },
        // expression_statement:not(:has(call_expression))
        PatternRow { name: "not_has_pattern", candidate: "(expression_statement) @c", args: "#not-has? @c (call_expression)" },
        // $function:has(> identifier[field="name"]) call_expression
        PatternRow { name: "ancestor_pattern", candidate: "(call_expression) @c", args: "#has-ancestor? @c ($function name: (identifier) @fn)" },
        // expression_statement > call_expression
        PatternRow { name: "ancestor_neighbor", candidate: "(call_expression) @c", args: "#has-ancestor? @c (expression_statement) stopBy: neighbor" },
        // $function > call_expression
        PatternRow { name: "ancestor_neighbor_miss", candidate: "(call_expression) @c", args: "#has-ancestor? @c ($function) stopBy: neighbor" },
        // $block[field="body"] call_expression
        PatternRow { name: "ancestor_field", candidate: "(call_expression) @c", args: "#has-ancestor? @c ($block) field: body stopBy: end" },
        // call_expression:not($function call_expression), bounded at expression_statement
        PatternRow { name: "ancestor_stop_before", candidate: "(call_expression) @c", args: "#has-ancestor? @c ($function) stopBy: (expression_statement)" },
        // $function call_expression, inclusive boundary
        PatternRow { name: "ancestor_stop_inclusive", candidate: "(call_expression) @c", args: "#has-ancestor? @c ($function) stopBy: ($function)" },
        // call_expression:not($function call_expression)
        PatternRow { name: "not_ancestor_pattern", candidate: "(call_expression) @c", args: "#not-has-ancestor? @c ($function)" },
        // expression_statement > call_expression
        PatternRow { name: "parent_pattern", candidate: "(call_expression) @c", args: "#has-parent? @c (expression_statement)" },
        // expression_statement > call_expression
        PatternRow { name: "parent_neighbor", candidate: "(call_expression) @c", args: "#has-parent? @c (expression_statement) stopBy: neighbor" },
        // expression_statement > call_expression; parent remains direct with end
        PatternRow { name: "parent_end", candidate: "(call_expression) @c", args: "#has-parent? @c (expression_statement) stopBy: end" },
        // $function > call_expression
        PatternRow { name: "parent_end_strict", candidate: "(call_expression) @c", args: "#has-parent? @c ($function) stopBy: end" },
        // $block[field="body"] > expression_statement
        PatternRow { name: "parent_field", candidate: "(expression_statement) @c", args: "#has-parent? @c ($block) field: body" },
        // expression_statement > call_expression, inclusive boundary
        PatternRow { name: "parent_stop_inclusive", candidate: "(call_expression) @c", args: "#has-parent? @c (expression_statement) stopBy: (expression_statement)" },
        // call_expression:not(expression_statement > call_expression)
        PatternRow { name: "not_parent_pattern", candidate: "(call_expression) @c", args: "#not-has-parent? @c (expression_statement)" },
        // expression_statement:has(+ expression_statement:has(call_expression > identifier))
        PatternRow { name: "precedes_pattern_neighbor", candidate: "(expression_statement) @c", args: "#precedes? @c (expression_statement (call_expression function: (identifier) @next)) stopBy: neighbor" },
        // expression_statement:has(~ expression_statement)
        PatternRow { name: "precedes_pattern_end", candidate: "(expression_statement) @c", args: "#precedes? @c (expression_statement) stopBy: end" },
        // expression_statement:not(:has(+ expression_statement))
        PatternRow { name: "not_precedes_pattern", candidate: "(expression_statement) @c", args: "#not-precedes? @c (expression_statement) neighbor" },
        // expression_statement:has(~ expression_statement), bounded at declaration
        PatternRow { name: "precedes_stop", candidate: "(expression_statement) @c", args: "#precedes? @c (expression_statement) stopBy: ($declaration)" },
        // expression_statement:has(~ expression_statement), inclusive boundary
        PatternRow { name: "precedes_stop_inclusive", candidate: "(expression_statement) @c", args: "#precedes? @c (expression_statement) stopBy: (expression_statement)" },
        // expression_statement + expression_statement
        PatternRow { name: "follows_pattern_neighbor", candidate: "(expression_statement) @c", args: "#follows? @c (expression_statement (call_expression function: (identifier) @previous)) stopBy: neighbor" },
        // expression_statement ~ expression_statement
        PatternRow { name: "follows_pattern_end", candidate: "(expression_statement) @c", args: "#follows? @c (expression_statement) stopBy: end" },
        // expression_statement:not(expression_statement + expression_statement)
        PatternRow { name: "not_follows_pattern", candidate: "(expression_statement) @c", args: "#not-follows? @c (expression_statement) neighbor" },
        // expression_statement ~ expression_statement, bounded at declaration
        PatternRow { name: "follows_stop", candidate: "(expression_statement) @c", args: "#follows? @c (expression_statement) stopBy: ($declaration)" },
        // expression_statement ~ expression_statement, inclusive boundary
        PatternRow { name: "follows_stop_inclusive", candidate: "(expression_statement) @c", args: "#follows? @c (expression_statement) stopBy: (expression_statement)" },
        // expression_statement:nth-child(2 of expression_statement:has(call_expression))
        PatternRow { name: "nth_child_pattern", candidate: "(expression_statement) @c", args: "#nth-child? @c 2 of (expression_statement (call_expression function: (identifier) @name))" },
        // expression_statement:not(:nth-child(2 of expression_statement))
        PatternRow { name: "not_nth_child_pattern", candidate: "(expression_statement) @c", args: "#not-nth-child? @c 2 of (expression_statement)" },
    ];
    for (name, language, source, function, block, declaration) in [
        (
            "rust",
            tree_sitter::Language::new(tree_sitter_rust::LANGUAGE),
            RUST,
            "function_item",
            "block",
            "let_declaration",
        ),
        (
            "ts",
            tree_sitter::Language::new(tree_sitter_typescript::LANGUAGE_TYPESCRIPT),
            TS,
            "function_declaration",
            "statement_block",
            "lexical_declaration",
        ),
    ] {
        let actual = rows
            .iter()
            .map(|row| {
                let args = row
                    .args
                    .replace("$function", function)
                    .replace("$block", block)
                    .replace("$declaration", declaration);
                let scm = format!("({} ({args}))", row.candidate);
                format!("{}: {}", row.name, matches(&language, source, &scm))
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(actual, "has_pattern: c=a();, callee=a | c=b();, callee=b | c=c();, callee=c\nhas_neighbor: c=a(); | c=b(); | c=c();\nhas_field: c=a(); | c=b(); | c=c();\nhas_stop_inclusive: c=a(); | c=b(); | c=c();\nhas_pattern_neighbor_miss: \nhas_stop_before: \nnot_has_pattern: \nancestor_pattern: c=a(), fn=host | c=b(), fn=host | c=c(), fn=host\nancestor_neighbor: c=a() | c=b() | c=c()\nancestor_neighbor_miss: \nancestor_field: c=a() | c=b() | c=c()\nancestor_stop_before: \nancestor_stop_inclusive: c=a() | c=b() | c=c()\nnot_ancestor_pattern: \nparent_pattern: c=a() | c=b() | c=c()\nparent_neighbor: c=a() | c=b() | c=c()\nparent_end: c=a() | c=b() | c=c()\nparent_end_strict: \nparent_field: c=a(); | c=b(); | c=c();\nparent_stop_inclusive: c=a() | c=b() | c=c()\nnot_parent_pattern: \nprecedes_pattern_neighbor: c=b();, next=c\nprecedes_pattern_end: c=a(); | c=b();\nnot_precedes_pattern: c=a(); | c=c();\nprecedes_stop: c=b();\nprecedes_stop_inclusive: c=a(); | c=b();\nfollows_pattern_neighbor: c=c();, previous=b\nfollows_pattern_end: c=b(); | c=c();\nnot_follows_pattern: c=a(); | c=b();\nfollows_stop: c=c();\nfollows_stop_inclusive: c=b(); | c=c();\nnth_child_pattern: c=b();, name=b\nnot_nth_child_pattern: c=a(); | c=c();", "{name}");
    }
}

#[test]
fn relational_fields_and_anonymous_siblings() {
    let rows = [
        // binary_expression:has(> identifier[field="left"])
        Row {
            name: "has_left",
            args: "#has? @c (identifier) field: left neighbor",
        },
        // binary_expression:has(> identifier[field="right"])
        Row {
            name: "has_right",
            args: "#has? @c identifier field: right",
        },
        // identifier:has(+ identifier[field="right"])
        Row {
            name: "precedes_right",
            args: "#precedes? @c (identifier) field: right neighbor",
        },
        // identifier[field="left"] + identifier
        Row {
            name: "follows_left",
            args: "#follows? @c (identifier) field: left neighbor",
        },
        // identifier:not(:has(+ identifier[field="right"]))
        Row {
            name: "not_precedes_right",
            args: "#not-precedes? @c (identifier) field: right neighbor",
        },
        // identifier:not(identifier[field="left"] + identifier)
        Row {
            name: "not_follows_left",
            args: "#not-follows? @c (identifier) field: left neighbor",
        },
        // identifier:nth-child(2)
        Row {
            name: "named_nth",
            args: "#nth-child? @c 2",
        },
        // binary_expression[field="value"] > identifier
        Row {
            name: "parent_value",
            args: "#has-parent? @c (binary_expression) field: value",
        },
        // binary_expression[field="value"] identifier
        Row {
            name: "ancestor_value",
            args: "#has-ancestor? @c (binary_expression) field: value",
        },
    ];
    for (name, language, source) in [
        (
            "rust",
            tree_sitter::Language::new(tree_sitter_rust::LANGUAGE),
            "fn host() { let x = a + b; }",
        ),
        (
            "ts",
            tree_sitter::Language::new(tree_sitter_typescript::LANGUAGE_TYPESCRIPT),
            "function host() { let x = a + b; }",
        ),
    ] {
        let actual = rows
            .iter()
            .map(|row| {
                let candidate = if row.name.starts_with("has_") {
                    "(binary_expression) @c"
                } else {
                    "(binary_expression (identifier) @c)"
                };
                let scm = format!("({candidate} ({}))", row.args);
                format!("{}: {}", row.name, matches(&language, source, &scm))
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(actual, "has_left: c=a + b\nhas_right: c=a + b\nprecedes_right: c=a\nfollows_left: c=b\nnot_precedes_right: c=b\nnot_follows_left: c=a\nnamed_nth: c=b\nparent_value: c=a | c=b\nancestor_value: c=a | c=b", "{name}");
    }
}

#[test]
fn relational_bindings_emit_and_filter() {
    for (language, source, function) in [
        (
            tree_sitter::Language::new(tree_sitter_rust::LANGUAGE),
            RUST,
            "function_item",
        ),
        (
            tree_sitter::Language::new(tree_sitter_typescript::LANGUAGE_TYPESCRIPT),
            TS,
            "function_declaration",
        ),
    ] {
        // function name and contained call are bound in one row.
        // $function:has(> identifier[field="name"]) call_expression
        let scm = format!(
            r#"((call_expression) @c
            (#has-ancestor? @c ({function} name: (identifier) @fn))
            (#contains? @fn "host")
            (#emit! "enclosing" "call" @c "function" @fn))"#
        );
        let q = hafley_scm::build(&language, &scm).unwrap();
        let tree = hafley_scm::cst::parse(&language, source.as_bytes()).unwrap();
        let mut arena = MatchArena::default();
        hafley_scm::run(
            &q,
            "fixture",
            source.as_bytes(),
            &tree,
            u32::MAX,
            &mut arena,
        )
        .unwrap();
        let actual = arena
            .emitted
            .iter()
            .map(|fact| {
                ["call", "function"]
                    .iter()
                    .map(|field| {
                        let value = fact
                            .get(&arena, q.field_id(field).unwrap())
                            .unwrap()
                            .text(source.as_bytes(), &q)
                            .unwrap();
                        format!("{field}={value}")
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .collect::<Vec<_>>()
            .join(" | ");
        assert_eq!(
            actual,
            "call=a(), function=host | call=b(), function=host | call=c(), function=host"
        );
    }
}

#[test]
fn relational_argument_errors() {
    let language = tree_sitter::Language::new(tree_sitter_rust::LANGUAGE);
    let rows = [
        ("zero", "#nth-child? @c 0", "arity"),
        ("negative", "#nth-child? @c -1", "arity"),
        ("bad_of", "#nth-child? @c 2 of missing_kind", "unknown"),
        ("unknown_pattern", "#has? @c (missing_kind)", "parse"),
        (
            "unknown_nested",
            "#has-ancestor? @c (function_item name: (missing_kind))",
            "parse",
        ),
        (
            "unknown_stop",
            "#follows? @c identifier stopBy: (missing_kind)",
            "parse",
        ),
        (
            "unknown_field",
            "#has? @c identifier field: missing_field",
            "unknown",
        ),
        (
            "duplicate_stop",
            "#precedes? @c identifier stopBy: end neighbor",
            "arity",
        ),
        ("missing_option", "#has? @c identifier field:", "arity"),
        (
            "mixed_pattern_kind",
            "#has? @c (identifier) identifier",
            "arity",
        ),
        ("native_nested", "#eq? @c (identifier)", "parse"),
        ("bad_pattern", "#has? @c (identifier", "parse"),
    ];
    let actual = rows
        .iter()
        .map(|(name, args, expected)| {
            let scm = format!("((identifier) @c ({args}))");
            let error = match hafley_scm::build(&language, &scm) {
                Err(hafley_scm::QueryExtError::Arity { .. }) => "arity",
                Err(hafley_scm::QueryExtError::UnknownOperator(_)) => "unknown",
                Err(hafley_scm::QueryExtError::Parse(_)) => "parse",
                _ => panic!("expected {expected}: {scm}"),
            };
            format!("{name}: {error}")
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(actual, "zero: arity\nnegative: arity\nbad_of: unknown\nunknown_pattern: parse\nunknown_nested: parse\nunknown_stop: parse\nunknown_field: unknown\nduplicate_stop: arity\nmissing_option: arity\nmixed_pattern_kind: arity\nnative_nested: parse\nbad_pattern: parse");
}

#[test]
fn relational_query_syntax_and_root_capture() {
    let language = tree_sitter::Language::new(tree_sitter_rust::LANGUAGE);
    // function_item:has(> identifier[field="name"]):has(call_expression)
    let native = r#"((call_expression) @c
        (#has-ancestor? @c
            ((function_item name: (identifier) @fn)
             ; Ignore delimiters in this comment: ) ( [
             (#eq? @fn "host"))))"#;
    assert_eq!(
        matches(&language, RUST, native),
        "c=a(), fn=host | c=b(), fn=host | c=c(), fn=host"
    );
    // expression_statement > call_expression; bind the parent node.
    let parent = "((call_expression) @c (#has-parent? @c (expression_statement) @parent))";
    assert_eq!(
        matches(&language, RUST, parent),
        "c=a(), parent=a(); | c=b(), parent=b(); | c=c(), parent=c();"
    );
    // expression_statement:has(> call_expression), alternation remains native query syntax.
    let alternatives =
        "((expression_statement) @c (#has? @c [(call_expression) (binary_expression)] neighbor))";
    assert_eq!(
        matches(&language, RUST, alternatives),
        "c=a(); | c=b(); | c=c();"
    );
    // source_file:not(:nth-child(1)); the root has no parent.
    assert_eq!(
        matches(&language, RUST, "((source_file) @c (#nth-child? @c 1))"),
        ""
    );
}

#[test]
fn relational_parent_reused_capture() {
    let language = tree_sitter::Language::new(tree_sitter_rust::LANGUAGE);
    // expression_statement > call_expression; both nodes use capture c.
    let query = "((call_expression) @c (#has-parent? @c (expression_statement) @c))";
    assert_eq!(
        matches(&language, RUST, query),
        "c=a(), c=a(); | c=b(), c=b(); | c=c(), c=c();"
    );
}

#[test]
fn relational_has_strict_and_rejected_bindings() {
    let language = tree_sitter::Language::new(tree_sitter_rust::LANGUAGE);
    let rows = [
        // call_expression:has(call_expression); the target is excluded.
        Row {
            name: "strict_descendant",
            args: "#has? @c (call_expression)",
        },
        // call_expression:has(> identifier[field="function"])
        Row {
            name: "direct_capture",
            args: "#has? @c (identifier) @callee neighbor",
        },
        // call_expression:not(:has(identifier))
        Row {
            name: "negated_binding",
            args: "#not-has? @c (identifier) @callee",
        },
    ];
    let actual = rows
        .iter()
        .map(|row| {
            let scm = format!("((call_expression) @c ({}))", row.args);
            format!("{}: {}", row.name, matches(&language, RUST, &scm))
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(actual, "strict_descendant: \ndirect_capture: c=a(), callee=a | c=b(), callee=b | c=c(), callee=c\nnegated_binding: ");
    let scm =
        "((call_expression) @c (#has? @c (identifier) @callee) (#contains? @callee \"missing\"))";
    let query = hafley_scm::build(&language, scm).unwrap();
    let tree = hafley_scm::cst::parse(&language, RUST.as_bytes()).unwrap();
    let mut arena = MatchArena::default();
    hafley_scm::run(
        &query,
        "fixture",
        RUST.as_bytes(),
        &tree,
        u32::MAX,
        &mut arena,
    )
    .unwrap();
    assert_eq!(
        format!("rows={} spans={}", arena.rows.len(), arena.spans.len()),
        "rows=0 spans=0"
    );
}
