# A2 ryi-only delta audit

Code tip `5811c274`; fixture tip `ba46b9b7`. The same 423/584 Rust sources were restored from each cached CodeQL `src.zip`; manifests came from `14342bf2`. Comparison uses the report's four-column key. S1 prelude/fixture scoping removed **10** sprefa type R rows (`886 -> 876`); lane Q's owner bucket was 362.

| New ryi-only bucket | Count | Verdict by source syntax |
| --- | ---: | --- |
| Root S7 type: CLI variant field 1; protobuf variant fields 11; Python bound variant 1; TSI variant fields 5; predicate variant fields 2 | 20 | 20 correct targets, 0 wrong |
| sprefa S8 type: edit seams 31; move context 26; rename context 20; stage 2 | 79 | 79 correct targets, 0 wrong |
| sprefa S8 call: stage 22; drain 20; edit roster 20; seams 12; move context 6; SCIP move 5; rename context 1 | 86 | 86 correct targets, 0 wrong |

Samples below are 20 distinct new rows. Paths are relative to the named corpus; `owner -> target` is the four-column key with paths abbreviated to their distinguishing suffixes. Every verdict is **correct**: the source has the shown field, type use, call, or constructor, and the target is the declared item.

| Corpus / kind | Owner -> target | Source syntax / verdict |
| --- | --- | --- |
| root type | `boop/src/main.rs:Selection -> cli/selection.rs:SelectionCmd` | variant field `cmd: cli::selection::SelectionCmd`, correct |
| root type | `read/cpg/cpg_proto.rs:BoolList -> cpg_proto.rs:BoolList` | generated variant field `BoolList(super::BoolList)`, correct |
| root type | `read/scip/scip_proto.rs:MultiLineRange -> scip_proto.rs:MultiLineRange` | generated variant field `MultiLineRange(super::MultiLineRange)`, correct |
| root type | `read/lang/python/_0_source.rs:Name -> atoms.rs:NameId` | variant field `Name(crate::read::shape::NameId)`, correct |
| root type | `read/types.rs:Fact -> read/tsi/types.rs:FactOut` | variant field `Fact(crate::read::tsi::types::FactOut)`, correct |
| root type | `types/predicate.rs:Node -> types/stop.rs:Stop` | variant field `stop: super::Stop`, correct |
| sprefa type | `edit/_5_move_text.rs:MoveCx -> edit/_1_move_cx.rs:MoveCx` | explicit `sprefa_extract::MoveCx` import, correct |
| sprefa type | `edit/_5_move_text.rs:RenameCx -> edit/_1_rename_cx.rs:RenameCx` | explicit `sprefa_extract::RenameCx` import, correct |
| sprefa type | `edit/_6_move.rs:respells -> edit/_0_seams.rs:Respell` | explicit `sprefa_extract::Respell` import, correct |
| sprefa type | `edit/_6_rename.rs:Plan -> edit/_0_seams.rs:SymbolRef` | exported edit-seam type, correct |
| sprefa type | `edit/_5_move_text.rs:report_rename -> edit/_1_rename_cx.rs:RenameRequest` | explicit import and parameter type, correct |
| sprefa type | `edit/_7_cleave.rs:Plan -> edit/_0_seams.rs:CleavePlan` | `sprefa_extract::edit_seams::CleavePlan`, correct |
| sprefa type | `edit/_6_move.rs:verify_after_commit -> edit/_3_stage.rs:VerifyJournal` | `sprefa_extract::move_stage::VerifyJournal`, correct |
| sprefa call | `edit/_5_move_text.rs:candidates -> edit.rs:rehome_for` | explicit import and call, correct |
| sprefa call | `edit/_6_move.rs:build_for -> edit/_2_drain.rs:directory_source` | exported drain function, correct |
| sprefa call | `edit/_6_move.rs:build_for -> edit/_3_stage.rs:content_id` | move-stage import and call, correct |
| sprefa call | `edit/_6_move.rs:absolute -> edit/_1_move_cx.rs:normalize` | explicit import and call, correct |
| sprefa call | `tests/5_move_scip.rs:a_dropped_ref_reads_as_missed_by_impl -> edit/_4_move_scip.rs:verify_import_refs` | exported SCIP move function, correct |
| sprefa call | `edit/_7_cleave.rs:caller_respells -> edit/_0_seams.rs:Respell` | struct construction, correct |
| sprefa call | `edit/_6_rename.rs:validated_batch -> edit/_1_rename_cx.rs:RenameRequest` | struct construction, correct |

The S8 private-target guard also removed 31 wrong type and 8 wrong call edges to the private `src/5_diff.rs::Span`; these are outside the new ryi-only sets above.
