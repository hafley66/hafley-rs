#[path = "0_call_query.rs"]
mod call_query;

#[path = "1_call_definition_rows.rs"]
mod call_definition_rows;

#[path = "2_call_metadata_rows.rs"]
mod call_metadata_rows;
#[path = "8_call_site_rows.rs"]
mod call_site_rows;
#[path = "5_const_string_rows.rs"]
mod const_string_rows;
#[path = "11_df_syntax_rows.rs"]
mod df_syntax_rows;
#[path = "14_expanded_call_rows.rs"]
mod expanded_call_rows;
#[cfg(feature = "rust")]
#[path = "23_frontend.rs"]
mod frontend;
#[path = "16_macro_invocation_rows.rs"]
mod macro_invocation_rows;
#[path = "10_module_resolution_rows.rs"]
mod module_resolution_rows;
#[path = "3_module_specifier_rows.rs"]
mod module_specifier_rows;
#[path = "12_receiver_rows.rs"]
mod receiver_rows;
mod syn_macro_expansion_defs;
#[path = "15_syntax.rs"]
mod syntax;
#[path = "19_tree_call_rows.rs"]
mod tree_call_rows;
#[path = "17_tree_entity_rows.rs"]
mod tree_entity_rows;
#[path = "21_tree_module_resolution_rows.rs"]
mod tree_module_resolution_rows;
#[path = "20_tree_module_specifier_rows.rs"]
mod tree_module_specifier_rows;
#[path = "22_tree_receiver_rows.rs"]
mod tree_receiver_rows;
#[path = "18_tree_type_candidate_rows.rs"]
mod tree_type_candidate_rows;
#[path = "13_tsi_syntax_rows.rs"]
mod tsi_syntax_rows;
#[path = "9_type_candidate_rows.rs"]
mod type_candidate_rows;
#[path = "7_type_entity_rows.rs"]
mod type_entity_rows;
#[path = "6_type_refs.rs"]
mod type_refs;

pub use call_definition_rows::{
    call_definition_rows, call_definition_rows_from_arena, CallDefinitionKind, CallDefinitionRow,
};
pub use call_metadata_rows::{
    build_line_starts, call_metadata_rows, cfg_test_predicate, item_attrs, line_col_to_byte,
    path_name, path_string, primary_type, variant_def_range, CallCfgRow, CallOwnerRow,
};
pub use call_query::RUST_CALL_QUERY;
pub const RUST_FAST_QUERY: &str = include_str!("4_fast_query.scm");
pub fn rust_combined_query() -> String {
    format!("{RUST_CALL_QUERY}\n{RUST_FAST_QUERY}")
}
pub use call_site_rows::{call_site_rows, CallSiteRow, CallSiteRows, ConstInitRow};
pub use const_string_rows::{const_string_rows, const_string_rows_from_tree, ConstStringRow};
pub use df_syntax_rows::{df_syntax_rows_from_tree, DfNodeKind as DfSyntaxKind, DfSyntaxRows};
pub use expanded_call_rows::{expanded_call_rows, ExpandedCallKind, ExpandedCallRows};
#[cfg(feature = "rust")]
pub use frontend::{call_query, fast_file_query, RustFastFile};
pub use macro_invocation_rows::{
    macro_invocation_rows, macro_invocation_rows_from_parsed, macro_invocation_rows_from_tree,
    MacroInvocationRow,
};
pub use module_resolution_rows::{
    module_resolution_rows, principal_ty, EnumVariantsRow, ImplMethodsRow, ModuleResolutionRows,
    StarImportRow, TraitMethodRow, TraitMethodsRow, UseBindingRow,
};
pub use module_specifier_rows::{
    mod_path_attr, module_specifier_rows, ModuleSpecifierKind, ModuleSpecifierRow,
};
pub use receiver_rows::{receiver_rows, ReceiverOutcome as RustReceiverOutcome};
pub use syn_macro_expansion_defs::{expand_file, Expanded};
pub use syntax::{parse_rust_syntax, RustSyntax};
pub use tree_call_rows::{call_metadata_rows_from_tree, call_site_rows_from_tree};
pub use tree_entity_rows::type_entity_rows_from_tree;
pub use tree_module_resolution_rows::module_resolution_rows_from_tree;
pub use tree_module_specifier_rows::module_specifier_rows_from_tree;
pub use tree_receiver_rows::receiver_rows_from_tree;
pub use tree_type_candidate_rows::type_candidate_rows_from_tree;
pub use tsi_syntax_rows::{tsi_syntax_rows_from_tree, Arg as TsiSyntaxArg, TsiSyntaxRows};
pub use type_candidate_rows::{
    bare_self_head, type_candidate_rows, TypeCandidateGroup, TypeCandidateKind, TypeCandidateOwner,
    TypeCandidateRow,
};
pub use type_entity_rows::{
    strip_type, type_entity_rows, DocRow, DocSectionRow, ImplSelfHeadRow, SignatureRef,
    SignatureSlot, TypeEntityKind, TypeEntityRow, TypeEntityRows,
};
pub use type_refs::{collect_path_args, type_refs};
