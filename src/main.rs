use std::env;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use habits::{HistoryFormat, batches, command_frequencies, load_history_file, repeated_sequences};

const USAGE: &str = "\
Usage:
  habits inspect --format <FORMAT> --path <PATH> [--gap-seconds <SECONDS>] [--top <N>] [--json] [--show-commands]
  habits paths [--json]
  habits --help
  habits --version

Formats: bash-plain, bash-timestamped, zsh-plain, zsh-extended, powershell";

fn main() -> ExitCode {
    match run(env::args().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::from(2)
        }
    }
}

fn run(arguments: Vec<String>) -> Result<(), String> {
    let Some(command) = arguments.first().map(String::as_str) else {
        println!("{USAGE}");
        return Ok(());
    };

    match command {
        "--help" | "-h" | "help" => {
            if arguments.len() != 1 {
                return usage_error("help takes no arguments");
            }
            println!("{USAGE}");
            Ok(())
        }
        "--version" | "-V" => {
            if arguments.len() != 1 {
                return usage_error("version takes no arguments");
            }
            println!("habits {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        "inspect" => inspect(&arguments[1..]),
        "paths" => paths(&arguments[1..]),
        unknown => usage_error(&format!("unknown command `{unknown}`")),
    }
}

struct InspectOptions {
    format: HistoryFormat,
    format_name: String,
    path: PathBuf,
    gap_seconds: i64,
    top: usize,
    json: bool,
    show_commands: bool,
}

fn inspect(arguments: &[String]) -> Result<(), String> {
    let options = parse_inspect_options(arguments)?;
    let entries = load_history_file(&options.path, options.format).map_err(|_| {
        format!(
            "could not read or parse requested history path `{}`",
            options.path.display()
        )
    })?;
    let timestamped_count = entries
        .iter()
        .filter(|entry| entry.timestamp.is_some())
        .count();
    let batches = batches(&entries, options.gap_seconds);
    let frequencies = command_frequencies(&entries);
    let patterns = repeated_sequences(&batches, 2, 6, 2);

    if options.show_commands {
        eprintln!("warning: --show-commands exposes raw history commands");
    }

    if options.json {
        render_inspect_json(
            &options,
            entries.len(),
            timestamped_count,
            batches.len(),
            &frequencies,
            &patterns,
        );
    } else {
        println!("format: {}", options.format_name);
        println!("entries: {}", entries.len());
        println!("timestamped: {timestamped_count}");
        println!("batches: {}", batches.len());
        println!("top frequencies:");
        for frequency in frequencies.iter().take(options.top) {
            if options.show_commands {
                println!("  {} occurrences: {}", frequency.count, frequency.command);
            } else {
                println!("  {} occurrences", frequency.count);
            }
        }
        println!("repeated sequences:");
        for pattern in patterns.iter().take(options.top) {
            if options.show_commands {
                println!(
                    "  rank {}, {} occurrences, length {}: {}",
                    pattern.rank,
                    pattern.occurrences,
                    pattern.commands.len(),
                    pattern.commands.join(" → ")
                );
            } else {
                println!(
                    "  rank {}, {} occurrences, length {}",
                    pattern.rank,
                    pattern.occurrences,
                    pattern.commands.len()
                );
            }
        }
    }
    Ok(())
}

fn parse_inspect_options(arguments: &[String]) -> Result<InspectOptions, String> {
    let mut format = None;
    let mut path = None;
    let mut gap_seconds = 300_i64;
    let mut top = 10_usize;
    let mut json = false;
    let mut show_commands = false;
    let mut index = 0;

    while index < arguments.len() {
        match arguments[index].as_str() {
            "--format" => {
                let value = option_value(arguments, &mut index, "--format")?;
                if format.is_some() {
                    return usage_error("duplicate --format");
                }
                let parsed = parse_format(value)
                    .ok_or_else(|| format!("invalid format `{value}`\n\n{USAGE}"))?;
                format = Some((parsed, value.to_owned()));
            }
            "--path" => {
                let value = option_value(arguments, &mut index, "--path")?;
                if path.is_some() {
                    return usage_error("duplicate --path");
                }
                path = Some(PathBuf::from(value));
            }
            "--gap-seconds" => {
                let value = option_value(arguments, &mut index, "--gap-seconds")?;
                gap_seconds = value
                    .parse::<i64>()
                    .ok()
                    .filter(|value| *value >= 0)
                    .ok_or_else(|| format!("invalid --gap-seconds value\n\n{USAGE}"))?;
            }
            "--top" => {
                let value = option_value(arguments, &mut index, "--top")?;
                top = value
                    .parse::<usize>()
                    .map_err(|_| format!("invalid --top value\n\n{USAGE}"))?;
            }
            "--json" => json = true,
            "--show-commands" => show_commands = true,
            option => return usage_error(&format!("unknown inspect option `{option}`")),
        }
        index += 1;
    }

    let (format, format_name) = format.ok_or_else(|| format!("missing --format\n\n{USAGE}"))?;
    let path = path.ok_or_else(|| format!("missing --path\n\n{USAGE}"))?;
    Ok(InspectOptions {
        format,
        format_name,
        path,
        gap_seconds,
        top,
        json,
        show_commands,
    })
}

fn option_value<'a>(
    arguments: &'a [String],
    index: &mut usize,
    option: &str,
) -> Result<&'a str, String> {
    *index += 1;
    arguments
        .get(*index)
        .map(String::as_str)
        .filter(|value| !value.starts_with("--"))
        .ok_or_else(|| format!("missing value for {option}\n\n{USAGE}"))
}

fn parse_format(value: &str) -> Option<HistoryFormat> {
    match value {
        "bash-plain" => Some(HistoryFormat::BashPlain),
        "bash-timestamped" => Some(HistoryFormat::BashTimestamped),
        "zsh-plain" => Some(HistoryFormat::ZshPlain),
        "zsh-extended" => Some(HistoryFormat::ZshExtended),
        "powershell" => Some(HistoryFormat::PowerShell),
        _ => None,
    }
}

fn render_inspect_json(
    options: &InspectOptions,
    entry_count: usize,
    timestamped_count: usize,
    batch_count: usize,
    frequencies: &[habits::CommandFrequency],
    patterns: &[habits::SequencePattern],
) {
    print!(
        "{{\"format\":{},\"entry_count\":{entry_count},\"timestamped_count\":{timestamped_count},\"batch_count\":{batch_count},\"top_frequencies\":[",
        json_string(&options.format_name)
    );
    for (index, frequency) in frequencies.iter().take(options.top).enumerate() {
        if index != 0 {
            print!(",");
        }
        print!("{{\"count\":{}", frequency.count);
        if options.show_commands {
            print!(",\"command\":{}", json_string(&frequency.command));
        }
        print!("}}");
    }
    print!("],\"repeated_sequences\":[");
    for (index, pattern) in patterns.iter().take(options.top).enumerate() {
        if index != 0 {
            print!(",");
        }
        print!(
            "{{\"rank\":{},\"occurrences\":{},\"length\":{}",
            pattern.rank,
            pattern.occurrences,
            pattern.commands.len()
        );
        if options.show_commands {
            print!(",\"commands\":[");
            for (command_index, command) in pattern.commands.iter().enumerate() {
                if command_index != 0 {
                    print!(",");
                }
                print!("{}", json_string(command));
            }
            print!("]");
        }
        print!("}}");
    }
    println!("]}}");
}

struct PathCandidate {
    path: PathBuf,
    format: &'static str,
}

fn paths(arguments: &[String]) -> Result<(), String> {
    let json = match arguments {
        [] => false,
        [argument] if argument == "--json" => true,
        _ => return usage_error("paths accepts only --json"),
    };
    let home = env::var_os("HOME").ok_or_else(|| "HOME is not set".to_owned())?;
    let home = PathBuf::from(home);
    if !home.is_absolute() {
        return Err("HOME must be an absolute path".to_owned());
    }
    let candidates = [
        PathCandidate {
            path: home.join(".zsh_history"),
            format: "zsh-plain",
        },
        PathCandidate {
            path: home.join(".bash_history"),
            format: "bash-plain",
        },
        PathCandidate {
            path: home.join(".local/share/powershell/PSReadLine/ConsoleHost_history.txt"),
            format: "powershell",
        },
    ];

    if json {
        print!("{{\"candidates\":[");
        for (index, candidate) in candidates.iter().enumerate() {
            if index != 0 {
                print!(",");
            }
            print!(
                "{{\"path\":{},\"exists\":{},\"format\":{}}}",
                json_path(&candidate.path),
                candidate.path.metadata().is_ok(),
                json_string(candidate.format)
            );
        }
        println!("]}}");
    } else {
        println!("Conventional history paths (suggested formats, not detected):");
        for candidate in candidates {
            println!(
                "{} | exists: {} | format: {}",
                candidate.path.display(),
                candidate.path.metadata().is_ok(),
                candidate.format
            );
        }
    }
    Ok(())
}

fn json_path(path: &Path) -> String {
    json_string(&path.to_string_lossy())
}

fn json_string(value: &str) -> String {
    let mut output = String::with_capacity(value.len() + 2);
    output.push('"');
    for character in value.chars() {
        match character {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\u{08}' => output.push_str("\\b"),
            '\u{0c}' => output.push_str("\\f"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            character if character <= '\u{1f}' => {
                use std::fmt::Write;
                write!(output, "\\u{:04x}", character as u32)
                    .expect("writing to String cannot fail");
            }
            character => output.push(character),
        }
    }
    output.push('"');
    output
}

fn usage_error<T>(message: &str) -> Result<T, String> {
    Err(format!("{message}\n\n{USAGE}"))
}
