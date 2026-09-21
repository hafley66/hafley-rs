// Wire vocabularies from extract::types and extract::tsi::types.
namespace Extract;

enum FamilyTag {
  Df: "df", Flow: "flow", Call: "call", Type: "type",
  Module: "module", Cst: "cst", Cfg: "cfg", Data: "data",
}
enum Mode { Syntax: "syntax", Semantic: "semantic" }
enum CoverageFlag { Partial: "partial", Complete: "complete" }
enum Method {
  SameFile: "same_file", CorpusUnique: "corpus_unique", ModulePlane: "module_plane",
  Checker: "checker", AliasChain: "alias_chain", Param: "param", Receiver: "receiver",
  SelfType: "self_type", IfaceImpl: "iface_impl", Decorator: "decorator",
  Subscript: "subscript", ReturnCall: "return_call", Scip: "scip",
  Unresolved: "unresolved", Parse: "parse", CheckerWalk: "checker_walk", Foreign: "foreign",
}

model IdArg { id: uint32; }
model SpanArg { span: [string, uint32, uint32]; }
model TextArg { text: string; }
model IntArg { int: int64; }
model AtomArg { atom: string; }
union TsiArg { id: IdArg, span: SpanArg, text: TextArg, int: IntArg, atom: AtomArg }

// One `ryi rename` site the arm found and declined to plan; `reason` is an
// `UnresolvedReason::as_str` spelling.
model RenameAbstain { file: string; span: Span; symbol: string; reason: string; receiver: string; }

// One `ryi cleave` plan, the shape `--json` closes the run with. `kind` on a
// specifier is `package`, `relative`, or `carried` when DEST already has it.
model CleaveSpecifier { name: string; module: string; dest_module: string; span: Span; kind: string; }
model CleaveDrag { name: string; span: Span; iteration: uint32; }
model CleavePlan {
  src: string; dest: string; item: string; item_span: Span;
  travelling: CleaveSpecifier[]; orphans: CleaveSpecifier[];
  callers: string[]; dragged: CleaveDrag[];
  drag_iterations: uint32; unresolved: string[];
}
