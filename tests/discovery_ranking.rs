use std::path::PathBuf;

use habits::{
    CandidateKind, HistoryFormat, HistorySource, conventional_history_paths, discover_histories,
    parse_history, rank_candidates,
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

    assert_eq!(discovered.len(), 3);
    assert_eq!(discovered[0].source, HistorySource::Zsh);
    assert_eq!(discovered[1].source, HistorySource::Bash);
    assert_eq!(discovered[2].source, HistorySource::PowerShell);
    assert!(!discovered[0].exists);
    assert_eq!(discovered[1].format, HistoryFormat::BashPlain);
    assert_eq!(discovered[1].entry_count, 1);
    assert!(!discovered[2].exists);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn auto_format_uses_only_consistently_well_formed_timestamp_records() {
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
    assert_eq!(discovered[1].format, HistoryFormat::BashPlain);
    assert_eq!(discovered[1].entry_count, 4);
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
