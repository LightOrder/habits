use std::env;
use std::process::ExitCode;

use habits::{batches, command_frequencies, load_history_file, repeated_sequences, HistoryFormat};

fn main() -> ExitCode {
    let mut arguments = env::args().skip(1);
    let Some(format) = arguments.next().and_then(parse_format) else {
        eprintln!("usage: cargo run --example audit -- <bash-plain|bash-timestamped|zsh-plain|zsh-extended|powershell> <history-file>");
        return ExitCode::from(2);
    };
    let Some(path) = arguments.next() else {
        eprintln!("usage: cargo run --example audit -- <bash-plain|bash-timestamped|zsh-plain|zsh-extended|powershell> <history-file>");
        return ExitCode::from(2);
    };

    let entries = match load_history_file(&path, format) {
        Ok(entries) => entries,
        Err(error) => {
            eprintln!("habits: {error}");
            return ExitCode::FAILURE;
        }
    };
    let timestamped = entries.iter().filter(|entry| entry.timestamp.is_some()).count();
    let batches = batches(&entries, 300);
    let patterns = repeated_sequences(&batches, 2, 6, 2);

    println!("entries: {}", entries.len());
    println!("timestamped: {timestamped}");
    println!("batches (5m gap): {}", batches.len());
    println!("top command frequency counts (command text withheld):");
    for item in command_frequencies(&entries).into_iter().take(10) {
        println!("  {:>3} occurrences", item.count);
    }
    println!("repeated sequence candidates (command text withheld):");
    for pattern in patterns.into_iter().take(10) {
        println!(
            "  rank {:>3}, seen {:>2}x, length {}",
            pattern.rank,
            pattern.occurrences,
            pattern.commands.len()
        );
    }
    ExitCode::SUCCESS
}

fn parse_format(argument: String) -> Option<HistoryFormat> {
    match argument.as_str() {
        "bash-plain" => Some(HistoryFormat::BashPlain),
        "bash-timestamped" => Some(HistoryFormat::BashTimestamped),
        "zsh-plain" => Some(HistoryFormat::ZshPlain),
        "zsh-extended" => Some(HistoryFormat::ZshExtended),
        "powershell" | "ps" => Some(HistoryFormat::PowerShell),
        _ => None,
    }
}
