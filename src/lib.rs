//! Local, deterministic utilities for extracting useful patterns from shell history.
//!
//! This crate deliberately stops before shell integration, persistence, macro execution,
//! or interactive search. It makes history data trustworthy and inspectable first.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub mod selector;
pub mod shell;

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

/// A conventional, explicitly enumerated history location.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HistoryPath {
    pub source: HistorySource,
    pub path: PathBuf,
}

/// Metadata and parsed entries for one discovered history location.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiscoveredHistory {
    pub source: HistorySource,
    pub path: PathBuf,
    pub exists: bool,
    pub format: HistoryFormat,
    pub format_selection: &'static str,
    pub entry_count: usize,
    pub entries: Vec<HistoryEntry>,
    /// A privacy-safe diagnostic; it never contains file contents.
    pub error: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CandidateKind {
    TypedInput,
    History,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContextEvidence {
    pub command: String,
    pub count: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RankedCandidate {
    pub command: String,
    pub kind: CandidateKind,
    pub frequency: usize,
    pub fuzzy_score: usize,
    pub predecessors: Vec<ContextEvidence>,
    pub successors: Vec<ContextEvidence>,
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

/// Returns only the three conventional history locations, in stable source order.
pub fn conventional_history_paths(home: &Path) -> Result<Vec<HistoryPath>, String> {
    if !home.is_absolute() {
        return Err("home directory must be an absolute path".to_owned());
    }
    Ok(vec![
        HistoryPath {
            source: HistorySource::Zsh,
            path: home.join(".zsh_history"),
        },
        HistoryPath {
            source: HistorySource::Bash,
            path: home.join(".bash_history"),
        },
        HistoryPath {
            source: HistorySource::PowerShell,
            path: home.join(".local/share/powershell/PSReadLine/ConsoleHost_history.txt"),
        },
    ])
}

/// Reads and parses an explicit ordered set of history paths without scanning directories.
pub fn discover_histories(paths: &[HistoryPath]) -> Vec<DiscoveredHistory> {
    paths
        .iter()
        .map(|candidate| {
            let fallback = plain_format(candidate.source);
            let mut result = DiscoveredHistory {
                source: candidate.source,
                path: candidate.path.clone(),
                exists: candidate.path.is_file(),
                format: fallback,
                format_selection: "plain fallback",
                entry_count: 0,
                entries: Vec::new(),
                error: None,
            };
            if !result.exists {
                return result;
            }
            let input = match std::fs::read_to_string(&candidate.path) {
                Ok(input) => input,
                Err(_) => {
                    result.error = Some("history file could not be read as UTF-8 text".to_owned());
                    return result;
                }
            };
            let (format, selection) = auto_format(candidate.source, &input);
            result.format = format;
            result.format_selection = selection;
            match parse_history(format, &input) {
                Ok(entries) => {
                    result.entry_count = entries.len();
                    result.entries = entries;
                }
                Err(_) => {
                    result.error = Some("history file could not be parsed".to_owned());
                }
            }
            result
        })
        .collect()
}

/// Ranks unique commands while preserving the typed input as a navigable first-class row.
///
/// Each inner history is an adjacency boundary, so context never crosses source files.
pub fn rank_candidates(histories: &[Vec<HistoryEntry>], query: &str) -> Vec<RankedCandidate> {
    #[derive(Default)]
    struct Aggregate {
        count: usize,
        first_seen: usize,
        predecessors: HashMap<String, (usize, usize)>,
        successors: HashMap<String, (usize, usize)>,
    }

    let mut aggregates: HashMap<String, Aggregate> = HashMap::new();
    let mut observation = 0;
    for history in histories {
        for (index, entry) in history.iter().enumerate() {
            let command = entry.command.trim();
            if command.is_empty() {
                continue;
            }
            let aggregate = aggregates.entry(command.to_owned()).or_insert_with(|| {
                let first_seen = observation;
                observation += 1;
                Aggregate {
                    first_seen,
                    ..Aggregate::default()
                }
            });
            aggregate.count += 1;
            if let Some(previous) = index.checked_sub(1).and_then(|i| history.get(i)) {
                add_evidence(
                    &mut aggregate.predecessors,
                    previous.command.trim(),
                    observation,
                );
            }
            if let Some(next) = history.get(index + 1) {
                add_evidence(&mut aggregate.successors, next.command.trim(), observation);
            }
        }
    }

    let mut history_candidates: Vec<_> = aggregates
        .into_iter()
        .filter_map(|(command, aggregate)| {
            fuzzy_score(&command, query).map(|score| {
                let evidence = aggregate
                    .predecessors
                    .values()
                    .chain(aggregate.successors.values())
                    .map(|(count, _)| count)
                    .sum::<usize>();
                (
                    RankedCandidate {
                        command,
                        kind: CandidateKind::History,
                        frequency: aggregate.count,
                        fuzzy_score: score,
                        predecessors: sorted_evidence(aggregate.predecessors),
                        successors: sorted_evidence(aggregate.successors),
                    },
                    evidence,
                    aggregate.first_seen,
                )
            })
        })
        .collect();
    history_candidates.sort_by(
        |(left, left_evidence, left_seen), (right, right_evidence, right_seen)| {
            right
                .fuzzy_score
                .cmp(&left.fuzzy_score)
                .then_with(|| right.frequency.cmp(&left.frequency))
                .then_with(|| right_evidence.cmp(left_evidence))
                .then_with(|| left_seen.cmp(right_seen))
                .then_with(|| left.command.cmp(&right.command))
        },
    );

    let mut ranked = vec![RankedCandidate {
        command: query.to_owned(),
        kind: CandidateKind::TypedInput,
        frequency: 0,
        fuzzy_score: usize::MAX,
        predecessors: Vec::new(),
        successors: Vec::new(),
    }];
    ranked.extend(
        history_candidates
            .into_iter()
            .map(|(candidate, _, _)| candidate),
    );
    ranked
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

fn plain_format(source: HistorySource) -> HistoryFormat {
    match source {
        HistorySource::Bash => HistoryFormat::BashPlain,
        HistorySource::Zsh => HistoryFormat::ZshPlain,
        HistorySource::PowerShell => HistoryFormat::PowerShell,
    }
}

fn auto_format(source: HistorySource, input: &str) -> (HistoryFormat, &'static str) {
    match source {
        HistorySource::Zsh if well_formed_zsh_extended(input) => {
            (HistoryFormat::ZshExtended, "well-formed timestamp markers")
        }
        HistorySource::Bash if well_formed_bash_timestamped(input) => (
            HistoryFormat::BashTimestamped,
            "well-formed timestamp markers",
        ),
        _ => (plain_format(source), "plain fallback"),
    }
}

fn well_formed_zsh_extended(input: &str) -> bool {
    let lines: Vec<_> = input.lines().filter(|line| !line.is_empty()).collect();
    if lines.is_empty() || parse_zsh_extended_marker(lines[0]).ok().flatten().is_none() {
        return false;
    }
    lines.iter().all(|line| {
        !line.starts_with(": ") || parse_zsh_extended_marker(line).ok().flatten().is_some()
    })
}

fn well_formed_bash_timestamped(input: &str) -> bool {
    let lines: Vec<_> = input.lines().collect();
    if lines.is_empty() {
        return false;
    }
    let mut index = 0;
    let mut records = 0;
    while index < lines.len() {
        if parse_bash_timestamp_marker(lines[index])
            .ok()
            .flatten()
            .is_none()
        {
            return false;
        }
        index += 1;
        let command_start = index;
        while index < lines.len() && !lines[index].starts_with('#') {
            index += 1;
        }
        if command_start == index {
            return false;
        }
        records += 1;
    }
    records > 0
}

fn add_evidence(evidence: &mut HashMap<String, (usize, usize)>, command: &str, observation: usize) {
    if command.is_empty() {
        return;
    }
    let item = evidence
        .entry(command.to_owned())
        .or_insert((0, observation));
    item.0 += 1;
}

fn sorted_evidence(evidence: HashMap<String, (usize, usize)>) -> Vec<ContextEvidence> {
    let mut evidence: Vec<_> = evidence
        .into_iter()
        .map(|(command, (count, first_seen))| (ContextEvidence { command, count }, first_seen))
        .collect();
    evidence.sort_by(|(left, left_seen), (right, right_seen)| {
        right
            .count
            .cmp(&left.count)
            .then_with(|| left_seen.cmp(right_seen))
            .then_with(|| left.command.cmp(&right.command))
    });
    evidence.into_iter().map(|(item, _)| item).collect()
}

fn fuzzy_score(command: &str, query: &str) -> Option<usize> {
    let command = command.to_lowercase();
    let query = query.to_lowercase();
    if query.is_empty() {
        return Some(0);
    }
    let command_chars: Vec<_> = command.chars().collect();
    let mut positions = Vec::new();
    let mut start = 0;
    for wanted in query.chars() {
        let relative = command_chars[start..]
            .iter()
            .position(|candidate| *candidate == wanted)?;
        let position = start + relative;
        positions.push(position);
        start = position + 1;
    }
    let first = positions[0];
    let span = positions[positions.len() - 1] - first;
    Some(
        1_000_000usize
            .saturating_sub(span.saturating_mul(100))
            .saturating_sub(first),
    )
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
