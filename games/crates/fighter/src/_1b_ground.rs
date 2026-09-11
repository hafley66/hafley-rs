//! Ground event and fact aliases for the shared locomotion statechart.
//!
//! Transition ownership lives in [`crate::_1a_chart`]. These names remain
//! direct inputs for callers that classify facts by grounded callback.

pub type Facts = crate::_1a_chart::GroundFacts;
pub type Event = crate::_1a_chart::Event;
