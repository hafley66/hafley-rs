use std::path::Path;

pub(super) fn stamp(root: &Path) -> String {
    boop_store::testing::boop_test_env(root)
        .into_iter()
        .map(|(key, value)| format!("{key}={}", super::shell_quote(&value)))
        .collect::<Vec<_>>()
        .join(" ")
}
