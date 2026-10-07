use clap::{CommandFactory as _, FromArgMatches as _};
use crate::cli_auto::Ryi;

const TOP: &str = "ryi <PATH>...                      per-file facts from files, directories or globs
ryi capabilities                   declared languages, checkers and edit support
ryi schema                         JSONL record contract
ryi fast <PATH>...                 written cross-file call and type facts
ryi slow <PATH>...                 compiler and saved SCIP evidence
ryi scip <ROOT>                    raw SCIP facts or index records
ryi ingest <PATH>...               validated foreign TSI rows
ryi query --query <SCM> <PATH>...  captured syntax rows
ryi graph <question> <PATH>...     callers, type uses, paths or slices
ryi trail [N]                      recorded runs and their evidence
ryi diff --from <REV> --to <REV>   fact changes between revisions
ryi watch --root <DIR>             worktree fact deltas
ryi stratify --from <ENTRY>        dependency-first file ranks
ryi rename <FILE#OLD> <NEW>        bound occurrence edits, dry-run first
ryi move <OLD> <NEW>               file move and repaired import specifiers
ryi cleave <FILE#ITEM> <DEST>      item move with its imports and callers
ryi region <FILE> <ID>             generated-region drift or replacement
ryi help <verb|flags>              examples, verb flags or shared flags
";

pub fn help_command() -> clap::Command {
    let mut command = Ryi::command().name("ryi").help_template(TOP).after_help(None::<&str>);
    for (name, examples) in crate::verb_help::EXAMPLES {
        let help = crate::verb_help::compact_help(command.find_subcommand(name).unwrap(), examples);
        command = command.mut_subcommand(name, |sub| sub.override_help(help));
    }
    command.subcommand(clap::Command::new("help")
        .about("Examples and shared flags")
        .arg(clap::Arg::new("topic").default_value("flags")))
}

impl Ryi {
    pub fn parse() -> Self {
        let matches = help_command().get_matches();
        Self::from_arg_matches(&matches).unwrap_or_else(|error| error.exit())
    }
}


pub fn local_help() -> bool {
    if std::env::args_os().nth(1).is_none_or(|arg| arg != "help") { return false; }
    let matches = help_command().get_matches();
    let topic = matches.subcommand_matches("help").and_then(|m| m.get_one::<String>("topic")).unwrap();
    let mut command = help_command();
    if topic == "flags" || topic == "extract" {
        let text = crate::verb_help::compact_help(&Ryi::command(), &["ryi --lines src/"]);
        print!("{text}\nGlobal: --format jsonl\nLogging: RUST_LOG; HAFLEY_LOG_FORMAT=json|text\nSQLite: --resolve stores phase 1 per-file and phase 2 project rows; stdout streams phase 2 only. Every SQLite unresolved row has its path.\n");
    } else if let Some(sub) = command.find_subcommand_mut(topic) {
        let _ = sub.print_help();
    } else {
        eprintln!("ryi: unknown help topic {topic}. Run: ryi help flags");
        std::process::exit(2);
    }
    true
}
