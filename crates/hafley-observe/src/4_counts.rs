use std::collections::BTreeMap;

use tracing_capture::{CaptureLayer, SharedStorage};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SpanCounts {
    pub instances: BTreeMap<String, usize>,
    pub entries: BTreeMap<String, usize>,
    pub fanout: BTreeMap<(String, String), usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Growth {
    Constant,
    Linear,
    Quadratic,
}

pub struct CountRecorder {
    storage: SharedStorage,
}

impl CountRecorder {
    pub fn new() -> (Self, CaptureLayer<tracing_subscriber::Registry>) {
        let storage = SharedStorage::default();
        let layer = CaptureLayer::new(&storage);
        (CountRecorder { storage }, layer)
    }

    pub fn counts(&self) -> SpanCounts {
        let storage = self.storage.lock();
        let mut counts = SpanCounts::default();
        for span in storage.all_spans() {
            let name = span.metadata().name().to_string();
            *counts.instances.entry(name.clone()).or_default() += 1;
            *counts.entries.entry(name.clone()).or_default() += span.stats().entered;
            if let Some(parent) = span.parent() {
                let edge = (parent.metadata().name().to_string(), name);
                *counts.fanout.entry(edge).or_default() += 1;
            }
        }
        counts
    }
}

impl SpanCounts {
    pub fn instances_of(&self, name: &str) -> usize {
        self.instances.get(name).copied().unwrap_or_default()
    }

    pub fn entries_of(&self, name: &str) -> usize {
        self.entries.get(name).copied().unwrap_or_default()
    }

    pub fn children_of(&self, parent: &str, child: &str) -> usize {
        self.fanout
            .get(&(parent.to_string(), child.to_string()))
            .copied()
            .unwrap_or_default()
    }

    pub fn assert_instances(&self, name: &str, expected: usize) {
        let actual = self.instances_of(name);
        assert_eq!(actual, expected, "span {name} instance count");
    }

    pub fn assert_children_at_most(&self, parent: &str, child: &str, ceiling: usize) {
        let actual = self.children_of(parent, child);
        assert!(
            actual <= ceiling,
            "fanout {parent} -> {child} was {actual}, ceiling {ceiling}"
        );
    }
}

/// The observed class of `name` between two runs whose input sizes differ by
/// `size_ratio`. Counts are deterministic, so no tolerance band is needed
/// beyond the midpoints that separate the three classes.
pub fn observed_growth(
    small: &SpanCounts,
    large: &SpanCounts,
    name: &str,
    size_ratio: f64,
) -> Growth {
    let before = small.entries_of(name) as f64;
    let after = large.entries_of(name) as f64;
    if before <= 0.0 {
        return Growth::Constant;
    }
    let observed = after / before;
    let linear_floor = (1.0 + size_ratio) / 2.0;
    let quadratic_floor = (size_ratio + size_ratio * size_ratio) / 2.0;
    if observed >= quadratic_floor {
        Growth::Quadratic
    } else if observed >= linear_floor {
        Growth::Linear
    } else {
        Growth::Constant
    }
}

pub fn assert_growth(
    small: &SpanCounts,
    large: &SpanCounts,
    name: &str,
    size_ratio: f64,
    expected: Growth,
) {
    let actual = observed_growth(small, large, name, size_ratio);
    assert_eq!(
        actual, expected,
        "span {name} grew {:?} from {} to {} entries across a {size_ratio}x input",
        actual,
        small.entries_of(name),
        large.entries_of(name)
    );
}
