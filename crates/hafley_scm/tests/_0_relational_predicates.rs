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
    hafley_scm::run(&query, "fixture", source.as_bytes(), &tree, u32::MAX, &mut arena).unwrap();
    arena.rows.iter().map(|row| {
        arena.spans[row.spans.start as usize..row.spans.end as usize].iter().map(|span| {
            format!("{}={}", query.names[span.name as usize], &source[span.bytes.start as usize..span.bytes.end as usize])
        }).collect::<Vec<_>>().join(", ")
    }).collect::<Vec<_>>().join(" | ")
}

#[test]
fn relational_siblings() {
    let rows = [
        // expression_statement:has(+ expression_statement)
        Row { name: "precedes_neighbor", args: "#precedes? @c expression_statement stopBy: neighbor" },
        // expression_statement:has(~ expression_statement)
        Row { name: "precedes_end", args: "#precedes? @c expression_statement stopBy: end" },
        // expression_statement + expression_statement
        Row { name: "follows_neighbor", args: "#follows? @c expression_statement stopBy: neighbor" },
        // expression_statement ~ expression_statement
        Row { name: "follows_end", args: "#follows? @c expression_statement stopBy: end" },
        // expression_statement:not(:has(+ expression_statement))
        Row { name: "not_precedes_neighbor", args: "#not-precedes? @c expression_statement stopBy: neighbor" },
        // expression_statement:not(:has(~ expression_statement))
        Row { name: "not_precedes_end", args: "#not-precedes? @c expression_statement stopBy: end" },
        // expression_statement:not(expression_statement + expression_statement)
        Row { name: "not_follows_neighbor", args: "#not-follows? @c expression_statement stopBy: neighbor" },
        // expression_statement:not(expression_statement ~ expression_statement)
        Row { name: "not_follows_end", args: "#not-follows? @c expression_statement stopBy: end" },
    ];
    let actual = [
        ("rust", tree_sitter::Language::new(tree_sitter_rust::LANGUAGE), RUST),
        ("ts", tree_sitter::Language::new(tree_sitter_typescript::LANGUAGE_TYPESCRIPT), TS),
    ].iter().flat_map(|(name, language, source)| rows.iter().map(move |row| {
        let scm = format!("((expression_statement) @c ({}))", row.args);
        format!("{name}/{}: {}", row.name, matches(language, source, &scm))
    })).collect::<Vec<_>>().join("\n");
    assert_eq!(actual, "rust/precedes_neighbor: c=b();\nrust/precedes_end: c=a(); | c=b();\nrust/follows_neighbor: c=c();\nrust/follows_end: c=b(); | c=c();\nrust/not_precedes_neighbor: c=a(); | c=c();\nrust/not_precedes_end: c=c();\nrust/not_follows_neighbor: c=a(); | c=b();\nrust/not_follows_end: c=a();\nts/precedes_neighbor: c=b();\nts/precedes_end: c=a(); | c=b();\nts/follows_neighbor: c=c();\nts/follows_end: c=b(); | c=c();\nts/not_precedes_neighbor: c=a(); | c=c();\nts/not_precedes_end: c=c();\nts/not_follows_neighbor: c=a(); | c=b();\nts/not_follows_end: c=a();");
}

#[test]
fn relational_nth_child() {
    let rows = [
        // expression_statement:nth-child(3)
        Row { name: "nth_child", args: "#nth-child? @c 3" },
        // expression_statement:nth-child(2 of expression_statement)
        Row { name: "nth_child_of", args: "#nth-child? @c 2 of expression_statement" },
        // expression_statement:not(:nth-child(3))
        Row { name: "not_nth_child", args: "#not-nth-child? @c 3" },
        // expression_statement:not(:nth-child(2 of expression_statement))
        Row { name: "not_nth_child_of", args: "#not-nth-child? @c 2 of expression_statement" },
    ];
    let actual = [
        ("rust", tree_sitter::Language::new(tree_sitter_rust::LANGUAGE), RUST),
        ("ts", tree_sitter::Language::new(tree_sitter_typescript::LANGUAGE_TYPESCRIPT), TS),
    ].iter().flat_map(|(name, language, source)| rows.iter().map(move |row| {
        let scm = format!("((expression_statement) @c ({}))", row.args);
        format!("{name}/{}: {}", row.name, matches(language, source, &scm))
    })).collect::<Vec<_>>().join("\n");
    assert_eq!(actual, "rust/nth_child: c=b();\nrust/nth_child_of: c=b();\nrust/not_nth_child: c=a(); | c=c();\nrust/not_nth_child_of: c=a(); | c=c();\nts/nth_child: c=b();\nts/nth_child_of: c=b();\nts/not_nth_child: c=a(); | c=c();\nts/not_nth_child_of: c=a(); | c=c();");
}

#[test]
fn nth_child_of_supertype_expands_subtypes() {
    let language = tree_sitter::Language::new(tree_sitter_typescript::LANGUAGE_TYPESCRIPT);
    let scm = "((expression_statement) @c (#nth-child? @c 3 of statement))";
    assert_eq!(matches(&language, TS, scm), "c=b();");
}


#[test]
fn build_routes_nested_patterns_through_scmpp() {
    let language = tree_sitter::Language::new(tree_sitter_rust::LANGUAGE);
    let source = "fn host() { a(); }\nfn other() { b(); }";
    let nested = "((call_expression) @c\n (#has-ancestor? @c (function_item name: (identifier) @fn (#eq? @fn \"host\"))))";
    let each = "((call_expression) @c (#has-ancestor? @c (function_item name: (identifier) @fn) rows: each))";
    let error = match hafley_scm::build(&language, each) {
        Err(error) => format!("{error:?}"),
        Ok(_) => "built".into(),
    };
    assert_eq!(
        format!("{}\n{error}", matches(&language, source, nested)),
        "c=a()\nScmpp(Unsupported(\"@fn is a rows: each capture; query --scmpp returns those rows\"))"
    );
}
