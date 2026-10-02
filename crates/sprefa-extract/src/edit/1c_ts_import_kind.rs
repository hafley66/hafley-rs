use crate::edit_seams::Edit;
use crate::lang::ts::TsSource;
use hafley_scm::span::Span;

pub(super) fn type_import(source: &TsSource, text: &str, module: &str, item: &str) -> Option<Edit> {
    let statements = super::imports(source, text);
    if statements
        .iter()
        .any(|row| row.module == module && row.names.iter().any(|name| name == item))
    {
        return None;
    }
    let quote = statements.first().map_or('"', |row| row.quote);
    let at = statements
        .iter()
        .map(|row| row.span.end())
        .max()
        .unwrap_or(0);
    Some(Edit {
        span: Span::anchor(at),
        text: format!("import type {{ {item} }} from {quote}{module}{quote}{}\n", if super::import_style(text).1 { ";" } else { "" }),
    })
}
