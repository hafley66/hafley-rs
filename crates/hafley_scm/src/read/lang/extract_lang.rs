//! `RyiLang`: the one language roster the extractor speaks. Every variant
//! names a grammar this crate links directly (`tree_sitter_language`); none is
//! delegated to a third-party language registry. `from_path` routes through the
//! `Source` roster, the rest are plain lookups.
//! @comment-ok: module header, the shape every lang/*.rs opens with

use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use std::borrow::Cow;

/// `MarkdownInline` is never routed from a path (a `.md` routes to the block
/// grammar); a caller names it directly to reach the inline plane.
/// `Gdscript`/`Commonlisp`/`Typespec` are the syntax-only front-ends: a `.gd`/`.lisp`/`.tsp`
/// routes to the `Source` that owns the parse, and the grammar table here
/// names their linked crates.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum RyiLang {
    Rust,
    TypeScript,
    Tsx,
    JavaScript,
    Go,
    Kotlin,
    Python,
    Prolog,
    Markdown,
    MarkdownInline,
    Gdscript,
    Commonlisp,
    Typespec,
    Html,
    /// The data family's two cst-delegated grammars: `.json`/`.yaml` carry
    /// ast-era cst rows through the data Source, so their grammars stay on
    /// this roster even though no Source row is named after them.
    Json,
    Yaml,
}

impl RyiLang {
    /// Routed through the `Source` roster: each `Source` answers
    /// `extract_lang(path)`; the linked-grammar shim is the roster's default. No
    /// path-suffix switch lives here.
    pub fn from_path(path: &str) -> Option<Self> {
        crate::read::lang::source_for(path).and_then(|source| source.extract_lang(path))
    }

    /// The YAML `language:` field's spelling; inverse of `parse_name`.
    pub fn name(&self) -> Cow<'static, str> {
        Cow::Borrowed(match self {
            Self::Rust => "rust",
            Self::TypeScript => "typescript",
            Self::Tsx => "tsx",
            Self::JavaScript => "javascript",
            Self::Go => "go",
            Self::Kotlin => "kotlin",
            Self::Python => "python",
            Self::Prolog => "prolog",
            Self::Markdown => "markdown",
            Self::MarkdownInline => "markdown_inline",
            Self::Gdscript => "gdscript",
            Self::Commonlisp => "commonlisp",
            Self::Typespec => "typespec",
            Self::Html => "html",
            Self::Json => "json",
            Self::Yaml => "yaml",
        })
    }

    /// Case-sensitive over this crate's alias table; the `--lang` flag and the
    /// YAML `language:` field both arrive here.
    pub fn parse_name(name: &str) -> Option<Self> {
        match name {
            #[cfg(feature = "rust")]
            "rust" | "rs" => Some(Self::Rust),
            #[cfg(feature = "typescript")]
            "typescript" | "ts" => Some(Self::TypeScript),
            #[cfg(feature = "typescript")]
            "tsx" => Some(Self::Tsx),
            #[cfg(feature = "typescript")]
            "javascript" | "js" => Some(Self::JavaScript),
            #[cfg(feature = "go")]
            "go" | "golang" => Some(Self::Go),
            #[cfg(feature = "kotlin")]
            "kotlin" | "kt" => Some(Self::Kotlin),
            #[cfg(feature = "python")]
            "python" | "py" => Some(Self::Python),
            #[cfg(feature = "prolog")]
            "prolog" => Some(Self::Prolog),
            #[cfg(feature = "markdown")]
            "markdown" | "md" => Some(Self::Markdown),
            #[cfg(feature = "markdown")]
            "markdown_inline" | "md_inline" => Some(Self::MarkdownInline),
            #[cfg(feature = "gdscript")]
            "gdscript" | "gd" => Some(Self::Gdscript),
            #[cfg(feature = "commonlisp")]
            "commonlisp" | "lisp" | "cl" => Some(Self::Commonlisp),
            #[cfg(feature = "typespec")]
            "typespec" | "tsp" => Some(Self::Typespec),
            #[cfg(feature = "fallback")]
            "html" | "htm" => Some(Self::Html),
            #[cfg(feature = "data")]
            "json" => Some(Self::Json),
            #[cfg(feature = "data")]
            "yaml" | "yml" => Some(Self::Yaml),
            _ => None,
        }
    }

    /// The linked grammar a tree-sitter `Parser` takes, one row per variant.
    /// The same `LANGUAGE` constants the raw extractors parse with
    /// (prolog/_0_source.rs:25, markdown/_0_source.rs:86).
    pub fn tree_sitter_language(&self) -> tree_sitter::Language {
        match self {
            #[cfg(feature = "rust")]
            Self::Rust => tree_sitter::Language::new(tree_sitter_rust::LANGUAGE),
            #[cfg(feature = "typescript")]
            Self::TypeScript => {
                tree_sitter::Language::new(tree_sitter_typescript::LANGUAGE_TYPESCRIPT)
            }
            #[cfg(feature = "typescript")]
            Self::Tsx => tree_sitter::Language::new(tree_sitter_typescript::LANGUAGE_TSX),
            #[cfg(feature = "typescript")]
            Self::JavaScript => tree_sitter::Language::new(tree_sitter_javascript::LANGUAGE),
            #[cfg(feature = "go")]
            Self::Go => tree_sitter::Language::new(tree_sitter_go::LANGUAGE),
            #[cfg(feature = "kotlin")]
            Self::Kotlin => tree_sitter::Language::new(tree_sitter_kotlin_sg::LANGUAGE),
            #[cfg(feature = "python")]
            Self::Python => tree_sitter::Language::new(tree_sitter_python::LANGUAGE),
            #[cfg(feature = "prolog")]
            Self::Prolog => tree_sitter::Language::new(tree_sitter_prolog::LANGUAGE),
            #[cfg(feature = "markdown")]
            Self::Markdown => tree_sitter::Language::new(tree_sitter_md::LANGUAGE),
            #[cfg(feature = "markdown")]
            Self::MarkdownInline => tree_sitter::Language::new(tree_sitter_md::INLINE_LANGUAGE),
            #[cfg(feature = "gdscript")]
            Self::Gdscript => tree_sitter::Language::new(tree_sitter_gdscript::LANGUAGE),
            #[cfg(feature = "commonlisp")]
            Self::Commonlisp => {
                tree_sitter::Language::new(tree_sitter_commonlisp::LANGUAGE_COMMONLISP)
            }
            #[cfg(feature = "typespec")]
            Self::Typespec => tree_sitter::Language::new(tree_sitter_typespec::LANGUAGE),
            #[cfg(feature = "fallback")]
            Self::Html => tree_sitter::Language::new(tree_sitter_html::LANGUAGE),
            #[cfg(feature = "data")]
            Self::Json => tree_sitter::Language::new(tree_sitter_json::LANGUAGE),
            #[cfg(feature = "data")]
            Self::Yaml => tree_sitter::Language::new(tree_sitter_yaml::LANGUAGE),
            #[cfg(not(all(
                feature = "rust",
                feature = "typescript",
                feature = "go",
                feature = "kotlin",
                feature = "python",
                feature = "prolog",
                feature = "markdown",
                feature = "data",
                feature = "fallback",
                feature = "gdscript",
                feature = "commonlisp",
                feature = "typespec",
            )))]
            _ => panic!("grammar feature for {} is disabled", self.name()),
        }
    }

    /// The kind id a grammar spells `kind` with, or `0` — the absent mark. Lets
    /// a caller resolve its kind tables once per file, never per node.
    pub fn kind_to_id(&self, kind: &str) -> u16 {
        self.tree_sitter_language().id_for_node_kind(kind, true)
    }

    /// The grammar's field id for `field`, or None when the kind has no such
    /// field.
    pub fn field_to_id(&self, field: &str) -> Option<u16> {
        self.tree_sitter_language()
            .field_id_for_name(field)
            .map(|id| id.get())
    }
}

impl std::fmt::Display for RyiLang {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.name())
    }
}

impl Serialize for RyiLang {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.name())
    }
}

impl<'de> Deserialize<'de> for RyiLang {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let name = String::deserialize(deserializer)?;
        RyiLang::parse_name(&name)
            .ok_or_else(|| de::Error::invalid_value(de::Unexpected::Str(&name), &"a known grammar"))
    }
}
