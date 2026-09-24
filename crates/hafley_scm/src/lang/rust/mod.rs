#[path = "0_call_query.rs"]
mod call_query;

#[path = "1_call_definition_rows.rs"]
mod call_definition_rows;

#[path = "2_call_metadata_rows.rs"]
mod call_metadata_rows;
#[path = "3_module_specifier_rows.rs"]
mod module_specifier_rows;
#[path = "5_const_string_rows.rs"]
mod const_string_rows;
#[path = "6_type_refs.rs"]
mod type_refs;
#[path = "7_type_entity_rows.rs"]
mod type_entity_rows;
#[path = "8_call_site_rows.rs"]
mod call_site_rows;
mod syn_macro_expansion_defs;

pub use call_definition_rows::{
    call_definition_rows, call_definition_rows_from_arena, CallDefinitionKind, CallDefinitionRow,
};
pub use call_metadata_rows::{
    build_line_starts, call_metadata_rows, cfg_test_predicate, item_attrs, line_col_to_byte,
    path_name, path_string, primary_type, variant_def_range, CallCfgRow, CallOwnerRow,
};
pub use call_query::RUST_CALL_QUERY;
pub const RUST_FAST_QUERY: &str = include_str!("4_fast_query.scm");
pub use module_specifier_rows::{
    mod_path_attr, module_specifier_rows, ModuleSpecifierKind, ModuleSpecifierRow,
};
pub use const_string_rows::{const_string_rows, ConstStringRow};
pub use type_refs::{collect_path_args, type_refs};
pub use type_entity_rows::{
    strip_type, type_entity_rows, DocRow, DocSectionRow, ImplSelfHeadRow, SignatureRef,
    SignatureSlot, TypeEntityKind, TypeEntityRow, TypeEntityRows,
};
pub use call_site_rows::{call_site_rows, CallSiteRow, CallSiteRows, ConstInitRow};
pub use syn_macro_expansion_defs::{expand_file, Expanded};
