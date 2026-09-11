//! Public execution seam for source-backed animation completion cases.
//!
//! The case artifact supplies the pinned source identity and the caller-resolved
//! completion facts. This function delegates to the same ground decision used
//! by the live fighter controller.

use crate::Phase;

/// Execute one animation completion decision through the live ground chart.
pub fn animation_completion(phase: Phase, finished: bool, forward: bool) -> Option<Phase> {
    crate::_1b_ground::decide(
        phase,
        crate::_1b_ground::Event::Motion(crate::_1b_ground::Facts {
            finished,
            forward,
            ..crate::_1b_ground::Facts::default()
        }),
    )
}
