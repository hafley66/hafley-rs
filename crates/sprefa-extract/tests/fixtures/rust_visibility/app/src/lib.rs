#[path = "0_a.rs"] pub mod a;
#[path = "1_c.rs"] pub mod c;

use dev_dep::ExternalTrait;

pub struct Local;
pub trait LocalTrait { type Item; }
impl LocalTrait for Local { type Item = Local; }

pub type LocalSlot = <Local as LocalTrait>::Item;
pub type ForeignSlot = <Local as ExternalTrait>::Item;

pub fn library_target_probe() {
    bin_helper();
    nested_helper();
    normal_call();
    dev_call();
    build_call();
}

pub fn normal_type_probe(_n: normal_dep::NormalType) {}
pub fn dev_type_probe(_d: dev_dep::DevType) {}
pub fn build_type_probe(_b: build_dep::BuildType) {}

pub fn wrong_branch(_w: crate::a::b::Widget) {}
pub fn missing_branch(_w: crate::missing::Widget) {}
pub fn right_branch(_w: crate::c::b::Widget) {}

use normal_dep::normal_call;
