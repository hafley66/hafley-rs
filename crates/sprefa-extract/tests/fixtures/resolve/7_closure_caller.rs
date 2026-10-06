pub fn run(values: &[u32]) -> Vec<u32> {
    values.iter().map(|value| helper(*value)).collect()
}

#[path = "8_closure_callee.rs"]
mod callee;
use callee::helper;
