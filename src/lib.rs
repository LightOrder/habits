//! Local, deterministic utilities for extracting useful patterns from shell history.
//!
//! This crate deliberately stops before shell integration, persistence, macro execution,
//! or interactive search. It makes history data trustworthy and inspectable first.

use std::collections::HashMap;
use std::path::Path;

/// The precise on-disk history format. Format selection is explicit because timestamp markers
/// are ambiguous with legal shell commands.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum HistoryFormat {
    BashPlain,
    BashTimestamped,
    ZshPlain,
    ZshExtended,
    PowerShell,
}

/// The shell that produced a recovered entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum HistorySource {
    Bash,
    Zsh,
    PowerShell,
}

impl HistoryFormat {
    pub fn source(self) -> HistorySource {
        match self {
            Self::BashPlain | Self::BashTimestamped => HistorySource::Bash,
            Self::ZshPlain | Self::ZshExtended => HistorySource::Zsh,
            Self::PowerShell => HistorySource::PowerShell,
        }
    }
}

/// One command recovered from a history file.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HistoryEntry {
    pub command: String,
    /// Unix seconds when the shell recorded them. `None` means the source did not provide one.
    pub timestamp: Option<i64>,
    pub source: HistorySource,
    /// Zero-based physical line where this recovered command begins.
    pub source_index: usize,
}

/// A contiguous, timestamp-backed unit of terminal work.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Batch {
    pub entries: Vec<HistoryEntry>,
}

impl Batch {
    pub fn commands(&self) -> Vec<&str> {
        self.entries
            .iter()
            .map(|entry| entry.command.as_str())
            .collect()
    }
}

/// Exact command frequency. Whitespace at each edge is ignored; internal syntax is untouched.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandFrequency {
    pub command: String,
    pub count: usize,
}

/// A repeated contiguous command sequence suitable for a future macro suggestion.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SequencePattern {
    pub commands: Vec<String>,
    pub occurrences: usize,
    /// Deterministic preference for longer, repeatedly observed sequences.
    pub rank: usize,
}

/// Parses a history-file payload without reading from disk.
///
/// `BashTimestamped` and `ZshExtended` preserve multiline records between timestamp markers.
/// Select a plain format when the source has no guaranteed timestamp-record convention.
pub fn parse_history(format: HistoryFormat, input: &str) -> Result<Vec<HistoryEntry>, String> {
    match format {
        HistoryFormat::BashPlain => Ok(parse_plain(input, HistorySource::Bash)),
        HistoryFormat::BashTimestamped => parse_bash_timestamped(input),
        HistoryFormat::ZshPlain => Ok(parse_plain(input, HistorySource::Zsh)),
        HistoryFormat::ZshExtended => parse_zsh_extended(input),
        HistoryFormat::PowerShell => Ok(parse_plain(input, HistorySource::PowerShell)),
    }
}

/// Reads one user-selected history file and parses it according to an explicit format.
///
/// Deliberately does not discover paths or scan the filesystem; callers choose the file.
pub fn load_history_file(
    path: impl AsRef<Path>,
    format: HistoryFormat,
) -> Result<Vec<HistoryEntry>, String> {
    let path = path.as_ref();
    let input = std::fs::read_to_string(path)
        .map_err(|error| format!("failed to read `{}`: {error}", path.display()))?;
    parse_history(format, &input)
}

/// Counts trimmed exact commands, ordered by descending count then lexical command text.
pub fn command_frequencies(entries: &[HistoryEntry]) -> Vec<CommandFrequency> {
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for entry in entries {
        *counts.entry(entry.command.trim()).or_default() += 1;
    }

    let mut frequencies: Vec<_> = counts
        .into_iter()
        .filter(|(command, _)| !command.is_empty())
        .map(|(command, count)| CommandFrequency {
            command: command.to_owned(),
            count,
        })
        .collect();
    frequencies.sort_by(|left, right| {
        right
            .count
            .cmp(&left.count)
            .then_with(|| left.command.cmp(&right.command))
    });
    frequencies
}

/// Groups adjacent timestamped entries when their gap is at most `max_gap_seconds`.
///
/// A missing, decreasing, or excessively separated timestamp is an explicit boundary rather
/// than a guessed time. Negative gap values are treated as zero.
pub fn batches(entries: &[HistoryEntry], max_gap_seconds: i64) -> Vec<Batch> {
    let max_gap_seconds = max_gap_seconds.max(0);
    let mut batches = Vec::new();
    let mut current = Vec::new();
    let mut previous_timestamp = None;

    for entry in entries {
        let Some(timestamp) = entry.timestamp else {
            flush_batch(&mut batches, &mut current);
            previous_timestamp = None;
            continue;
        };

        let starts_new_batch = previous_timestamp.is_some_and(|previous| {
            timestamp <= previous || timestamp.saturating_sub(previous) > max_gap_seconds
        });
        if starts_new_batch {
            flush_batch(&mut batches, &mut current);
        }
        current.push(entry.clone());
        previous_timestamp = Some(timestamp);
    }
    flush_batch(&mut batches, &mut current);
    batches
}

/// Finds repeated contiguous n-grams within timestamp-valid batches.
///
/// `rank = occurrences * length²`: a transparent heuristic that favors longer repeated work
/// flows over isolated command frequency. No sequence crosses a batch boundary.
pub fn repeated_sequences(
    batches: &[Batch],
    min_length: usize,
    max_length: usize,
    min_occurrences: usize,
) -> Vec<SequencePattern> {
    if min_length == 0 || min_length > max_length || min_occurrences < 2 {
        return Vec::new();
    }

    let mut counts: HashMap<Vec<String>, usize> = HashMap::new();
    for batch in batches {
        for length in min_length..=max_length.min(batch.entries.len()) {
            for window in batch.entries.windows(length) {
                let commands = window.iter().map(|entry| entry.command.clone()).collect();
                *counts.entry(commands).or_default() += 1;
            }
        }
    }

    let mut patterns: Vec<_> = counts
        .into_iter()
        .filter(|(_, occurrences)| *occurrences >= min_occurrences)
        .map(|(commands, occurrences)| SequencePattern {
            rank: occurrences * commands.len() * commands.len(),
            commands,
            occurrences,
        })
        .collect();
    patterns.sort_by(|left, right| {
        right
            .rank
            .cmp(&left.rank)
            .then_with(|| right.occurrences.cmp(&left.occurrences))
            .then_with(|| left.commands.cmp(&right.commands))
    });
    patterns
}

fn parse_bash_timestamped(input: &str) -> Result<Vec<HistoryEntry>, String> {
    let mut entries = Vec::new();
    let mut current_timestamp = None;
    let mut current_start = 0;
    let mut current_lines = Vec::new();

    for (line_index, line) in input.lines().enumerate() {
        if let Some(timestamp) = parse_bash_timestamp_marker(line)? {
            flush_timestamped_entry(
                &mut entries,
                &mut current_lines,
                current_timestamp.take(),
                current_start,
                HistorySource::Bash,
            );
            current_timestamp = Some(timestamp);
            current_start = line_index + 1;
        } else if current_timestamp.is_some() {
            current_lines.push(line);
        } else {
            push_entry(&mut entries, line, None, HistorySource::Bash, line_index);
        }
    }
    flush_timestamped_entry(
        &mut entries,
        &mut current_lines,
        current_timestamp,
        current_start,
        HistorySource::Bash,
    );
    Ok(entries)
}

fn parse_zsh_extended(input: &str) -> Result<Vec<HistoryEntry>, String> {
    let mut entries = Vec::new();
    let mut current_timestamp = None;
    let mut current_start = 0;
    let mut current_lines = Vec::new();

    for (line_index, line) in input.lines().enumerate() {
        if let Some((timestamp, command)) = parse_zsh_extended_marker(line)? {
            flush_timestamped_entry(
                &mut entries,
                &mut current_lines,
                current_timestamp.take(),
                current_start,
                HistorySource::Zsh,
            );
            current_timestamp = Some(timestamp);
            current_start = line_index;
            current_lines.push(command);
        } else if current_timestamp.is_some() {
            current_lines.push(line);
        } else {
            push_entry(&mut entries, line, None, HistorySource::Zsh, line_index);
        }
    }
    flush_timestamped_entry(
        &mut entries,
        &mut current_lines,
        current_timestamp,
        current_start,
        HistorySource::Zsh,
    );
    Ok(entries)
}

fn parse_bash_timestamp_marker(line: &str) -> Result<Option<i64>, String> {
    let Some(raw_timestamp) = line.strip_prefix('#') else {
        return Ok(None);
    };
    if raw_timestamp.is_empty()
        || !raw_timestamp
            .chars()
            .all(|character| character.is_ascii_digit())
    {
        return Ok(None);
    }
    raw_timestamp
        .parse::<i64>()
        .map(Some)
        .map_err(|error| format!("invalid Bash timestamp `{raw_timestamp}`: {error}"))
}

fn parse_zsh_extended_marker(line: &str) -> Result<Option<(i64, &str)>, String> {
    let Some(rest) = line.strip_prefix(": ") else {
        return Ok(None);
    };
    let Some((metadata, command)) = rest.split_once(';') else {
        return Ok(None);
    };
    let Some((raw_timestamp, raw_duration)) = metadata.split_once(':') else {
        return Ok(None);
    };
    if raw_timestamp.is_empty()
        || raw_duration.is_empty()
        || !raw_timestamp
            .chars()
            .all(|character| character.is_ascii_digit())
        || !raw_duration
            .chars()
            .all(|character| character.is_ascii_digit())
    {
        return Ok(None);
    }
    raw_timestamp
        .parse::<i64>()
        .map(|timestamp| Some((timestamp, command)))
        .map_err(|error| format!("invalid Zsh timestamp `{raw_timestamp}`: {error}"))
}

fn parse_plain(input: &str, source: HistorySource) -> Vec<HistoryEntry> {
    let mut entries = Vec::new();
    for (line_index, line) in input.lines().enumerate() {
        push_entry(&mut entries, line, None, source, line_index);
    }
    entries
}

fn flush_timestamped_entry(
    entries: &mut Vec<HistoryEntry>,
    lines: &mut Vec<&str>,
    timestamp: Option<i64>,
    source_index: usize,
    source: HistorySource,
) {
    if let Some(timestamp) = timestamp {
        let command = lines.join("\n");
        push_entry(entries, &command, Some(timestamp), source, source_index);
    }
    lines.clear();
}

fn push_entry(
    entries: &mut Vec<HistoryEntry>,
    command: &str,
    timestamp: Option<i64>,
    source: HistorySource,
    source_index: usize,
) {
    let command = command.trim();
    if command.is_empty() {
        return;
    }
    entries.push(HistoryEntry {
        command: command.to_owned(),
        timestamp,
        source,
        source_index,
    });
}

fn flush_batch(batches: &mut Vec<Batch>, current: &mut Vec<HistoryEntry>) {
    if !current.is_empty() {
        batches.push(Batch {
            entries: std::mem::take(current),
        });
    }
}
