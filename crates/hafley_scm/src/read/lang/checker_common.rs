use std::collections::HashMap;
use soopy::_0_types::ContentId;

/// The driver wrote each span's SUPPLIED path; a corpus path becomes the file's
/// content digest and any other path stays as it is, naming a file off-corpus.
pub fn stamp_digests(
    rows: Vec<crate::read::tsi::FactOut>,
    corpus: &[(String, ContentId)],
) -> Vec<crate::read::tsi::FactOut> {
    let digest_of: HashMap<&str, String> = corpus
        .iter()
        .map(|(path, blob)| (path.as_str(), blob.to_string()))
        .collect();
    rows.into_iter()
        .map(|mut row| {
            for arg in &mut row.args {
                if let crate::read::tsi::Arg::Span(key, _, _) = arg {
                    if let Some(digest) = digest_of.get(key.as_str()) {
                        *key = digest.clone();
                    }
                }
            }
            row
        })
        .collect()
}
