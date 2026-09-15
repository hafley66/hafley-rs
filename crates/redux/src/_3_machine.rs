//! Enum-state machine declaration: one authored row list that lowers to the
//! existing [`crate::Slice`] algebra and to graph metadata from the same rows.
//!
//! The macro is a thin lowering, not a runtime. It emits one [`crate::slice!`]
//! invocation for dispatch and two constants (`Node`/`Edge` slices) for the
//! declared transition rows. State is the caller's own enum, with no wrapper
//! phase field; events, context, effects, and the handled/unhandled output enum
//! are caller-owned types.
//!
//! # Authoring contract
//!
//! - Guard functions are `fn(&State, Context, &Event) -> bool`.
//! - Action functions are `fn(&mut State, Context, &Event, &mut impl FnMut(Effect))`.
//! - Guard and action names are single identifiers naming free functions; module
//!   paths are not accepted in this bounded grammar.
//! - The event is owned by `reduce` and borrowed for guards, actions, and target
//!   expressions, so a row can reborrow it repeatedly; it is never moved out.
//! - `[stay]` assigns no target and preserves the current payload (plus any
//!   action mutation); a complete target constructor such as
//!   `[Open { token: *id }]` must name every field.
//! - The event pattern is an `if let`, so its bindings scope through the action
//!   call and the target expression. Target expressions may read event bindings.
//! - The state pattern is evaluated inside `matches!`, so its bindings do NOT
//!   escape into the arm. Write `_` for payload fields unless a literal value
//!   gate is intended; a name there is not visible to the target expression.
//! - `Context` must satisfy the existing `Slice::Context<'a>: Copy` bound and is
//!   passed to guards and actions by value.
//! - Row order is priority. A row whose state pattern does not match, whose
//!   event pattern does not match, or whose guard is false falls through to the
//!   next row; if none match the machine returns the unhandled variant.
//!
//! # Out of scope
//!
//! No superstates or nested machines, no implicit clock or frame counter, no
//! hooks, and no statig dependency. State, event, and effect types stay
//! caller-owned; this macro only lowers the declared rows.
//!
//! The graph is adjacency metadata over the declared rows, not a reachability
//! or dynamic-guard evaluation claim. This module ships the metadata types and
//! the uniqueness/endpoint check; rendering (D2, SVG) lives with the consumer
//! that needs it.

/// One declared state node in a machine graph.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Node {
    pub id: &'static str,
}

/// One declared transition row. `target` is the source node id when the row is
/// an explicit `[stay]` transition.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Edge {
    pub id: &'static str,
    pub source: &'static str,
    pub event: &'static str,
    pub guard: Option<&'static str>,
    pub action: Option<&'static str>,
    pub target: &'static str,
}

/// A machine's node and edge slices, as emitted by the declaration macro.
#[derive(Copy, Clone, Debug)]
pub struct Graph {
    pub nodes: &'static [Node],
    pub edges: &'static [Edge],
}

/// Why a declared graph is malformed. Checked against declared labels only.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum GraphError {
    /// The node list is empty.
    NoNodes,
    /// Two nodes share an id.
    DuplicateNode(&'static str),
    /// Two edges share an id.
    DuplicateEdge(&'static str),
    /// An edge endpoint names no declared node.
    UnknownNode {
        edge: &'static str,
        node: &'static str,
    },
}

impl Graph {
    pub const fn new(nodes: &'static [Node], edges: &'static [Edge]) -> Self {
        Self { nodes, edges }
    }

    /// Check unique node ids, unique edge ids, and declared endpoints. Covers
    /// declared adjacency only; it makes no reachability claim.
    pub fn validate(&self) -> Result<(), GraphError> {
        if self.nodes.is_empty() {
            return Err(GraphError::NoNodes);
        }
        for (i, node) in self.nodes.iter().enumerate() {
            if self.nodes[..i].iter().any(|other| other.id == node.id) {
                return Err(GraphError::DuplicateNode(node.id));
            }
        }
        for (i, edge) in self.edges.iter().enumerate() {
            if self.edges[..i].iter().any(|other| other.id == edge.id) {
                return Err(GraphError::DuplicateEdge(edge.id));
            }
            for endpoint in [edge.source, edge.target] {
                if !self.nodes.iter().any(|node| node.id == endpoint) {
                    return Err(GraphError::UnknownNode {
                        edge: edge.id,
                        node: endpoint,
                    });
                }
            }
        }
        Ok(())
    }
}

/// Optional label string: absent expands to `None`, present to `Some(name)`.
#[doc(hidden)]
#[macro_export]
macro_rules! __machine_opt_name {
    () => {
        ::core::option::Option::None
    };
    ($name:path) => {
        ::core::option::Option::Some(::core::stringify!($name))
    };
}

/// Metadata target label: `[stay]` maps to the source node id.
#[doc(hidden)]
#[macro_export]
macro_rules! __machine_target_name {
    ($source:ident, stay) => {
        ::core::stringify!($source)
    };
    ($source:ident, $target:ident) => {
        ::core::stringify!($target)
    };
}

/// Build one `Edge` from a declared row.
#[doc(hidden)]
#[macro_export]
macro_rules! __machine_edge {
    ($id:ident, $source:ident, $event:ident, [$($guard:ident)?], [$($action:ident)?], $target:ident $($rest:tt)*) => {
        $crate::Edge {
            id: ::core::stringify!($id),
            source: ::core::stringify!($source),
            event: ::core::stringify!($event),
            guard: $crate::__machine_opt_name!($($guard)?),
            action: $crate::__machine_opt_name!($($action)?),
            target: $crate::__machine_target_name!($source, $target),
        }
    };
}

/// Target assignment: `[stay]` assigns nothing, a constructor assigns `*st`
/// through a caller-scoped alias (angle-bracket qualified struct literals are
/// not stable syntax).
#[doc(hidden)]
#[macro_export]
macro_rules! __machine_assign {
    ($st:ident, $alias:ident, stay) => {};
    ($st:ident, $alias:ident, $target:ident $($rest:tt)*) => {
        *$st = $alias::$target $($rest)*;
    };
}

/// Declare an enum-state machine.
///
/// The rows below build one small door machine and drive its generated
/// `Slice`: `Idle` accepts a payload event and constructs `Open` from the event
/// binding.
///
/// ```
/// use redux::{Slice, machine};
///
/// #[derive(Copy, Clone, Debug, PartialEq, Eq)]
/// enum State {
///     Idle,
///     Open { token: u32 },
/// }
///
/// #[derive(Copy, Clone, Debug, PartialEq, Eq)]
/// enum Event {
///     Go { token: u32 },
/// }
///
/// #[derive(Copy, Clone, Debug, PartialEq, Eq)]
/// enum Outcome {
///     Handled,
///     Unhandled,
/// }
///
/// #[derive(Copy, Clone)]
/// struct Cx;
///
/// machine! {
///     Door for State {
///         context: Cx,
///         event: Event,
///         effect: (),
///         output: Outcome { handled: Handled, unhandled: Unhandled },
///         reduce(st, ev, cx, fx)
///         states: [Idle, Open],
///         graph: { nodes: NODES, edges: EDGES },
///         ([Idle] [Go { token }] -> [Open { token: *token }], id: GO),
///     }
/// }
///
/// let mut state = State::Idle;
/// let outcome = Door::reduce(&mut state, Event::Go { token: 4 }, Cx, &mut |_| {});
/// assert_eq!(outcome, Outcome::Handled);
/// assert_eq!(state, State::Open { token: 4 });
/// ```
#[macro_export]
macro_rules! machine {
    (
        $(#[$meta:meta])*
        $vis:vis $name:ident for $state:ty {
            context: $cx:ty,
            event: $event:ty,
            effect: $effect:ty,
            output: $out:ty { handled: $handled:ident, unhandled: $unhandled:ident },
            reduce($st:ident, $ev:ident, $cx_id:ident, $fx:ident)
            states: [ $($node:ident),+ $(,)? ],
            graph: { nodes: $nodes:ident, edges: $edges:ident },
            $(
                (
                    [$src:ident $($src_rest:tt)*]
                    [$evt:ident $($evt_rest:tt)*]
                    $(guard: $g:ident)?
                    $(action: $a:ident)?
                    -> [$tgt:ident $($tgt_rest:tt)*]
                    , id: $id:ident
                )
            ),* $(,)?
        }
    ) => {
        $crate::slice! {
            $(#[$meta])*
            $vis $name for $state {
                context: $cx, event: $event, output: $out, effect: $effect,
                reduce($st, $ev, $cx_id, $fx) {
                    #[allow(non_camel_case_types)]
                    type __MachineState = $state;
                    #[allow(non_camel_case_types)]
                    type __MachineEvent = $event;
                    #[allow(non_camel_case_types)]
                    type __MachineOutcome = $out;
                    $(
                        if ::core::matches!(&*$st, __MachineState::$src $($src_rest)*) {
                            if let __MachineEvent::$evt $($evt_rest)* = &$ev {
                                if true $(&& $g($st, $cx_id, &$ev))? {
                                    $( $a($st, $cx_id, &$ev, $fx); )?
                                    $crate::__machine_assign!($st, __MachineState, $tgt $($tgt_rest)*);
                                    return __MachineOutcome::$handled;
                                }
                            }
                        }
                    )*
                    __MachineOutcome::$unhandled
                }
            }
        }
        $vis const $nodes: &[$crate::Node] = &[
            $( $crate::Node { id: ::core::stringify!($node) } ),+
        ];
        $vis const $edges: &[$crate::Edge] = &[
            $(
                $crate::__machine_edge!(
                    $id, $src, $evt,
                    [ $($g)? ], [ $($a)? ],
                    $tgt $($tgt_rest)*
                )
            ),*
        ];
    };
}
