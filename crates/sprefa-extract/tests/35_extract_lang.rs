//! `RyiLang` routing and naming — the language layer the parser seams share,
//! one linked grammar per variant.
use sprefa_extract::RyiLang;

#[test]
fn from_path_routes_the_roster_grammars_and_delegates_the_rest() {
    assert_eq!(RyiLang::from_path("go.pl"), Some(RyiLang::Prolog));
    assert_eq!(RyiLang::from_path("t.plt"), Some(RyiLang::Prolog));
    assert_eq!(RyiLang::from_path("r.horn"), Some(RyiLang::Prolog));
    assert_eq!(RyiLang::from_path("a.md"), Some(RyiLang::Markdown));
    assert_eq!(RyiLang::from_path("a.markdown"), Some(RyiLang::Markdown));
    assert_eq!(RyiLang::from_path("p.gd"), Some(RyiLang::Gdscript));
    for lisp in ["l.lisp", "l.lsp", "l.cl", "l.asd"] {
        assert_eq!(RyiLang::from_path(lisp), Some(RyiLang::Commonlisp), "{lisp}");
    }
    for (path, lang) in [
        ("a.rs", RyiLang::Rust),
        ("a.ts", RyiLang::TypeScript),
        ("a.tsx", RyiLang::Tsx),
        ("a.js", RyiLang::JavaScript),
        ("a.jsx", RyiLang::JavaScript),
        ("a.go", RyiLang::Go),
        ("a.kt", RyiLang::Kotlin),
        ("a.py", RyiLang::Python),
        ("a.html", RyiLang::Html),
    ] {
        assert_eq!(RyiLang::from_path(path), Some(lang), "{path}");
    }
    assert_eq!(RyiLang::from_path("README"), None);
    assert_eq!(RyiLang::from_path("a.unknownext"), None);
    // Coverage that died with the removed registry: no linked grammar, no lang.
    assert_eq!(RyiLang::from_path("a.css"), None);
    assert_eq!(RyiLang::from_path("a.java"), None);
}

#[test]
fn every_lang_name_round_trips_through_the_yaml_spelling() {
    let langs = [
        RyiLang::Rust,
        RyiLang::TypeScript,
        RyiLang::Tsx,
        RyiLang::JavaScript,
        RyiLang::Go,
        RyiLang::Kotlin,
        RyiLang::Python,
        RyiLang::Prolog,
        RyiLang::Markdown,
        RyiLang::MarkdownInline,
        RyiLang::Gdscript,
        RyiLang::Commonlisp,
        RyiLang::Html,
        RyiLang::Json,
        RyiLang::Yaml,
    ];
    for lang in langs {
        assert_eq!(RyiLang::parse_name(&lang.name()), Some(lang), "{}", lang.name());
    }
    assert_eq!(RyiLang::parse_name("not-a-grammar"), None);
}

#[test]
fn parse_name_answers_the_alias_table() {
    assert_eq!(RyiLang::parse_name("rs"), Some(RyiLang::Rust));
    assert_eq!(RyiLang::parse_name("ts"), Some(RyiLang::TypeScript));
    assert_eq!(RyiLang::parse_name("js"), Some(RyiLang::JavaScript));
    assert_eq!(RyiLang::parse_name("golang"), Some(RyiLang::Go));
    assert_eq!(RyiLang::parse_name("kt"), Some(RyiLang::Kotlin));
    assert_eq!(RyiLang::parse_name("py"), Some(RyiLang::Python));
    assert_eq!(RyiLang::parse_name("md"), Some(RyiLang::Markdown));
    assert_eq!(RyiLang::parse_name("md_inline"), Some(RyiLang::MarkdownInline));
    assert_eq!(RyiLang::parse_name("gd"), Some(RyiLang::Gdscript));
    for lisp in ["lisp", "cl"] {
        assert_eq!(RyiLang::parse_name(lisp), Some(RyiLang::Commonlisp), "{lisp}");
    }
    assert_eq!(RyiLang::parse_name("html"), Some(RyiLang::Html));
    assert_eq!(RyiLang::parse_name("htm"), Some(RyiLang::Html));
}

/// Every variant names a grammar this workspace links: the table loads for all
/// of them, and each grammar knows its own name spellings.
#[test]
fn every_variant_carries_a_linked_grammar() {
    for lang in [
        RyiLang::Rust,
        RyiLang::TypeScript,
        RyiLang::Tsx,
        RyiLang::JavaScript,
        RyiLang::Go,
        RyiLang::Kotlin,
        RyiLang::Python,
        RyiLang::Prolog,
        RyiLang::Markdown,
        RyiLang::MarkdownInline,
        RyiLang::Gdscript,
        RyiLang::Commonlisp,
        RyiLang::Html,
        RyiLang::Json,
        RyiLang::Yaml,
    ] {
        let ts = lang.tree_sitter_language();
        let (lo, hi) = (
            tree_sitter::MIN_COMPATIBLE_LANGUAGE_VERSION,
            tree_sitter::LANGUAGE_VERSION,
        );
        assert!(
            (lo..=hi).contains(&ts.version()),
            "{lang}: ABI {} outside {lo}..={hi}",
            ts.version()
        );
    }
    // A kind the grammar declares resolves; one it does not resolves to 0,
    // the absent mark.
    assert_ne!(RyiLang::Rust.kind_to_id("function_item"), 0);
    assert_eq!(RyiLang::Rust.kind_to_id("no_such_kind_anywhere"), 0);
    assert_ne!(RyiLang::Rust.field_to_id("name"), None);
    assert_eq!(RyiLang::Rust.field_to_id("no_such_field"), None);
}

#[test]
fn serde_round_trips_through_the_yaml_spelling() {
    let lang = RyiLang::MarkdownInline;
    let text = serde_json::to_string(&lang).expect("serializes");
    assert_eq!(text, "\"markdown_inline\"");
    let back: RyiLang = serde_json::from_str(&text).expect("deserializes");
    assert_eq!(back, lang);
    assert!(serde_json::from_str::<RyiLang>("\"nope\"").is_err());
}
