#![cfg(feature = "rust_syn")]

use hafley_scm::lang::rust::{build_line_starts, module_resolution_rows};

#[test]
fn module_rows_keep_imports_declarations_and_definition_ranges() {
    let source = r#"
use crate::outer::{Thing, inner as renamed, self, *};
pub use super::Api;
mod inline { type Nested = u8; }
#[path = "elsewhere.rs"] mod external;
enum Choice { A, B(u8) }
trait Work { fn required(&self); fn defaulted(&self) {} }
type Alias = Choice;
impl Work for Choice { fn required(&self) {} }
"#;
    let parsed = syn::parse_file(source).expect("Rust parses");
    let rows = module_resolution_rows(&parsed, &build_line_starts(source));

    let imports: Vec<_> = rows.uses.iter().map(|row| (
        row.local.as_str(), row.qualifier.join("::"), row.asked.as_str(), row.reexport,
    )).collect();
    assert_eq!(imports, [
        ("Thing", "crate::outer".into(), "Thing", false),
        ("renamed", "crate::outer".into(), "inner", false),
        ("outer", "crate".into(), "outer", false),
        ("Api", "super".into(), "Api", true),
    ]);
    assert_eq!(rows.stars[0].qualifier, ["crate", "outer"]);
    assert_eq!(rows.inline_mods, ["inline"]);
    assert_eq!(rows.mod_decls, [("external".into(), Some("elsewhere.rs".into()))]);
    assert_eq!(rows.enums[0].name, "Choice");
    assert_eq!(rows.enums[0].variants.iter().map(|(name, _)| name.as_str()).collect::<Vec<_>>(), ["A", "B"]);
    assert_eq!(rows.traits[0].methods.iter().map(|row| (row.name.as_str(), row.default)).collect::<Vec<_>>(), [("required", false), ("defaulted", true)]);
    assert_eq!(&source[rows.aliases[1].start as usize..rows.aliases[1].end as usize], "Alias");
    let required = &rows.traits[0].methods[0].range;
    assert_eq!(&source[required.start as usize..required.end as usize], "required(&self)");
    assert_eq!(rows.impls[0].self_type, "Choice");
    assert_eq!(rows.impls[0].trait_name.as_deref(), Some("Work"));
    assert_eq!(rows.impls[0].methods[0].0, "required");
    let range = &rows.impls[0].methods[0].1;
    assert_eq!(&source[range.start as usize..range.end as usize], "required(&self) {}");
}
