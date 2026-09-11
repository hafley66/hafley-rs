//! Public execution seam for source-backed animation completion cases.
//!
//! The case artifact supplies the pinned source identity and the caller-resolved
//! completion facts. This function delegates to the same ground decision used
//! by the live fighter controller.

use crate::{Phase, ground};

/// Execute one animation completion decision through the live ground chart.
pub fn animation_completion(phase: Phase, finished: bool, forward: bool) -> Option<Phase> {
    ground::decide(
        phase,
        ground::Event::Motion(ground::Facts {
            finished,
            forward,
            ..ground::Facts::default()
        }),
    )
}
