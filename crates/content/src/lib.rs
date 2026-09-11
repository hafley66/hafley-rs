#[path = "0_types.rs"]
mod types;
pub use types::*;

#[cfg(feature = "ingest")]
#[path = "1_decode.rs"]
mod decode;
#[cfg(feature = "ingest")]
pub use decode::{decode_file, decode_html};

#[cfg(feature = "ingest")]
#[path = "2_bake.rs"]
mod bake;
#[cfg(feature = "ingest")]
pub use bake::bake;

#[cfg(feature = "ingest")]
#[path = "3_chart.rs"]
mod chart;
#[cfg(feature = "ingest")]
pub use chart::{emit_chart, TransitionSpec};

#[cfg(feature = "ingest")]
#[path = "4_attributes.rs"]
mod attributes;
#[cfg(feature = "ingest")]
pub use attributes::attributes;

#[cfg(feature = "ingest")]
#[path = "5_source.rs"]
mod source;
#[cfg(feature = "ingest")]
pub use source::{
    CallSite, FunctionEvidence, FunctionSite, Guard, Inventory, Op, PortBody, PortEffect, PortExpr,
    PortFile, PortFn, PortSpan, PortStatement, PortValue, RECOGNIZED_OPERATIONS, SourceError,
    SourceAssociation, SourceCallRequirement, SourceCallback, SourceFileRecord,
    SourceInventoryCounts, SourceMachineInventory,
    SourceRef, SourceRule, SourceState, Unresolved, common_inventory, conditional_choice,
    emit_port_rust, function_evidence, if_guard, lower_callback, lower_guard, motion_states,
    source_machine_inventory, source_machine_inventory_with_dispatch,
};

#[cfg(feature = "ingest")]
#[path = "6_catalog.rs"]
mod catalog;
#[cfg(feature = "ingest")]
pub use catalog::{
    Catalog, CatalogEntry, CatalogError, CatalogEvidence, CharacterSpec, SourceEntry,
    generate_catalog,
};

#[path = "7_roles.rs"]
mod roles;
pub use roles::{
    ActionRole, RoleBinding, RoleBindings, RoleError, RoleFallback, role_bindings_source,
};
#[cfg(feature = "ingest")]
pub use roles::generate_role_bindings;
