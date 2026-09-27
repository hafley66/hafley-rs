#[path = "2_corpus.rs"]
pub mod corpus;
#[path = "0_query.rs"]
pub mod query;
#[path = "1_scope.rs"]
pub mod scope;

pub fn analyze(
    language: tree_sitter::Language,
    source: &str,
    query: &str,
    path: &std::path::Path,
) -> Result<scope::ScopeGraph, query::QueryFailure> {
    let matches = query::run_query(language, source, query, path)?;
    Ok(scope::resolve(
        matches
            .into_iter()
            .flat_map(|matched| matched.captures.into_iter()),
        source.len(),
    ))
}

pub fn sprefa_query<'a>(
    language: &str,
    query: &str,
    source: &'a [u8],
) -> Result<Vec<sprefa_extract::TreeSitterSpannedMatch>, Box<dyn std::error::Error>> {
    let request = sprefa_extract::TreeSitterQuery {
        language: language.into(),
        query: query.into(),
    };
    Ok(sprefa_extract::query_tree_sitter_spans(source, &request)?)
}
