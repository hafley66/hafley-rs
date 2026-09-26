use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TypeEdgeKind {
  Field,
  Variant,
  Impl,
  Generic,
  Param,
  Returns,
  Uses,
  DocRef,
}
