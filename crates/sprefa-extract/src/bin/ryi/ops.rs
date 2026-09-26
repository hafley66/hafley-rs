use crate::models::call_edge::CallEdge;
use crate::models::edit_plan::EditPlan;
use crate::models::fact_summary::FactSummary;
use crate::models::type_edge::TypeEdge;
use crate::ops_auto::CleaveArgs;
use crate::ops_auto::DiffArgs;
use crate::ops_auto::FastArgs;
use crate::ops_auto::GraphArgs;
use crate::ops_auto::IngestArgs;
use crate::ops_auto::MoveArgs;
use crate::ops_auto::OpResult;
use crate::ops_auto::QueryArgs;
use crate::ops_auto::RegionArgs;
use crate::ops_auto::RenameArgs;
use crate::ops_auto::SchemaArgs;
use crate::ops_auto::ScipArgs;
use crate::ops_auto::SlowArgs;
use crate::ops_auto::TrailArgs;
use crate::ops_auto::WatchArgs;

pub fn fast(args: &FastArgs) -> impl Iterator<Item = OpResult<TypeEdge>> + '_ {
  let _ = args;
  std::iter::from_fn(|| todo!())
}

pub fn slow(args: &SlowArgs) -> OpResult<FactSummary> {
  let _ = args;
  todo!()
}

pub fn scip(args: &ScipArgs) -> OpResult<FactSummary> {
  let _ = args;
  todo!()
}

pub fn graph(args: &GraphArgs) -> OpResult<Vec<CallEdge>> {
  let _ = args;
  todo!()
}

pub fn cleave(args: &CleaveArgs) -> OpResult<EditPlan> {
  let _ = args;
  todo!()
}

pub fn r#move(args: &MoveArgs) -> OpResult<EditPlan> {
  let _ = args;
  todo!()
}

pub fn rename(args: &RenameArgs) -> OpResult<EditPlan> {
  let _ = args;
  todo!()
}

pub fn query(args: &QueryArgs) -> OpResult<FactSummary> {
  let _ = args;
  todo!()
}

pub fn region(args: &RegionArgs) -> OpResult<EditPlan> {
  let _ = args;
  todo!()
}

pub fn watch(args: &WatchArgs) -> OpResult<FactSummary> {
  let _ = args;
  todo!()
}

pub fn diff(args: &DiffArgs) -> OpResult<FactSummary> {
  let _ = args;
  todo!()
}

pub fn ingest(args: &IngestArgs, input: impl Iterator<Item = OpResult<TypeEdge>>) -> OpResult<FactSummary> {
  let _ = args;
  let _ = input;
  todo!()
}

pub fn schema(args: &SchemaArgs) -> OpResult<FactSummary> {
  let _ = args;
  todo!()
}

pub fn trail(args: &TrailArgs) -> OpResult<FactSummary> {
  let _ = args;
  todo!()
}
