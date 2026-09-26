use std::path::PathBuf;

pub struct Twice;
pub mod again;
pub mod third;

pub fn zero(_b: OnlyB) {}

pub fn one(_a: OnlyA) {}

pub fn hidden_twin(_s: Shared) {}

pub fn same_file_wins(_t: Twice) {}

pub fn two_visible(_p: Pair) {}

pub fn external(_p: PathBuf) {}

pub fn decoy(_d: DecoyOnly) {}

pub fn calls() {
    only_a_fn();
    shared_fn();
    only_b_fn();
}
