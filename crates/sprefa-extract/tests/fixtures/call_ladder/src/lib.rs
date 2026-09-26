pub mod _0_types;
pub mod _1_none;
pub mod _2_one;
pub mod _3_many;
pub mod _4_nested;
pub mod _5_scope;
pub mod _6_local;
pub mod _7_local;
pub mod _8_trait;
pub mod _10_self_constructor;
pub mod deep;

pub fn exit() {}

#[path = "deep/_11_path_module.rs"]
pub mod relocated;
