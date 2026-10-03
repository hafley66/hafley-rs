use crate::types::QueryError;

pub fn text(error: &variant_reexport::QueryError) -> String {
    match error {
        variant_reexport::QueryError::Parse(text) => text.clone(),
        variant_reexport::QueryError::Only { pattern, op } => format!("{pattern} {op}"),
    }
}

pub fn only(error: &QueryError) -> bool {
    match error {
        QueryError::Only { .. } => true,
        _ => false,
    }
}
