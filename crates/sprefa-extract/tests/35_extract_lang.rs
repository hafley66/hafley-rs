//! RyiLang routing, naming, and metavariable-sigil behavior — the language
//! layer the parser seams share.
use ast_grep_core::Language as _;
use ast_grep_language::{LanguageExt as _, SupportLang};
use sprefa_extract::RyiLang;

#[test]
fn from_path_routes_the_extract_grammars_and_delegates_the_rest() {
    assert_eq!(RyiLang::from_path("go.pl"), Some(RyiLang::Prolog));
    assert_eq!(RyiLang::from_path("t.plt"), Some(RyiLang::Prolog));
    assert_eq!(RyiLang::from_path("r.horn"), Some(RyiLang::Prolog));
    assert_eq!(RyiLang::from_path("a.md"), Some(RyiLang::Markdown));
    assert_eq!(RyiLang::from_path("a.markdown"), Some(RyiLang::Markdown));
    assert_eq!(RyiLang::from_path("p.gd"), Some(RyiLang::Gdscript));
    for lisp in ["l.lisp", "l.lsp", "l.cl", "l.asd"] {
        assert_eq!(RyiLang::from_path(lisp), Some(RyiLang::Commonlisp), "{lisp}");
    }
    for (path, sg) in [
        ("a.rs", SupportLang::Rust),
        ("a.ts", SupportLang::TypeScript),
        ("a.tsx", SupportLang::Tsx),
        ("a.js", SupportLang::JavaScript),
        ("a.go", SupportLang::Go),
        ("a.kt", SupportLang::Kotlin),
    ] {
        assert_eq!(RyiLang::from_path(path), Some(RyiLang::Sg(sg)));
    }
    assert_eq!(RyiLang::from_path("README"), None);
    assert_eq!(RyiLang::from_path("a.unknownext"), None);
}

#[test]
fn every_lang_name_round_trips_through_the_yaml_spelling() {
    let mut langs = vec![
        RyiLang::Prolog,
        RyiLang::Markdown,
        RyiLang::MarkdownInline,
        RyiLang::Gdscript,
        RyiLang::Commonlisp,
    ];
    langs.extend(
        SupportLang::all_langs()
            .iter()
            .copied()
            .map(RyiLang::Sg),
    );
    for lang in langs {
        assert_eq!(RyiLang::parse_name(&lang.name()), Some(lang));
    }
    assert_eq!(RyiLang::parse_name("not-a-grammar"), None);
}

/// `µ` is what ast-grep-language picks for every grammar whose identifiers take
/// Unicode letters (lib.rs:196-211). prolog is not such a grammar: `variable` is
/// `[A-Z]...` and `identifier` is `_*[a-z]...`, so `µT` parses to
/// `(ERROR (UNEXPECTED 181))` under it and `_T` is a plain variable. `_` is the
/// C/C++/CSS choice (ast-grep-language lib.rs:186-190).
/// @comment-ok: fail-first receipt, the sigil is why the two parse at all
#[test]
fn expando_char_is_underscore_for_prolog_and_mu_for_markdown() {
    assert_eq!(RyiLang::Prolog.expando_char(), '_');
    assert_eq!(RyiLang::Gdscript.expando_char(), '_');
    assert_eq!(RyiLang::Commonlisp.expando_char(), '_');
    assert_eq!(RyiLang::Markdown.expando_char(), 'µ');
    assert_eq!(RyiLang::MarkdownInline.expando_char(), 'µ');
    assert_eq!(
        RyiLang::Sg(SupportLang::Rust).expando_char(),
        SupportLang::Rust.expando_char()
    );
    assert_eq!(
        RyiLang::Sg(SupportLang::C).expando_char(),
        SupportLang::C.expando_char()
    );
    for lang in [
        RyiLang::Prolog,
        RyiLang::Markdown,
        RyiLang::Gdscript,
        RyiLang::Commonlisp,
    ] {
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
        let ours = RyiLang::Sg(SupportLang::Rust).pre_process_pattern(query);
        assert_eq!(ours, SupportLang::Rust.pre_process_pattern(query), "{query}");
    }
    assert_eq!(RyiLang::Markdown.pre_process_pattern("# $T"), "# µT");
}
