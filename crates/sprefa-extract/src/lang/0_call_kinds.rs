//! The call-kind table: the tree-sitter node kinds that denote a call site in
//! the grammars the fallback actually loads. Collected the only
//! trustworthy way, one `ryi --family cst <fixture>` dump per language (issue
//! default-families-no-conditional), so a kind no loaded grammar emits is
//! absent and a call kind one does emit is present. This file is DATA, not
//! code: a new language contributes a row, never a branch in the projector.

use crate::family::CallKind;
use crate::types::LangKind;

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

/// Leaf kinds that carry a NAME, the exact replacement for the identifier
/// substring: every name-leaf kind the loaded grammars declare, collected
/// the same way as CALL_KINDS, one `ryi --family cst` dump per language
/// (issue kind-vocab-constraint). A grammar declares a subset; the rest
/// resolve to no id against it and match nothing there.
///
/// - `field_identifier`                       c, cpp, go, rust struct fields
/// - `identifier`                             c, csharp, css, elixir, go, java,
///                                            javascript, kotlin, lua, php,
///                                            python, ruby, rust, scala, ts
/// - `namespace_identifier`                   cpp
/// - `nested_type_identifier`                 ts, tsx, `A.B` in type position
/// - `package_identifier`                     go
/// - `property_identifier`                    javascript, ts, tsx
/// - `qualified_identifier`                   cpp, `ns::name`
/// - `scoped_identifier`                      java, rust
/// - `scoped_type_identifier`                 java, `A.B` types
/// - `shorthand_property_identifier`          ts, tsx object-literal shorthand
/// - `shorthand_property_identifier_pattern`  javascript, ts, tsx destructuring
/// - `simple_identifier`                      kotlin, swift
/// - `type_identifier`                        c, cpp, go, java, kotlin, rust,
///                                            scala, swift, ts, tsx
pub const NAME_LEAF_KINDS: &[&str] = &[
    "field_identifier",
    "identifier",
    "namespace_identifier",
    "nested_type_identifier",
    "package_identifier",
    "property_identifier",
    "qualified_identifier",
    "scoped_identifier",
    "scoped_type_identifier",
    "shorthand_property_identifier",
    "shorthand_property_identifier_pattern",
    "simple_identifier",
    "type_identifier",
];

/// Leaf kinds that carry a callee NAME inside a call node, beyond the
/// identifier kinds (NAME_LEAF_KINDS): php's bare `name`, haskell's
/// `variable`, bash's `word` under `command_name`.
pub const CALLEE_NAME_KINDS: &[&str] = &["name", "variable", "word"];

/// Child kinds the generic callee walk skips (astgrep.rs `callee_of`), the
/// exact replacement for the argument/suffix substrings: the argument
/// subtrees plus the kotlin/swift navigation suffixes. Collected
/// from the same per-language dumps; a grammar declares a subset, and bash
/// declares none (its words sit directly under `command`).
///
/// - `argument`                csharp, php
/// - `argument_list`           c, cpp, csharp, go, java, python, ruby
/// - `arguments`               elixir, javascript, lua, php, rust, scala, ts,
///                             tsx
/// - `block_argument`          ruby, `&blk`
/// - `bracketed_argument_list` csharp, `a[i]`
/// - `call_suffix`             kotlin, swift
/// - `constructor_suffix`      swift
/// - `hash_splat_argument`     ruby, `**h`
/// - `navigation_suffix`       kotlin, swift, `a.b`
/// - `splat_argument`          ruby, `*args`
/// - `template_argument_list`  cpp, `foo<int>(...)`
/// - `type_argument_list`      csharp, `Foo<int>(...)`
/// - `type_arguments`          java, kotlin, scala, swift, ts, tsx
/// - `value_argument`          kotlin, swift
/// - `value_argument_label`    swift
/// - `value_arguments`         kotlin, swift
pub const ARG_KINDS: &[&str] = &[
    "argument",
    "argument_list",
    "arguments",
    "block_argument",
    "bracketed_argument_list",
    "call_suffix",
    "constructor_suffix",
    "hash_splat_argument",
    "navigation_suffix",
    "splat_argument",
    "template_argument_list",
    "type_argument_list",
    "type_arguments",
    "value_argument",
    "value_argument_label",
    "value_arguments",
];

/// Call kinds whose callee is the FIRST name leaf (prefix application:
/// `map f xs` calls `map`). Every other kind takes the LAST name leaf before
/// the argument subtree, the trailing segment of a member chain (`s.fp(...)`
/// names `fp`).
pub const CALLEE_FIRST_KINDS: &[&str] = &["apply"];

/// The module as a CALL caller: a nameless whole-file cover def minted by the
/// call projectors so a module-level call site has a caller under
/// `Resolve<CallF>`. Not a call_def wire row (skipped in `flatten_call`, v5
/// emits no such def); `caller_name` answers null, the bench join's empty
/// src_name for module-level rows. Tag "module" collides with python's TypeF
/// ext tag only across families, which the vocab rail allows.
pub const MODULE_CALLER: CallKind = CallKind::Ext(LangKind {
    lang: "python",
    tag: "module",
});
