//! Air event and fact aliases for the shared locomotion statechart.
//!
//! Transition ownership lives in [`crate::_1a_chart`]. These names remain
//! direct inputs for callers that classify facts by airborne callback.

pub type AirFacts = crate::_1a_chart::AirFacts;
pub type AirEvent = crate::_1a_chart::Event;
