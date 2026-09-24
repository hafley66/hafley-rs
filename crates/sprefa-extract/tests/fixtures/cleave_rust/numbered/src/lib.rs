use std::fmt::Debug;
use type_facts::helper;
use call_facts::project_call;

#[path = "1_type.rs"]
mod type_facts;
#[path = "2_call.rs"]
mod call_facts;

fn target() -> usize {
    project_call() + helper("value")
}

pub fn current() -> usize {
    target()
}
