//! The call-kind table: the tree-sitter node kinds that denote a call site in
//! the grammars the ast-grep fallback actually loads. Collected the only
//! trustworthy way, one `ryi --family cst <fixture>` dump per language (issue
//! default-families-no-conditional), so a kind no loaded grammar emits is
//! absent and a call kind one does emit is present. This file is DATA, not
//! code: a new language contributes a row, never a branch in the projector.

/// Node kinds that ARE a call site, sorted. The grammars that emit each kind:
///
/// - `apply`                      haskell function application
/// - `call`                       ruby, elixir (ruby: `obj.bar` is a call too)
/// - `call_expression`            c, cpp, scala, swift
/// - `command`                    bash, the simple-command invocation
/// - `function_call`              lua
/// - `function_call_expression`   php
/// - `invocation_expression`      c#
/// - `member_call_expression`     php, `$obj->m()`
/// - `method_invocation`          java
/// - `object_creation_expression` java, c#, a constructor invocation
/// - `scoped_call_expression`     php, `Foo::m()`
///
/// Scala's `infix_expression` is deliberately absent: the same kind carries
/// `a + b`, so including it would mint a call site from every arithmetic
/// expression. html and css load grammars with no call kind at all.
pub const CALL_KINDS: &[&str] = &[
    "apply",
    "call",
    "call_expression",
    "command",
    "function_call",
    "function_call_expression",
    "invocation_expression",
    "member_call_expression",
    "method_invocation",
    "object_creation_expression",
    "scoped_call_expression",
];

/// Leaf kinds that carry a callee NAME inside a call node, beyond the
/// identifier kinds (kind contains `identifier`): php's bare `name`, haskell's
/// `variable`, bash's `word` under `command_name`.
pub const CALLEE_NAME_KINDS: &[&str] = &["name", "variable", "word"];

/// Call kinds whose callee is the FIRST name leaf (prefix application:
/// `map f xs` calls `map`). Every other kind takes the LAST name leaf before
/// the argument subtree, the trailing segment of a member chain (`s.fp(...)`
/// names `fp`).
pub const CALLEE_FIRST_KINDS: &[&str] = &["apply"];
