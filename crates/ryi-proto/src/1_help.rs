use clap::{CommandFactory as _, FromArgMatches as _};
use crate::cli_auto::Ryi;

const TOP: &str = "ryi <PATH>...                    per-file facts from files, directories or globs
ryi capabilities                 declared languages, checkers and edit support
ryi schema                       JSONL record contract
ryi fast <PATH>...                written cross-file call and type facts
ryi slow <PATH>...                compiler and saved SCIP evidence
ryi scip <ROOT>                   raw SCIP facts or index records
ryi ingest <PATH>...              validated foreign TSI rows
ryi query --query <SCM> <PATH>...  captured syntax rows
ryi graph <question> <PATH>...    callers, type uses, paths or slices
ryi trail [N]                     recorded runs and their evidence
ryi diff --from <REV> --to <REV>  fact changes between revisions
ryi watch --root <DIR>            worktree fact deltas
ryi stratify --from <ENTRY>       dependency-first file ranks
ryi rename <FILE#OLD> <NEW>       bound occurrence edits, dry-run first
ryi move <OLD> <NEW>              file move and repaired import specifiers
ryi cleave <FILE#ITEM> <DEST>     item move with its imports and callers
ryi region <FILE> <ID>            generated-region drift or replacement

Use ryi <verb> --help for examples and that verb's flags.

Laws:
  Fact streams on stdout are JSONL.
  Edits are dry-run until --commit; region writes require --apply.
  Abstained edits carry their reason.
";

pub fn help_command() -> clap::Command {
    Ryi::command().name("ryi").help_template(TOP).after_help(None::<&str>)
}

impl Ryi {
    pub fn parse() -> Self {
        let matches = help_command().get_matches();
        Self::from_arg_matches(&matches).unwrap_or_else(|error| error.exit())
    }
}
