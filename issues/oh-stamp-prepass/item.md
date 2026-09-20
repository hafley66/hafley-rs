---
created: 2026-09-19
updated: 2026-09-19
type: improvement
status: open
priority: low
related: ['@oh-test-kit']
labels: [observability]
---

# Stamp missing oh::test attributes instead of trusting recall

## Description

Idea only. Do not act. Written down so it survives the session.

## The problem

An AI writing a new test forgets the attribute. A bare `#[test]` without `use oh::test;`
in scope silently resolves to the builtin, gets no budget, no seed, no drain, and can
run forever. Forgetting is the default failure, and it is silent.

## The idea

A pre-pass stamps it instead of trusting anyone to remember. Walk the git diff for
added test functions, and for each one missing the attribute, insert it and insert the
import if absent. AST-grep or the extraction runtime already in this org can find them;
this is the same shape of work the language tooling already does.

The joke being that the tooling being built to extract and rewrite code across languages
would be used to rewrite its own tests.

## What makes it hard

To run before every build you would have to sit in front of `rustc` and `cargo`, which
means intercepting the driver. That is a large surface and a bad place to live.

## Cheaper placements, in rough order of sanity

| where | catches | cost |
|---|---|---|
| a `just` recipe run before `cargo test` | anything run through the recipe | zero, and bypassable |
| a pre-commit hook | anything committed | bypassable with `--no-verify` |
| a CI rail that fails on an unstamped test | everything that merges | catches it late |
| a scanner test inside the crate that greps `src/**` and `tests/**` | everything, at test time | it is itself a test, so it runs when tests run |

## The one that probably wins

The scanner test. It needs no interception, no driver wrapping, no hook. It fails the
battery when an unstamped `#[test]` exists, names the file and line, and the fix is
mechanical. Same shape as the existing bounded-loop scanner.

The stamping pre-pass then becomes a convenience that fixes what the scanner reports,
not a thing that has to be in the build path.

## Open

Whether the scanner can distinguish `#[test]` resolved to `oh::test` from the builtin
without doing name resolution. A grep sees the same three characters either way, so it
likely has to check for the `use oh::test;` import at file scope and treat its absence
as the failure.
