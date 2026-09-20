# Writing `.scm` queries that use ast-grep's relational operators

OUTLINE. Each heading carries one line stating what the section asserts.

## What this surface is

`.scm` gains ast-grep's `Inside`, `Has`, `Follows` and `Precedes` because `lower_scm` rewrites a query file into an `AstRule` tree, and ast-grep evaluates that tree.

## Terms a `.scm` author does not have yet

A table of `AstRule`, `Matches`, `utils`, `StopBy`, `lower_scm` and `ScmProgram`, each with its meaning and its `file:line`.

## Why upstream `.scm` has no relations

`tree-sitter#880` is open since 2021-01-13, the C library evaluates no predicate, and a predicate argument is a capture, an identifier or a string.

## Split a file into named rules and a reporting rule

A top-level pattern with a `@label` becomes a `utils` entry, an unlabelled pattern becomes the reporting rule, several become `Any`, and none becomes `Any` over every label.

## Choose the node a pattern reports

Every predicate names one capture, that capture's host node is the reported node, and the enclosing pattern becomes an `Inside` constraint.

## Pick a predicate

One table of `#inside?`, `#has?`, `#follows?`, `#precedes?`, `#match?`, `#pattern?` and the `not-` prefix, each with its lowered `AstRule` and one `.scm` line.

## Refer to another pattern by name

`(#inside? @m scope)` lowers to `Inside { rule: Matches("scope"), stop_by: Some(End("end")) }`, and the reference is a name because an argument cannot hold a nested rule.

## Set where the walk stops

A string third argument is the walk mode `"end"` or `"neighbor"`, an identifier third argument is a label that lowers to `StopBy::Rule`, and any other string is `UnknownStopBy`.

## Know what the surface drops

Field selectors, supertypes and quantifiers are parsed and discarded, `(_)` and `(MISSING x)` are `Syntax` errors, and a negated field lowers its field name as a kind.

## Read every refusal

One table of the seven `ScmLowerError` variants at lower time plus `AstRuleError::UnknownKind` at run time, each with its input and its literal message.

## Run the same rule from `.scm` and from YAML

`examples/scm_vs_yaml.rs` lowers one `.scm` query, decodes one hand-written YAML twin, and prints equal rule trees and equal match sets.

## Check containment with a complement

`#inside?` and `#not-inside?` over `src/project.rs` return two counts whose sum equals the unfiltered count.

## Languages the query can target

`RyiLang::parse_name` accepts five grammars of this crate plus the `SupportLang` roster of `ast-grep-language`, counted from source.
