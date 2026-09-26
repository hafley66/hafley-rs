use crate::_0_types::{free_zero, Device, Action};
use crate::_1_none::reexported;

pub fn free_call() { free_zero(); }
pub fn inherent_call(device: &Device) { device.ping(); }
pub fn trait_impl_call(device: &Device) { device.act(); }
pub fn trait_object_call(device: &dyn Action) { device.act(); }
pub fn generic_call<T: Action>(device: &T) { device.act(); }
pub fn ufcs_call(device: &Device) { <Device as Action>::act(device); }
pub fn closure_call() { let closure = || free_zero(); closure(); }
pub fn deref_call(device: &Box<Device>) { device.ping(); }
pub fn reexport_call() { reexported(); }

macro_rules! invoke { ($f:ident) => { $f() }; }
pub fn macro_call() { invoke!(free_zero); }
