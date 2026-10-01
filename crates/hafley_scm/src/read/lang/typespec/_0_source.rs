//! TypeSpec (`.tsp`) extraction over the vendored tree-sitter-typespec grammar.
//! One plane, CstF, through the shared `cst_bundle` walk; no resolve/rehome/rename/cfg leg.

use crate::read::family::CstF;
use crate::read::lang::cst_bundle::cst_bundle;
use crate::read::lang::extract_lang::RyiLang;
use crate::read::rows::FamilyBundle;
use crate::read::source::{FamilyMask, RyiOutput, Source};
use crate::read::trace;

#[derive(Default)]
pub struct TypespecSource;

impl Source for TypespecSource {
    fn planes(&self) -> FamilyMask {
        FamilyMask {
            cst: true,
            types: false,
            call: false,
            df: false,
            data: false,
        }
    }

    fn name(&self) -> &'static str {
        "typespec"
    }

    fn matches(&self, path: &str) -> bool {
        path.ends_with(".tsp")
    }

    fn extract_lang(&self, _path: &str) -> Option<RyiLang> {
        Some(RyiLang::Typespec)
    }

    fn extract(&self, path: &str, content: &[u8], mask: FamilyMask) -> RyiOutput {
        let mut output = RyiOutput::default();
        if mask.cst {
            let span = trace::family_span("typespec", "cst");
            let _entered = span.enter();
            let bundle: Option<FamilyBundle<CstF>> = cst_bundle(path, content, &mut output.strings);
            if let Some(bundle) = &bundle {
                trace::record_bundle(&span, bundle, 0);
            }
            output.cst = bundle;
        }
        output
    }
}
