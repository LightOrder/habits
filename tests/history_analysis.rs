use habits::{
    batches, command_frequencies, load_history_file, parse_history, repeated_sequences, HistoryFormat,
    HistorySource,
};

#[test]
fn parses_extended_zsh_history_and_preserves_source_line_numbers() {
    let entries = parse_history(
        HistoryFormat::ZshExtended,
        ": 1710000000:0;git status\n: 1710000010:12;cargo test\n",
    )
    .expect("extended zsh history should parse");

    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].command, "git status");
    assert_eq!(entries[0].timestamp, Some(1_710_000_000));
    assert_eq!(entries[1].command, "cargo test");
    assert_eq!(entries[1].timestamp, Some(1_710_000_010));
    assert_eq!(entries[1].source, HistorySource::Zsh);
    assert_eq!(entries[1].source_index, 1);
}

#[test]
fn extended_zsh_history_preserves_multiline_commands_until_the_next_marker() {
    let entries = parse_history(
        HistoryFormat::ZshExtended,
        ": 1710000000:0;printf 'one\n two'\n: 1710000010:0;git status\n",
    )
    .expect("extended zsh history should parse");

    assert_eq!(entries[0].command, "printf 'one\n two'");
    assert_eq!(entries[0].timestamp, Some(1_710_000_000));
    assert_eq!(entries[1].command, "git status");
}

#[test]
fn zsh_plain_mode_preserves_extended_history_lookalikes_verbatim() {
    let entries = parse_history(
        HistoryFormat::ZshPlain,
        ": 1710000000:not-a-duration;do not reinterpret me\n",
    )
    .expect("plain zsh history should parse");

    assert_eq!(entries[0].command, ": 1710000000:not-a-duration;do not reinterpret me");
    assert_eq!(entries[0].timestamp, None);
}

#[test]
fn parses_timestamped_bash_multiline_records_and_preserves_start_line() {
    let entries = parse_history(
        HistoryFormat::BashTimestamped,
        "#1710000000\nprintf 'one\\ntwo'\n#1710000010\ncargo test\n",
    )
    .expect("timestamped bash history should parse");

    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].command, "printf 'one\\ntwo'");
    assert_eq!(entries[0].timestamp, Some(1_710_000_000));
    assert_eq!(entries[0].source_index, 1);
    assert_eq!(entries[1].command, "cargo test");
}

#[test]
fn bash_plain_mode_never_discards_numeric_comment_commands() {
    let entries = parse_history(HistoryFormat::BashPlain, "#1710000000\ngit status\n")
        .expect("plain bash history should parse");

    assert_eq!(entries[0].command, "#1710000000");
    assert_eq!(entries[0].timestamp, None);
    assert_eq!(entries[1].command, "git status");
}

#[test]
fn powershell_lines_have_no_implied_timestamps() {
    let entries = parse_history(
        HistoryFormat::PowerShell,
        "Get-ChildItem\ngit status\n",
    )
    .expect("PowerShell history should parse");

    assert!(entries.iter().all(|entry| entry.timestamp.is_none()));
    assert_eq!(entries[0].command, "Get-ChildItem");
}

#[test]
fn frequency_counts_trimmed_exact_commands_without_rewriting_command_syntax() {
    let entries = parse_history(
        HistoryFormat::ZshPlain,
        " git status \ngit status\ngit   status\n\n",
    )
    .expect("history should parse");

    let frequencies = command_frequencies(&entries);

    assert_eq!(frequencies[0].command, "git status");
    assert_eq!(frequencies[0].count, 2);
    assert!(frequencies.iter().any(|item| item.command == "git   status" && item.count == 1));
}

#[test]
fn batching_requires_contiguous_monotonic_timestamped_entries_inside_the_gap() {
    let entries = parse_history(
        HistoryFormat::ZshExtended,
        ": 100:0;git status\n: 140:0;cargo test\n: 300:0;git diff\n: 150:0;git push\n: 1000:0;git fetch\n",
    )
    .expect("history should parse");

    let batches = batches(&entries, 60);

    assert_eq!(batches.len(), 4);
    assert_eq!(batches[0].commands(), vec!["git status", "cargo test"]);
    assert_eq!(batches[1].commands(), vec!["git diff"]);
    assert_eq!(batches[2].commands(), vec!["git push"]);
    assert_eq!(batches[3].commands(), vec!["git fetch"]);
}

#[test]
fn batching_handles_timestamp_extremes_without_overflow() {
    let entries = parse_history(
        HistoryFormat::ZshExtended,
        ": 9223372036854775807:0;latest\n: 0:0;earliest\n",
    )
    .expect("history should parse");

    assert_eq!(batches(&entries, 60).len(), 2);
}

#[test]
fn repeated_sequences_favor_longer_contiguous_patterns_over_single_command_frequency() {
    let entries = parse_history(
        HistoryFormat::ZshExtended,
        ": 100:0;git status\n: 110:0;cargo test\n: 120:0;git diff\n: 500:0;git status\n: 510:0;cargo test\n: 520:0;git diff\n",
    )
    .expect("history should parse");
    let batches = batches(&entries, 60);

    let patterns = repeated_sequences(&batches, 2, 3, 2);

    assert_eq!(patterns[0].commands, vec!["git status", "cargo test", "git diff"]);
    assert_eq!(patterns[0].occurrences, 2);
    assert!(patterns[0].rank > patterns[1].rank);
}

#[test]
fn loads_and_parses_an_explicit_history_file() {
    let path = std::env::temp_dir().join(format!("habits-history-{}.txt", std::process::id()));
    std::fs::write(&path, ": 1710000000:0;git status\n").expect("fixture should write");

    let entries = load_history_file(&path, HistoryFormat::ZshExtended).expect("file should load");

    std::fs::remove_file(path).expect("fixture should clean up");
    assert_eq!(entries[0].command, "git status");
    assert_eq!(entries[0].timestamp, Some(1_710_000_000));
}
