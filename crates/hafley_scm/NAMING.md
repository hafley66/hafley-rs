# Filesystem and naming rules, from the 2026-09-21 session

The filesystem is the map. It is read before any file is opened. A path is a
sentence, read top down, and every segment adds one thing the parent did not
say.

## Rules

1. A directory carries its noun once. Children never repeat it. A child's name
   is only what it adds. `edge_rule/walk/ancestor.rs`, not
   `edge_rule/edge_rule_walk/edge_rule_walk_ancestor.rs`.
2. Leaf names may repeat across siblings (`types.rs` under two parents). Never
   down one path.
3. Verb files end in their object: `split_predicates_into_kind_queries`, not
   `split`. `run_over_file_tree`, not `run`. A verb with nothing after it is
   not a name.
4. Nouns carry their object: `captured_span`, not `span`; `match_arena`, not
   `arena`. Single generic nouns (`program`, `plan`, `unit`, `join`, `labeled`,
   `lifted`) are not names; they describe a shape, not a thing.
5. A file that touches an external API says so in its name: `ts_` prefix for
   `tree_sitter::`. A reader knows where the FFI edge is without opening it.
6. If a thing was worth a file or folder, it has more than one word of
   information to its name. Single-word files and folders are refused.
7. One type per file under `types/`. Structs, enums, traits are files, so the
   listing is the type index.
8. A pipeline is a folder of ordered stages; each stage is a folder of steps
   when it has more than one. Dependencies of the pipeline (`edge_rule/`) sit
   beside it, never inside a stage, and never import the pipeline.
9. Names come from three sources only: the library's own word (tree-sitter says
   `capture`, `kind`, `predicate`, `cursor`), a word the owner gave, or a plain
   physical object. Coined abstractions from the model are refused; an unnamed
   slot stays `TODO_NAME` until the owner fills it.
10. Two languages in one file is a leak. Language knowledge lives in that
    language's own file or folder.
11. Comments state constraints the code cannot show. Two consecutive lines at
    most; the hook enforces it. Placeholders are `todo!()` fns with a one-line
    contract, not comment blocks.

## Likes, observed

- `scm_edge_query/edge_rule/walk/1_ancestor_and_parent.rs`: no word twice on
  the way down; the path reads as a sentence.
- `ts_` prefix marking tree-sitter calls.
- `capture` and `kind`, because tree-sitter already says them.
- `arena` as "a closed space of relational pointing", one per run, rows index
  by `Range<u32>`, no lifetimes.
- Verb files that end in their object.
- The RxJS pipe as the second render of the same filesystem: `map(stage_1)`,
  `map(stage_2)`, `switchMap(files -> scan(stage_3))`.

## Dislikes, observed

- `Program`, `Plan`, `Join`, `Labeled`, `Lifted`, `Landmark`, `Anchor`,
  `Wall`, `Mutate`: shape words or coinages, not names.
- `catch` for a captured node: a keyword.
- `edge`/`kind` used for two different things in one crate (`EdgeKind` vs
  tree-sitter node kind).
- `split`, `compile`, `run` as bare file names.
- `tree$`, `queryExt$` in the pipe: bare or coined stream names.
- Tables in place of a filesystem or a pipe when asked for one.
- A flat `src/` for a crate with stages.
- `compile` for a step that calls `Query::new`; tree-sitter compiles, we
  build.
- Long words repeated in every file of a folder.
- Prose where a list or a tree was asked for.

## Read-aloud render of the current tree

"hafley scm. Types: the extended query; a predicate; how to walk; where to
stop; the match arena; a match row; a captured span; the error. Pipeline,
stage one, split predicates into kind queries: tree-sitter reads the general
predicates; parse each into a predicate; mint the kind query text. Stage two,
build the extended query: tree-sitter makes one query per kind; intern the
names. Stage three, run over a file tree: tree-sitter kind cursors into sorted
ids; tree-sitter user cursor into candidates; test predicates per candidate;
append to the match arena; tree-sitter match limit check. Stage four, explain
the plan: print queries; print predicates. Walk: the trait; tree-sitter
ancestor; tree-sitter descendant; dispatch by direction. Tests: corpus pinned
counts; one test per direction; allocation and query-new budget. Bench: corpus
under divan's allocation profiler."

If a segment cannot be read aloud as a phrase a stranger would follow, it is
misnamed.
