use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct Capture {
    pub label: String,
    pub text: String,
    pub start: u32,
    pub end: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueryOutput {
    pub captures: Vec<Capture>,
    pub did_exceed_match_limit: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NodeKind {
    Root,
    Scope,
    Def,
    Ref,
    Push,
    Pop,
    Export,
    Import,
}

impl NodeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Root => "root",
            Self::Scope => "scope",
            Self::Def => "def",
            Self::Ref => "ref",
            Self::Push => "push",
            Self::Pop => "pop",
            Self::Export => "export",
            Self::Import => "import",
        }
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct NamedEdge {
    pub caller_path: String,
    pub caller_name: String,
    pub callee_path: String,
    pub callee_name: String,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct Unresolved {
    pub path: String,
    pub name: String,
    pub reason: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct Analysis {
    pub edges: Vec<NamedEdge>,
    pub unresolved: Vec<Unresolved>,
    pub rows: Vec<ScmRow>,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(tag = "record")]
pub enum ScmRow {
    #[serde(rename = "scip_def")]
    ScipDef {
        symbol: String,
        file: String,
        repo: String,
    },
    #[serde(rename = "scip_ref")]
    ScipRef {
        file: String,
        symbol: String,
        def_file: String,
        repo: String,
    },
    #[serde(rename = "scip_local")]
    ScipLocal {
        #[serde(rename = "fn")]
        enclosing_fn: String,
        name: String,
    },
}

#[derive(Debug)]
pub enum LabError {
    Io(String),
    Query(String),
    MatchLimitExceeded { language: String, path: String },
    Sql(String),
}

impl std::fmt::Display for LabError {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(text) | Self::Query(text) | Self::Sql(text) => out.write_str(text),
            Self::MatchLimitExceeded { language, path } => {
                write!(out, "MatchLimitExceeded: {language} query on {path}")
            }
        }
    }
}

impl std::error::Error for LabError {}
