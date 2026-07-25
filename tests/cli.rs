use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);
const SENTINEL: &str = "printf 'HABITS_PRIVATE_SENTINEL_7f93'";

struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let serial = NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed);
        let root =
            std::env::temp_dir().join(format!("habits-cli-test-{}-{serial}", std::process::id()));
        std::fs::create_dir_all(&root).expect("temporary fixture directory should be created");
        Self { root }
    }

    fn path(&self, relative: &str) -> PathBuf {
        self.root.join(relative)
    }

    fn write(&self, relative: &str, contents: &[u8]) -> PathBuf {
        let path = self.path(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("fixture parent should be created");
        }
        std::fs::write(&path, contents).expect("fixture should be written");
        path
    }

    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_habits"));
        command.env("HOME", &self.root);
        command
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn inspect_fixture(fixture: &Fixture) -> PathBuf {
    fixture.write(
        "history",
        format!(": 100:0;{SENTINEL}\n: 110:0;cargo test\n: 500:0;{SENTINEL}\n: 510:0;cargo test\n")
            .as_bytes(),
    )
}

fn run(command: &mut Command) -> Output {
    command.output().expect("habits binary should execute")
}

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("stdout should be UTF-8")
}

fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("stderr should be UTF-8")
}

#[test]
fn safe_human_inspect_reports_aggregates_without_history_content() {
    let fixture = Fixture::new();
    let path = inspect_fixture(&fixture);
    let output = run(fixture.command().args([
        "inspect",
        "--format",
        "zsh-extended",
        "--path",
        path.to_str().unwrap(),
        "--gap-seconds",
        "60",
        "--top",
        "10",
    ]));

    assert!(output.status.success(), "{}", stderr(&output));
    let report = stdout(&output);
    assert!(report.contains("zsh-extended"));
    assert!(report.contains("entries: 4"));
    assert!(report.contains("timestamped: 4"));
    assert!(report.contains("batches"));
    assert!(report.contains("2"));
    assert!(report.contains("top"));
    assert!(report.contains("repeated"));
    assert!(!report.contains(SENTINEL));
    assert!(!report.contains("cargo test"));
    assert!(!stderr(&output).contains(SENTINEL));
}

#[test]
fn show_commands_is_an_explicit_opt_in_with_a_stderr_warning() {
    let fixture = Fixture::new();
    let path = inspect_fixture(&fixture);
    let output = run(fixture.command().args([
        "inspect",
        "--format",
        "zsh-extended",
        "--path",
        path.to_str().unwrap(),
        "--gap-seconds",
        "60",
        "--show-commands",
    ]));

    assert!(output.status.success(), "{}", stderr(&output));
    assert!(stdout(&output).contains(SENTINEL));
    assert!(stdout(&output).contains(&format!("{SENTINEL} → cargo test")));
    assert!(stderr(&output).to_ascii_lowercase().contains("warning"));
    assert!(!stderr(&output).contains(SENTINEL));
}

#[test]
fn aggregate_json_is_valid_deterministic_and_omits_command_fields() {
    let fixture = Fixture::new();
    let path = inspect_fixture(&fixture);
    let invoke = || {
        run(fixture.command().args([
            "inspect",
            "--format",
            "zsh-extended",
            "--path",
            path.to_str().unwrap(),
            "--gap-seconds",
            "60",
            "--json",
        ]))
    };
    let first = invoke();
    let second = invoke();

    assert!(first.status.success(), "{}", stderr(&first));
    assert_eq!(first.stdout, second.stdout);
    let report = stdout(&first);
    let parsed = JsonParser::parse(&report).expect("inspect output should be valid JSON");
    let object = parsed.object();
    assert_eq!(object["format"].string(), "zsh-extended");
    assert_eq!(object["entry_count"].number(), 4);
    assert_eq!(object["timestamped_count"].number(), 4);
    assert_eq!(object["batch_count"].number(), 2);
    assert!(!report.contains(SENTINEL));
    assert!(!report.contains("cargo test"));
    assert!(!report.contains("\"command\""));
    assert!(!report.contains("\"commands\""));
}

#[test]
fn json_show_commands_emits_only_opted_in_command_fields_and_valid_escapes() {
    let fixture = Fixture::new();
    let escaped = "printf 'quote=\" slash=\\\\ tab=\t'";
    let path = fixture.write(
        "history",
        format!(": 100:0;{escaped}\n: 110:0;next\n: 120:0;{escaped}\n: 130:0;next\n").as_bytes(),
    );
    let output = run(fixture.command().args([
        "inspect",
        "--format",
        "zsh-extended",
        "--path",
        path.to_str().unwrap(),
        "--json",
        "--show-commands",
    ]));

    assert!(output.status.success(), "{}", stderr(&output));
    assert!(stderr(&output).to_ascii_lowercase().contains("warning"));
    let report = stdout(&output);
    let parsed = JsonParser::parse(&report).expect("opt-in output should be valid JSON");
    let object = parsed.object();
    let frequencies = object["top_frequencies"].array();
    assert!(frequencies.iter().any(|row| {
        row.object()
            .get("command")
            .is_some_and(|command| command.string() == escaped)
    }));
    let sequences = object["repeated_sequences"].array();
    assert!(
        sequences
            .iter()
            .any(|row| row.object().contains_key("commands"))
    );
}

#[test]
fn paths_json_reports_only_known_candidates_without_reading_them() {
    let fixture = Fixture::new();
    fixture.write(".zsh_history", &[0xff, 0xfe, b'Z']);
    fixture.write(".bash_history", b"HABITS_PRIVATE_SENTINEL_7f93");
    let output = run(fixture.command().args(["paths", "--json"]));

    assert!(output.status.success(), "{}", stderr(&output));
    let report = stdout(&output);
    let parsed = JsonParser::parse(&report).expect("paths output should be valid JSON");
    let candidates = parsed.object()["candidates"].array();
    assert_eq!(candidates.len(), 3);
    let expected = [
        (fixture.path(".zsh_history"), true, "zsh-plain"),
        (fixture.path(".bash_history"), true, "bash-plain"),
        (
            fixture.path(".local/share/powershell/PSReadLine/ConsoleHost_history.txt"),
            false,
            "powershell",
        ),
    ];
    for (candidate, (path, exists, format)) in candidates.iter().zip(expected) {
        let candidate = candidate.object();
        assert_eq!(candidate["path"].string(), path.to_str().unwrap());
        assert_eq!(candidate["exists"].boolean(), exists);
        assert_eq!(candidate["format"].string(), format);
        assert!(candidate.contains_key("source"));
        assert!(candidate.contains_key("format_selection"));
        assert!(candidate.contains_key("entry_count"));
    }
    assert!(!report.contains("HABITS_PRIVATE_SENTINEL_7f93"));
}

#[test]
fn paths_manual_override_replaces_automatic_discovery() {
    let fixture = Fixture::new();
    let path = fixture.write("chosen-history", b"#100\ncargo test\n");
    let output = run(fixture.command().args([
        "paths",
        "--path",
        path.to_str().unwrap(),
        "--format",
        "bash-timestamped",
        "--json",
    ]));

    assert!(output.status.success(), "{}", stderr(&output));
    let report = stdout(&output);
    let parsed = JsonParser::parse(&report).expect("paths output should be valid JSON");
    let candidates = parsed.object()["candidates"].array();
    assert_eq!(candidates.len(), 1);
    let candidate = candidates[0].object();
    assert_eq!(candidate["path"].string(), path.to_str().unwrap());
    assert_eq!(candidate["format"].string(), "bash-timestamped");
    assert_eq!(candidate["format_selection"].string(), "manual override");
    assert_eq!(candidate["entry_count"].number(), 1);
    assert!(!report.contains("cargo test"));
}

#[test]
fn paths_rejects_a_non_absolute_home_without_statting_the_cwd() {
    let fixture = Fixture::new();
    let mut command = fixture.command();
    command.env("HOME", "relative-home");
    let output = run(command.args(["paths", "--json"]));

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(stderr(&output).to_ascii_lowercase().contains("home"));
}

#[test]
fn all_explicit_history_formats_are_accepted() {
    let cases = [
        ("bash-plain", b"echo ok\n".as_slice()),
        ("bash-timestamped", b"#100\necho ok\n".as_slice()),
        ("zsh-plain", b"echo ok\n".as_slice()),
        ("zsh-extended", b": 100:0;echo ok\n".as_slice()),
        ("powershell", b"Get-ChildItem\n".as_slice()),
    ];

    for (format, contents) in cases {
        let fixture = Fixture::new();
        let path = fixture.write("history", contents);
        let output = run(fixture.command().args([
            "inspect",
            "--format",
            format,
            "--path",
            path.to_str().unwrap(),
        ]));
        assert!(
            output.status.success(),
            "{format} failed: {}",
            stderr(&output)
        );
        assert!(stdout(&output).contains(format));
    }
}

#[test]
fn inspect_uses_the_documented_gap_and_top_defaults() {
    let fixture = Fixture::new();
    let mut contents = String::new();
    for index in 0..11 {
        contents.push_str(&format!(": {}:0;command-{index}\n", index * 300));
    }
    contents.push_str(": 3301:0;final-command\n");
    let path = fixture.write("history", contents.as_bytes());
    let output = run(fixture.command().args([
        "inspect",
        "--format",
        "zsh-extended",
        "--path",
        path.to_str().unwrap(),
        "--json",
    ]));

    assert!(output.status.success(), "{}", stderr(&output));
    let parsed = JsonParser::parse(&stdout(&output)).expect("default report should be valid JSON");
    let object = parsed.object();
    assert_eq!(object["batch_count"].number(), 2);
    assert_eq!(object["top_frequencies"].array().len(), 10);
}

#[test]
fn invalid_inspect_arguments_fail_with_usage_and_no_report() {
    let fixture = Fixture::new();
    let path = fixture.write("history", SENTINEL.as_bytes());
    let path = path.to_str().unwrap();
    let cases: &[&[&str]] = &[
        &["inspect", "--path", path],
        &["inspect", "--format", "not-a-format", "--path", path],
        &["inspect", "--format", "zsh-plain"],
        &[
            "inspect",
            "--format",
            "zsh-plain",
            "--path",
            path,
            "--gap-seconds",
            "later",
        ],
        &[
            "inspect",
            "--format",
            "zsh-plain",
            "--path",
            path,
            "--top",
            "many",
        ],
    ];

    for arguments in cases {
        let output = run(fixture.command().args(*arguments));
        assert!(!output.status.success(), "{arguments:?}");
        assert!(output.stdout.is_empty(), "{arguments:?}");
        assert!(stderr(&output).to_ascii_lowercase().contains("error"));
        assert!(stderr(&output).to_ascii_lowercase().contains("usage"));
        assert!(!stderr(&output).contains(SENTINEL));
    }
}

#[test]
fn missing_file_failure_identifies_only_the_requested_path() {
    let fixture = Fixture::new();
    let path = fixture.path("missing-history");
    let output = run(fixture.command().args([
        "inspect",
        "--format",
        "zsh-plain",
        "--path",
        path.to_str().unwrap(),
    ]));

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(stderr(&output).contains(path.to_str().unwrap()));
    assert!(!stderr(&output).contains(SENTINEL));
}

#[test]
fn malformed_history_errors_do_not_echo_history_content() {
    let fixture = Fixture::new();
    let secret = "999999999999999999999999999999999999";
    let path = fixture.write("history", format!("#{secret}\n{SENTINEL}\n").as_bytes());
    let output = run(fixture.command().args([
        "inspect",
        "--format",
        "bash-timestamped",
        "--path",
        path.to_str().unwrap(),
    ]));

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(stderr(&output).contains(path.to_str().unwrap()));
    assert!(!stderr(&output).contains(secret));
    assert!(!stderr(&output).contains(SENTINEL));
}

#[test]
fn help_and_unknown_subcommand_have_the_required_statuses() {
    let fixture = Fixture::new();
    for arguments in [&["--help"][..], &["help"][..]] {
        let output = run(fixture.command().args(arguments));
        assert!(output.status.success(), "{}", stderr(&output));
        assert!(stdout(&output).to_ascii_lowercase().contains("usage"));
    }

    let output = run(fixture.command().arg("mystery"));
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(stderr(&output).to_ascii_lowercase().contains("usage"));
}

#[test]
fn version_matches_the_cargo_package_version() {
    let fixture = Fixture::new();
    let output = run(fixture.command().arg("--version"));

    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(
        stdout(&output),
        format!("habits {}\n", env!("CARGO_PKG_VERSION"))
    );
    assert!(output.stderr.is_empty());
}

#[derive(Debug)]
enum Json {
    Null,
    Bool(bool),
    Number(i64),
    String(String),
    Array(Vec<Json>),
    Object(BTreeMap<String, Json>),
}

impl Json {
    fn object(&self) -> &BTreeMap<String, Json> {
        match self {
            Self::Object(value) => value,
            _ => panic!("expected JSON object"),
        }
    }

    fn string(&self) -> &str {
        match self {
            Self::String(value) => value,
            _ => panic!("expected JSON string"),
        }
    }

    fn number(&self) -> i64 {
        match self {
            Self::Number(value) => *value,
            _ => panic!("expected JSON number"),
        }
    }

    fn boolean(&self) -> bool {
        match self {
            Self::Bool(value) => *value,
            _ => panic!("expected JSON boolean"),
        }
    }

    fn array(&self) -> &[Json] {
        match self {
            Self::Array(value) => value,
            _ => panic!("expected JSON array"),
        }
    }
}

struct JsonParser<'a> {
    input: &'a [u8],
    position: usize,
}

impl<'a> JsonParser<'a> {
    fn parse(input: &'a str) -> Result<Json, String> {
        let mut parser = Self {
            input: input.as_bytes(),
            position: 0,
        };
        let value = parser.value()?;
        parser.whitespace();
        if parser.position != parser.input.len() {
            return Err("trailing data after JSON value".into());
        }
        Ok(value)
    }

    fn value(&mut self) -> Result<Json, String> {
        self.whitespace();
        match self.peek() {
            Some(b'{') => self.object_value(),
            Some(b'[') => self.array_value(),
            Some(b'"') => self.string_value().map(Json::String),
            Some(b'-' | b'0'..=b'9') => self.number_value(),
            Some(b't') => self.literal(b"true", Json::Bool(true)),
            Some(b'f') => self.literal(b"false", Json::Bool(false)),
            Some(b'n') => self.literal(b"null", Json::Null),
            _ => Err("expected JSON value".into()),
        }
    }

    fn object_value(&mut self) -> Result<Json, String> {
        self.expect(b'{')?;
        let mut values = BTreeMap::new();
        self.whitespace();
        if self.take(b'}') {
            return Ok(Json::Object(values));
        }
        loop {
            self.whitespace();
            let key = self.string_value()?;
            self.whitespace();
            self.expect(b':')?;
            values.insert(key, self.value()?);
            self.whitespace();
            if self.take(b'}') {
                return Ok(Json::Object(values));
            }
            self.expect(b',')?;
        }
    }

    fn array_value(&mut self) -> Result<Json, String> {
        self.expect(b'[')?;
        self.whitespace();
        if self.take(b']') {
            return Ok(Json::Array(Vec::new()));
        }
        let mut values = Vec::new();
        loop {
            values.push(self.value()?);
            self.whitespace();
            if self.take(b']') {
                return Ok(Json::Array(values));
            }
            self.expect(b',')?;
        }
    }

    fn string_value(&mut self) -> Result<String, String> {
        self.expect(b'"')?;
        let mut value = String::new();
        loop {
            let byte = self.next().ok_or("unterminated JSON string")?;
            match byte {
                b'"' => return Ok(value),
                b'\\' => match self.next().ok_or("unterminated JSON escape")? {
                    b'"' => value.push('"'),
                    b'\\' => value.push('\\'),
                    b'/' => value.push('/'),
                    b'b' => value.push('\u{0008}'),
                    b'f' => value.push('\u{000c}'),
                    b'n' => value.push('\n'),
                    b'r' => value.push('\r'),
                    b't' => value.push('\t'),
                    b'u' => {
                        for _ in 0..4 {
                            if !self.next().is_some_and(|digit| digit.is_ascii_hexdigit()) {
                                return Err("invalid JSON unicode escape".into());
                            }
                        }
                    }
                    _ => return Err("invalid JSON escape".into()),
                },
                0x00..=0x1f => return Err("unescaped JSON control character".into()),
                ascii if ascii.is_ascii() => value.push(ascii as char),
                _ => {
                    let start = self.position - 1;
                    let tail = std::str::from_utf8(&self.input[start..])
                        .map_err(|_| "invalid UTF-8 in JSON string")?;
                    let character = tail.chars().next().unwrap();
                    value.push(character);
                    self.position = start + character.len_utf8();
                }
            }
        }
    }

    fn number_value(&mut self) -> Result<Json, String> {
        let start = self.position;
        self.take(b'-');
        while self.peek().is_some_and(|byte| byte.is_ascii_digit()) {
            self.position += 1;
        }
        let raw = std::str::from_utf8(&self.input[start..self.position]).unwrap();
        raw.parse::<i64>()
            .map(Json::Number)
            .map_err(|_| "invalid JSON integer".into())
    }

    fn literal(&mut self, literal: &[u8], value: Json) -> Result<Json, String> {
        if self.input.get(self.position..self.position + literal.len()) == Some(literal) {
            self.position += literal.len();
            Ok(value)
        } else {
            Err("invalid JSON literal".into())
        }
    }

    fn whitespace(&mut self) {
        while self.peek().is_some_and(|byte| byte.is_ascii_whitespace()) {
            self.position += 1;
        }
    }

    fn expect(&mut self, byte: u8) -> Result<(), String> {
        if self.take(byte) {
            Ok(())
        } else {
            Err(format!("expected byte {byte}"))
        }
    }

    fn take(&mut self, byte: u8) -> bool {
        if self.peek() == Some(byte) {
            self.position += 1;
            true
        } else {
            false
        }
    }

    fn peek(&self) -> Option<u8> {
        self.input.get(self.position).copied()
    }

    fn next(&mut self) -> Option<u8> {
        let byte = self.peek()?;
        self.position += 1;
        Some(byte)
    }
}
