use std::path::PathBuf;

use habits::{
    CandidateKind, HistoryFormat, HistorySource, ModelKind, conventional_history_paths,
    discover_histories, parse_history, rank_candidates, suggestion_grid,
};

fn fixture_root(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "habits-{label}-{}-{}",
        std::process::id(),
        std::thread::current().name().unwrap_or("test")
    ))
}

fn entry(command: &str) -> habits::HistoryEntry {
    habits::HistoryEntry {
        command: command.to_owned(),
        timestamp: None,
        source: HistorySource::Zsh,
        source_index: 0,
    }
}

#[test]
fn conventional_discovery_is_stably_ordered_and_reports_missing_files() {
    let root = fixture_root("discovery-order");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join(".bash_history"), "cargo test\n").unwrap();

    let paths = conventional_history_paths(&root).unwrap();
    let discovered = discover_histories(&paths);

    assert_eq!(discovered.len(), 5);
    assert_eq!(discovered[0].source, HistorySource::Zsh);
    assert_eq!(discovered[1].source, HistorySource::Bash);
    assert_eq!(discovered[2].source, HistorySource::PowerShell);
    assert_eq!(discovered[3].source, HistorySource::PowerShell);
    assert_eq!(discovered[4].source, HistorySource::PowerShell);
    assert!(!discovered[0].exists);
    assert_eq!(discovered[1].format, HistoryFormat::BashPlain);
    assert_eq!(discovered[1].entry_count, 1);
    assert!(!discovered[2].exists);
    assert!(!discovered[3].exists);
    assert!(!discovered[4].exists);
    assert!(discovered[3].path.ends_with(
        "AppData/Roaming/Microsoft/Windows/PowerShell/PSReadLine/ConsoleHost_history.txt"
    ));
    assert!(
        discovered[4]
            .path
            .ends_with("AppData/Roaming/PowerShell/PSReadLine/ConsoleHost_history.txt")
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn auto_format_uses_timestamped_records_without_misreading_command_text() {
    let root = fixture_root("format");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join(".zsh_history"),
        ": 100:0;git status\n: 110:4;cargo test\n",
    )
    .unwrap();
    std::fs::write(
        root.join(".bash_history"),
        "#100\ncargo test\n#not-a-time\ngit status\n",
    )
    .unwrap();

    let discovered = discover_histories(&conventional_history_paths(&root).unwrap());

    assert_eq!(discovered[0].format, HistoryFormat::ZshExtended);
    assert_eq!(discovered[0].entry_count, 2);
    assert_eq!(discovered[1].format, HistoryFormat::BashTimestamped);
    assert_eq!(discovered[1].entry_count, 1);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn auto_format_keeps_marker_like_command_lines_inside_timestamped_records() {
    let root = fixture_root("marker-like-commands");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join(".zsh_history"),
        ": 100:0;printf first\n: echo continuation\n: 110:0;cargo test\n",
    )
    .unwrap();
    std::fs::write(
        root.join(".bash_history"),
        "#100\nprintf first\n# comment\n#110\ncargo test\n",
    )
    .unwrap();

    let discovered = discover_histories(&conventional_history_paths(&root).unwrap());

    assert_eq!(discovered[0].format, HistoryFormat::ZshExtended);
    assert_eq!(discovered[0].entry_count, 2);
    assert_eq!(discovered[1].format, HistoryFormat::BashTimestamped);
    assert_eq!(discovered[1].entry_count, 2);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn ranking_keeps_typed_input_and_fuzzy_narrows_case_insensitively() {
    let histories = vec![vec![
        entry("git status"),
        entry("cargo test"),
        entry("Git stash"),
    ]];
    let ranked = rank_candidates(&histories, "gis");

    assert_eq!(ranked[0].kind, CandidateKind::TypedInput);
    assert_eq!(ranked[0].command, "gis");
    assert_eq!(
        ranked
            .iter()
            .filter(|candidate| candidate.kind == CandidateKind::History)
            .map(|candidate| candidate.command.as_str())
            .collect::<Vec<_>>(),
        vec!["git status", "Git stash"]
    );
}

#[test]
fn ranking_prefers_frequency_then_stable_first_observation() {
    let histories = vec![
        vec![entry("first tie")],
        vec![entry("popular"), entry("popular")],
        vec![entry("second tie")],
    ];
    let ranked = rank_candidates(&histories, "");

    assert_eq!(
        ranked
            .iter()
            .skip(1)
            .map(|candidate| candidate.command.as_str())
            .collect::<Vec<_>>(),
        vec!["popular", "first tie", "second tie"]
    );
}

#[test]
fn ranking_exposes_predecessor_and_successor_evidence_without_crossing_sources() {
    let histories = vec![
        vec![entry("git status"), entry("cargo test"), entry("git push")],
        vec![entry("npm test"), entry("cargo test"), entry("git push")],
    ];
    let ranked = rank_candidates(&histories, "cargo");
    let cargo = ranked
        .iter()
        .find(|candidate| candidate.command == "cargo test")
        .unwrap();

    assert_eq!(cargo.predecessors[0].command, "git status");
    assert_eq!(cargo.predecessors[0].count, 1);
    assert_eq!(cargo.predecessors[1].command, "npm test");
    assert_eq!(cargo.successors[0].command, "git push");
    assert_eq!(cargo.successors[0].count, 2);
}

#[test]
fn suggestion_grid_has_four_stable_model_lanes_and_preserves_typed_input() {
    let grid = suggestion_grid(&[vec![entry("git status")]], "git");

    assert_eq!(grid.typed_input, "git");
    assert_eq!(grid.lanes.len(), 4);
    assert_eq!(
        grid.lanes.map(|lane| lane.model),
        [
            ModelKind::Prefix,
            ModelKind::Fuzzy,
            ModelKind::Frequency,
            ModelKind::Sequence,
        ]
    );
}

#[test]
fn suggestion_lanes_cap_at_five_and_use_bounded_model_local_scores() {
    let histories = vec![vec![
        entry("git alpha"),
        entry("git bravo"),
        entry("git charlie"),
        entry("git delta"),
        entry("git echo"),
        entry("git foxtrot"),
    ]];
    let grid = suggestion_grid(&histories, "git");

    for lane in &grid.lanes[..3] {
        assert_eq!(lane.suggestions.len(), 5);
        assert!(lane.suggestions.iter().all(|item| item.score <= 100));
        assert_eq!(lane.suggestions[0].score, 100);
    }
    assert!(grid.lanes[3].suggestions.is_empty());
}

#[test]
fn prefix_and_fuzzy_lanes_have_distinct_deterministic_ordering() {
    let late_prefix = format!("{} gts", "x".repeat(500));
    let histories = vec![vec![entry(&late_prefix), entry("git status")]];
    let grid = suggestion_grid(&histories, "gts");

    assert_eq!(grid.lanes[0].suggestions[0].command, late_prefix);
    assert_eq!(grid.lanes[1].suggestions[0].command, "git status");
}

#[test]
fn frequency_lane_favors_most_frequent_query_compatible_command() {
    let histories = vec![vec![
        entry("git status"),
        entry("git stash"),
        entry("git stash"),
        entry("git stash"),
    ]];
    let grid = suggestion_grid(&histories, "gis");

    assert_eq!(grid.lanes[2].suggestions[0].command, "git stash");
    assert_eq!(grid.lanes[2].suggestions[0].score, 100);
}

#[test]
fn repeated_prior_paths_offer_each_depth_but_single_paths_do_not() {
    let histories = vec![vec![
        entry("previous"),
        entry("one"),
        entry("two"),
        entry("three"),
        entry("previous"),
        entry("one"),
        entry("two"),
        entry("three"),
        entry("previous"),
        entry("single"),
        entry("previous"),
    ]];
    let grid = suggestion_grid(&histories, "");
    let sequence: Vec<_> = grid.lanes[3]
        .suggestions
        .iter()
        .map(|item| item.command.as_str())
        .collect();

    assert!(sequence.contains(&"one"));
    assert!(sequence.contains(&"two"));
    assert!(sequence.contains(&"three"));
    assert!(!sequence.contains(&"single"));
}

#[test]
fn sequence_backs_off_to_global_repeated_transitions_when_latest_context_has_no_path() {
    let histories = vec![vec![
        entry("alpha"),
        entry("beta"),
        entry("alpha"),
        entry("beta"),
        entry("latest"),
    ]];
    let grid = suggestion_grid(&histories, "");
    let sequence: Vec<_> = grid.lanes[3]
        .suggestions
        .iter()
        .map(|item| item.command.as_str())
        .collect();

    assert!(sequence.contains(&"beta"));
}

#[test]
fn sequence_keeps_contextual_predictions_when_query_has_no_path_match() {
    let histories = vec![vec![
        entry("alpha"),
        entry("beta"),
        entry("alpha"),
        entry("beta"),
        entry("latest"),
    ]];
    let grid = suggestion_grid(&histories, "git");

    assert_eq!(grid.lanes[3].suggestions[0].command, "beta");
}

#[test]
fn sequence_paths_never_cross_history_vector_boundaries() {
    let histories = vec![
        vec![entry("previous"), entry("cross"), entry("previous")],
        vec![entry("cross"), entry("previous")],
    ];
    let grid = suggestion_grid(&histories, "cross");

    assert!(
        !grid.lanes[3]
            .suggestions
            .iter()
            .any(|suggestion| suggestion.command == "cross")
    );
}

#[test]
fn lane_ties_are_stable_and_commands_are_deduplicated() {
    let histories = vec![vec![
        entry("alpha"),
        entry("beta"),
        entry("alpha"),
        entry("beta"),
        entry("prior"),
    ]];
    let grid = suggestion_grid(&histories, "");

    assert_eq!(grid.lanes[2].suggestions[0].command, "alpha");
    for lane in &grid.lanes {
        let commands: Vec<_> = lane
            .suggestions
            .iter()
            .map(|item| item.command.as_str())
            .collect();
        let unique: std::collections::HashSet<_> = commands.iter().copied().collect();
        assert_eq!(commands.len(), unique.len());
    }
}

#[test]
fn prefix_ties_use_word_start_then_frequency_then_first_observation() {
    let histories = vec![vec![
        entry("git first"),
        entry("git popular"),
        entry("run git frequent"),
        entry("git second"),
        entry("git popular"),
        entry("run git frequent"),
        entry("run git frequent"),
    ]];
    let grid = suggestion_grid(&histories, "git");
    let prefix: Vec<_> = grid.lanes[0]
        .suggestions
        .iter()
        .map(|item| item.command.as_str())
        .collect();

    assert_eq!(
        prefix,
        vec!["git popular", "git first", "git second", "run git frequent"]
    );
}

#[test]
fn fuzzy_ties_use_frequency_then_first_observation_and_deduplicate() {
    let histories = vec![vec![
        entry("git status"),
        entry("git stash"),
        entry("git staged"),
        entry("git stash"),
    ]];
    let grid = suggestion_grid(&histories, "gis");
    let fuzzy: Vec<_> = grid.lanes[1]
        .suggestions
        .iter()
        .map(|item| item.command.as_str())
        .collect();

    assert_eq!(fuzzy, vec!["git stash", "git status", "git staged"]);
}

#[test]
fn sequence_ties_use_first_observation_and_deduplicate_repeated_paths() {
    let histories = vec![vec![
        entry("prior"),
        entry("alpha"),
        entry("unique one"),
        entry("unique two"),
        entry("prior"),
        entry("beta"),
        entry("unique three"),
        entry("unique four"),
        entry("prior"),
        entry("alpha"),
        entry("unique five"),
        entry("unique six"),
        entry("prior"),
        entry("beta"),
        entry("unique seven"),
        entry("unique eight"),
        entry("prior"),
    ]];
    let grid = suggestion_grid(&histories, "");
    let sequence: Vec<_> = grid.lanes[3]
        .suggestions
        .iter()
        .map(|item| item.command.as_str())
        .collect();

    assert_eq!(sequence, vec!["alpha", "beta"]);
}

#[test]
fn parsing_fixtures_cover_all_discovered_shell_families() {
    assert_eq!(
        parse_history(HistoryFormat::ZshPlain, "git status\n")
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        parse_history(HistoryFormat::BashTimestamped, "#100\ngit status\n").unwrap()[0].timestamp,
        Some(100)
    );
    assert_eq!(
        parse_history(HistoryFormat::PowerShell, "Get-ChildItem\n")
            .unwrap()
            .len(),
        1
    );
}
