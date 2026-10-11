#![cfg(feature = "cli")]
//! One spelling, one shape: a long flag means the same value (name, action,
//! arity) on every verb that takes it. The snapshot lists every spelling whose
//! shape differs between verbs; an empty snapshot is the contract.

use std::collections::BTreeMap;

#[test]
fn every_long_flag_has_one_shape_across_verbs() {
    let root = ryi_proto::help::help_command();
    let mut shapes: BTreeMap<String, BTreeMap<String, Vec<String>>> = BTreeMap::new();
    let verbs = std::iter::once(&root).chain(root.get_subcommands());
    for verb in verbs {
        for arg in verb.get_arguments() {
            let Some(long) = arg.get_long() else { continue };
            if matches!(long, "help" | "version") {
                continue;
            }
            let values = arg
                .get_value_names()
                .map(|names| names.join(" "))
                .unwrap_or_default();
            let shape = format!("{values} {:?} {:?}", arg.get_action(), arg.get_num_args());
            shapes
                .entry(long.to_owned())
                .or_default()
                .entry(shape)
                .or_default()
                .push(verb.get_name().to_owned());
        }
    }
    let drift: String = shapes
        .iter()
        .filter(|(_, by_shape)| by_shape.len() > 1)
        .map(|(long, by_shape)| {
            let shapes: Vec<String> = by_shape
                .iter()
                .map(|(shape, verbs)| format!("  {shape}: {}", verbs.join(",")))
                .collect();
            format!("--{long}\n{}\n", shapes.join("\n"))
        })
        .collect();
    insta::assert_snapshot!("flag_shapes", drift);
}
