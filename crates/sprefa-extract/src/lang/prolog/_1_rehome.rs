//! `impl Rehome for PrologSource`: DISABLED.
//!
//! The load-directive specifier matcher rode a YAML rule engine
//! (`rules/move_specifier.yml` + the dl6 `move_candidate` gate). That engine
//! is gone, so rehome answers empty: no import refs, no respells. A prolog
//! move plans without a specifier leg instead of matching through a removed
//! front-end.
//! @comment-ok: module header, the seam list every lang file opens with

use super::PrologSource;
use crate::move_cx::MoveCx;
use crate::edit_seams::ImportRef;
use crate::edit_seams::Respell;
use crate::edit_seams::Rehome;

impl Rehome for PrologSource {
    fn import_refs(&self, _cx: &MoveCx) -> Vec<ImportRef> {
        Vec::new()
    }

    fn respell(&self, _cx: &MoveCx, _reference: &ImportRef) -> Option<Respell> {
        None
    }
}
