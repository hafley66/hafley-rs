//! Every file under tests/ is a module of this one target. Selecting one
//! file's tests is `--test main -- <module>`, and `autotests = false` in
//! Cargo.toml is what stops cargo minting a second target per file. A file
//! that owns process-wide state keeps its own [[test]] entry instead of
//! joining this one.

#[path = "0_source_tree.rs"]
mod t0_source_tree;
#[path = "10_edit_producers.rs"]
mod t10_edit_producers;
#[path = "11_mutation_planner.rs"]
mod t11_mutation_planner;
#[path = "12_producer_planner.rs"]
mod t12_producer_planner;
#[path = "13_stage_store.rs"]
mod t13_stage_store;
#[path = "14_commit_engine.rs"]
mod t14_commit_engine;
#[path = "15_source_mutations.rs"]
mod t15_source_mutations;
#[path = "16_multi_repo_refresh.rs"]
mod t16_multi_repo_refresh;
#[path = "1_correctness.rs"]
mod t1_correctness;
#[path = "2_identities.rs"]
mod t2_identities;
#[path = "3_refs.rs"]
mod t3_refs;
#[path = "4_revision_graph.rs"]
mod t4_revision_graph;
#[path = "5_spans.rs"]
mod t5_spans;
#[path = "7_tracked_state.rs"]
mod t7_tracked_state;
#[path = "9_source_actions.rs"]
mod t9_source_actions;
