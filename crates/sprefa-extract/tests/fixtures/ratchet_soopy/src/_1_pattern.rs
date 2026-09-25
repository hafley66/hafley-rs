use anyhow::{Context, Result};
use globset::{Glob, GlobSet, GlobSetBuilder};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pattern(pub String);

pub(crate) fn compile(patterns: &[Pattern]) -> Result<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        builder
            .add(Glob::new(&pattern.0).with_context(|| format!("invalid glob {:?}", pattern.0))?);
    }
    builder.build().context("compile glob set")
}
