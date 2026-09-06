//! UI layer: the egui debug panel ([`debug`]), the XP-themed menu nav system ([`menu`]), and the
//! [`themes`] both wear. Sub-pages (items, characters, ...) live as folders under `menu` so they can
//! grow their own stems without churning this level. [`scale`] holds the pure canvas_items
//! counter-scale math `debug`'s `process()` drives every frame.

pub mod debug;
pub mod menu;
pub mod scale;
pub mod themes;
