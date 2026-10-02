# alloy-scm Rust lab: fix printer choice, named props, module registry (branch lab/20261002-alloy-scm-rust, continue on it)

Lab: labs/isolated/lab-20261002-alloy-scm-rust-vs-tsp-rust. Current: 239 matchers -> 87 PASS / 30 DIFF / 122 GAP;
14 DIFFs are whitespace-only.
1. Comparison: add a whitespace-insensitive equality column (strip all whitespace) beside byte equality; README reports both.
2. Printer (core/0_print.tsx, now a lab-local copy you may change; note divergence from turnkey in README):
   prefer parses that keep separator/terminator literals (`,` in lists, `;` after tuple structs) over fewest-literals.
   Expected fixes: `<'a, T: 'a>`, `Result<User, AppError>`, `(bar: i32, baz: String)`, `(&self, f: ...)`, `struct Point(f64, f64);`.
3. rust/ policy: where_clause and mutable_specifier consumed as named props, not first-fit children
   (`struct Foo<T> where T: Serialize {`, `static mut BUFFER`).
4. Module registry: SourceFile/ModDirectory record declarations per file; references to another file's symbol emit
   `pub mod x;` + `use crate::x::Sym;` like A's 3_files/SourceFile (Reference.test.tsx:66/100/146, SourceFile.test.tsx:100).
Gate: rerun `pnpm test`; README totals before/after. Commit per step; messages end
`Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>`. No push. No Python. No hafley-rs cargo tests.
