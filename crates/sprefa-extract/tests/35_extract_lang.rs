use std::process::Command;

use ast_grep_core::Language;
use ast_grep_language::SupportLang;
use sprefa_extract::{
    decode_ast_rule_yaml, query_ast_rule, query_patterns, AstCaptureFact, AstPatternQuery,
    AstRuleError, ExtractLang,
};

const PROLOG_SAMPLE: &str = "tests/fixtures/prolog/0_sample.pl";
const MARKDOWN_SAMPLE: &str = "tests/fixtures/markdown/0_sample.md";

fn query(id: &str, pattern: &str, selector: Option<&str>, captures: &[&str]) -> AstPatternQuery {
    AstPatternQuery {
        id: id.into(),
        pattern: pattern.into(),
        selector: selector.map(Into::into),
        captures: captures.iter().map(|name| name.to_string()).collect(),
    }
}

fn captured(facts: &[AstCaptureFact], capture: &str) -> Vec<String> {
    facts
        .iter()
        .filter(|fact| fact.capture == capture)
        .map(|fact| fact.text.clone())
        .collect()
}

fn read(path: &str) -> Vec<u8> {
    std::fs::read(path).unwrap_or_else(|error| panic!("fixture {path}: {error}"))
}

#[test]
fn from_path_routes_the_extract_grammars_and_delegates_the_rest() {
    assert_eq!(ExtractLang::from_path("go.pl"), Some(ExtractLang::Prolog));
    assert_eq!(ExtractLang::from_path("t.plt"), Some(ExtractLang::Prolog));
    assert_eq!(ExtractLang::from_path("r.horn"), Some(ExtractLang::Prolog));
    assert_eq!(ExtractLang::from_path("a.md"), Some(ExtractLang::Markdown));
    assert_eq!(
        ExtractLang::from_path("a.markdown"),
        Some(ExtractLang::Markdown)
    );
    for (path, sg) in [
        ("a.rs", SupportLang::Rust),
        ("a.ts", SupportLang::TypeScript),
        ("a.tsx", SupportLang::Tsx),
        ("a.js", SupportLang::JavaScript),
        ("a.go", SupportLang::Go),
        ("a.kt", SupportLang::Kotlin),
    ] {
        assert_eq!(ExtractLang::from_path(path), Some(ExtractLang::Sg(sg)));
    }
    assert_eq!(ExtractLang::from_path("README"), None);
    assert_eq!(ExtractLang::from_path("a.unknownext"), None);
}

#[test]
fn every_lang_name_round_trips_through_the_yaml_spelling() {
    let mut langs = vec![
        ExtractLang::Prolog,
        ExtractLang::Markdown,
        ExtractLang::MarkdownInline,
    ];
    langs.extend(
        SupportLang::all_langs()
            .iter()
            .copied()
            .map(ExtractLang::Sg),
    );
    for lang in langs {
        assert_eq!(ExtractLang::parse_name(&lang.name()), Some(lang));
    }
    assert_eq!(ExtractLang::parse_name("not-a-grammar"), None);
}

/// `µ` is what ast-grep-language picks for every grammar whose identifiers take
/// Unicode letters (lib.rs:196-211). prolog is not such a grammar: `variable` is
/// `[A-Z]...` and `identifier` is `_*[a-z]...`, so `µT` parses to
/// `(ERROR (UNEXPECTED 181))` under it and `_T` is a plain variable. `_` is the
/// C/C++/CSS choice (ast-grep-language lib.rs:186-190).
/// @comment-ok: fail-first receipt, the sigil is why the two parse at all
#[test]
fn expando_char_is_underscore_for_prolog_and_mu_for_markdown() {
    assert_eq!(ExtractLang::Prolog.expando_char(), '_');
    assert_eq!(ExtractLang::Markdown.expando_char(), 'µ');
    assert_eq!(ExtractLang::MarkdownInline.expando_char(), 'µ');
    assert_eq!(
        ExtractLang::Sg(SupportLang::Rust).expando_char(),
        SupportLang::Rust.expando_char()
    );
    assert_eq!(
        ExtractLang::Sg(SupportLang::C).expando_char(),
        SupportLang::C.expando_char()
    );
    for lang in [ExtractLang::Prolog, ExtractLang::Markdown] {
        assert_eq!(lang.meta_var_char(), '$');
    }
}

/// The vendored `rewrite_dollar` has to stay the ast-grep-language rewrite
/// (lib.rs:88-97): same input, same output, sigil aside.
#[test]
fn pre_process_pattern_matches_the_ast_grep_rewrite() {
    for query in [
        "seen($T) <- $BODY.",
        "f($$$ARGS)",
        "f($$$)",
        "$$X",
        "$lowercase",
        "no metavar here",
        "$$$",
    ] {
        let ours = ExtractLang::Sg(SupportLang::Rust).pre_process_pattern(query);
        assert_eq!(
            ours,
            SupportLang::Rust.pre_process_pattern(query),
            "{query}"
        );
    }
    assert_eq!(ExtractLang::Markdown.pre_process_pattern("# $T"), "# µT");
}

#[test]
fn prolog_ast_pattern_matches_a_directive_goal() {
    let content = read(PROLOG_SAMPLE);
    let facts = query_patterns(
        PROLOG_SAMPLE,
        &content,
        &[query(
            "use_module",
            "use_module($MODULE, $IMPORTS)",
            None,
            &["MODULE", "IMPORTS"],
        )],
    )
    .expect("prolog pattern query");
    assert_eq!(captured(&facts, "MODULE"), vec!["'../shared/graph'"]);
    assert_eq!(captured(&facts, "IMPORTS"), vec!["[reachable/2, walk//1]"]);
    let text = String::from_utf8(content).expect("utf8 fixture");
    let only = facts.first().expect("one match");
    assert_eq!(
        &text[only.match_start as usize..only.match_end as usize],
        "use_module('../shared/graph', [reachable/2, walk//1])"
    );
}

/// The markdown block grammar closes a heading on the newline: a pattern
/// without one parses to an `ERROR` node, with one to `atx_heading`.
#[test]
fn markdown_ast_pattern_matches_a_heading() {
    let content = read(MARKDOWN_SAMPLE);
    let facts = query_patterns(
        MARKDOWN_SAMPLE,
        &content,
        &[query("heading", "# $TEXT\n", None, &["TEXT"])],
    )
    .expect("markdown pattern query");
    assert_eq!(captured(&facts, "TEXT"), vec!["title"]);
    let text = String::from_utf8(content).expect("utf8 fixture");
    let only = facts.first().expect("one match");
    assert_eq!(&text[only.start as usize..only.end as usize], "title");
    assert_eq!(
        &text[only.match_start as usize..only.match_end as usize],
        "# title\n"
    );
}

#[test]
fn a_yaml_ast_rule_runs_on_a_prolog_file() {
    let yaml = "id: use_module\nrule:\n  pattern: use_module($MODULE)\n";
    let request = decode_ast_rule_yaml(yaml).expect("yaml decodes");
    let content = read(PROLOG_SAMPLE);
    let matches = query_ast_rule(PROLOG_SAMPLE, &content, &request).expect("prolog yaml rule");
    assert_eq!(matches.len(), 1);
    assert_eq!(
        matches[0]
            .captures
            .iter()
            .map(|capture| capture.text.as_str())
            .collect::<Vec<_>>(),
        vec!["library(lists)"]
    );
}

#[test]
fn an_unknown_extension_still_reports_no_grammar() {
    let request = decode_ast_rule_yaml("id: x\nrule:\n  pattern: foo($BAR)\n").expect("yaml");
    assert_eq!(
        query_ast_rule("notes.unknownext", b"foo(bar)", &request),
        Err(AstRuleError::NoGrammar("notes.unknownext".into()))
    );
}

#[test]
fn the_cli_ast_pattern_door_reaches_prolog_and_markdown() {
    for (path, pattern, capture, expected) in [
        (
            PROLOG_SAMPLE,
            "rule=use_module($MODULE, $IMPORTS)",
            "rule=MODULE",
            "'../shared/graph'",
        ),
        (MARKDOWN_SAMPLE, "rule=# $TEXT\n", "rule=TEXT", "title"),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_extract"))
            .args(["--ast-pattern", pattern, "--ast-capture", capture, path])
            .output()
            .expect("extract binary runs");
        assert!(
            output.status.success(),
            "{path} stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
        assert!(
            stdout.contains(&format!("\"text\":\"{}\"", expected.replace('\\', "\\\\"))),
            "{path} stdout: {stdout}"
        );
    }
}
