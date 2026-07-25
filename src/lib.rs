//! Local, deterministic utilities for extracting useful patterns from shell history.
//!
//! It provides local history discovery, deterministic ranking, and an optional interactive
//! selector. It deliberately excludes persistence, automatic shell configuration, and macro
//! execution.

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ModelKind {
    Prefix,
    Fuzzy,
    Frequency,
    Sequence,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScoredSuggestion {
    pub command: String,
    pub score: u8,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModelLane {
    pub model: ModelKind,
    pub suggestions: Vec<ScoredSuggestion>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SuggestionGrid {
    pub lanes: [ModelLane; 4],
    pub typed_input: String,
}

#[derive(Clone)]
struct GridAggregate {
    command: String,
    count: usize,
    first_seen: usize,
    fuzzy: usize,
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
        HistoryPath {
            source: HistorySource::PowerShell,
            path: home.join(
                "AppData/Roaming/Microsoft/Windows/PowerShell/PSReadLine/ConsoleHost_history.txt",
            ),
        },
        HistoryPath {
            source: HistorySource::PowerShell,
            path: home.join("AppData/Roaming/PowerShell/PSReadLine/ConsoleHost_history.txt"),
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

/// Produces four stable, model-local suggestion lanes from explicit history vectors.
pub fn suggestion_grid(histories: &[Vec<HistoryEntry>], query: &str) -> SuggestionGrid {
    let mut by_command: HashMap<String, (usize, usize)> = HashMap::new();
    let mut next_observation = 0;
    for entry in histories.iter().flatten() {
        let command = entry.command.trim();
        if command.is_empty() {
            continue;
        }
        let aggregate = by_command.entry(command.to_owned()).or_insert_with(|| {
            let first_seen = next_observation;
            next_observation += 1;
            (0, first_seen)
        });
        aggregate.0 += 1;
    }
    let compatible: Vec<_> = by_command
        .into_iter()
        .filter_map(|(command, (count, first_seen))| {
            fuzzy_score(&command, query).map(|fuzzy| GridAggregate {
                command,
                count,
                first_seen,
                fuzzy: fuzzy.max(1),
            })
        })
        .collect();

    let mut prefix: Vec<_> = compatible
        .iter()
        .filter_map(|item| {
            prefix_word_start(&item.command, query).map(|position| {
                (
                    item.clone(),
                    1_000_000usize.saturating_sub(position).max(1),
                    position,
                )
            })
        })
        .collect();
    prefix.sort_by(|(left, _, left_start), (right, _, right_start)| {
        left_start
            .cmp(right_start)
            .then_with(|| right.count.cmp(&left.count))
            .then_with(|| left.first_seen.cmp(&right.first_seen))
            .then_with(|| left.command.cmp(&right.command))
    });

    let mut fuzzy = compatible.clone();
    fuzzy.sort_by(|left, right| {
        right
            .fuzzy
            .cmp(&left.fuzzy)
            .then_with(|| right.count.cmp(&left.count))
            .then_with(|| left.first_seen.cmp(&right.first_seen))
            .then_with(|| left.command.cmp(&right.command))
    });

    let mut frequency = compatible.clone();
    frequency.sort_by(|left, right| {
        right
            .count
            .cmp(&left.count)
            .then_with(|| right.fuzzy.cmp(&left.fuzzy))
            .then_with(|| left.first_seen.cmp(&right.first_seen))
            .then_with(|| left.command.cmp(&right.command))
    });

    let prefix_suggestions =
        normalize_suggestions(prefix.into_iter().map(|(item, raw, _)| (item.command, raw)));
    let fuzzy_suggestions =
        normalize_suggestions(fuzzy.into_iter().map(|item| (item.command, item.fuzzy)));
    let frequency_suggestions =
        normalize_suggestions(frequency.into_iter().map(|item| (item.command, item.count)));
    let sequence_suggestions = sequence_suggestions(histories, query, &compatible);

    SuggestionGrid {
        lanes: [
            ModelLane {
                model: ModelKind::Prefix,
                suggestions: prefix_suggestions,
            },
            ModelLane {
                model: ModelKind::Fuzzy,
                suggestions: fuzzy_suggestions,
            },
            ModelLane {
                model: ModelKind::Frequency,
                suggestions: frequency_suggestions,
            },
            ModelLane {
                model: ModelKind::Sequence,
                suggestions: sequence_suggestions,
            },
        ],
        typed_input: query.to_owned(),
    }
}

fn normalize_suggestions(
    ranked: impl IntoIterator<Item = (String, usize)>,
) -> Vec<ScoredSuggestion> {
    let ranked: Vec<_> = ranked.into_iter().take(5).collect();
    let maximum = ranked.first().map_or(1, |(_, raw)| (*raw).max(1));
    ranked
        .into_iter()
        .map(|(command, raw)| ScoredSuggestion {
            command,
            score: ((raw as u128 * 100) / maximum as u128) as u8,
        })
        .collect()
}

fn prefix_word_start(command: &str, query: &str) -> Option<usize> {
    if query.is_empty() {
        return Some(0);
    }
    let command = command.to_lowercase();
    let query = query.to_lowercase();
    command.match_indices(&query).find_map(|(position, _)| {
        (position == 0
            || command[..position]
                .chars()
                .next_back()
                .is_some_and(|character| !character.is_alphanumeric()))
        .then_some(position)
    })
}

fn sequence_suggestions(
    histories: &[Vec<HistoryEntry>],
    query: &str,
    compatible: &[GridAggregate],
) -> Vec<ScoredSuggestion> {
    let Some(previous) = histories.iter().find_map(|history| {
        history
            .iter()
            .rev()
            .map(|entry| entry.command.trim())
            .find(|command| !command.is_empty())
    }) else {
        return Vec::new();
    };

    let mut path_counts: HashMap<Vec<String>, usize> = HashMap::new();
    for history in histories {
        let commands: Vec<_> = history.iter().map(|entry| entry.command.trim()).collect();
        for start in 0..commands.len() {
            if commands[start] != previous {
                continue;
            }
            for depth in 1..=3 {
                let end = start + depth;
                if end >= commands.len() || commands[start..=end].iter().any(|item| item.is_empty())
                {
                    break;
                }
                let path = commands[start..=end]
                    .iter()
                    .map(|item| (*item).to_owned())
                    .collect();
                *path_counts.entry(path).or_default() += 1;
            }
        }
    }

    #[derive(Clone)]
    struct Evidence {
        command: String,
        raw: usize,
        depth: usize,
        fuzzy: usize,
        first_seen: usize,
    }
    let metadata: HashMap<_, _> = compatible
        .iter()
        .map(|item| (item.command.as_str(), (item.fuzzy, item.first_seen)))
        .collect();
    let mut best: HashMap<String, Evidence> = HashMap::new();
    for (path, occurrences) in path_counts {
        if occurrences < 2 {
            continue;
        }
        let depth = path.len() - 1;
        let command = path.last().expect("path has a successor");
        let Some(&(fuzzy, first_seen)) = metadata.get(command.as_str()) else {
            continue;
        };
        if fuzzy_score(command, query).is_none() {
            continue;
        }
        let evidence = Evidence {
            command: command.clone(),
            raw: occurrences * 100 + depth,
            depth,
            fuzzy,
            first_seen,
        };
        let replace = best.get(command).is_none_or(|current| {
            evidence.raw > current.raw
                || (evidence.raw == current.raw && evidence.depth < current.depth)
        });
        if replace {
            best.insert(command.clone(), evidence);
        }
    }
    let mut ranked: Vec<_> = best.into_values().collect();
    ranked.sort_by(|left, right| {
        right
            .raw
            .cmp(&left.raw)
            .then_with(|| left.depth.cmp(&right.depth))
            .then_with(|| right.fuzzy.cmp(&left.fuzzy))
            .then_with(|| left.first_seen.cmp(&right.first_seen))
            .then_with(|| left.command.cmp(&right.command))
    });
    normalize_suggestions(
        ranked
            .into_iter()
            .map(|evidence| (evidence.command, evidence.raw)),
    )
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
    let mut lines = input.lines().filter(|line| !line.is_empty());
    let Some(first) = lines.next() else {
        return false;
    };
    if parse_zsh_extended_marker(first).ok().flatten().is_none() {
        return false;
    }
    // After an initial record marker, any non-marker line is command continuation text.
    // This deliberately mirrors `parse_zsh_extended`, including `: ` command lines.
    true
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
        while index < lines.len()
            && parse_bash_timestamp_marker(lines[index])
                .ok()
                .flatten()
                .is_none()
        {
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
