# Habits v0 CLI Completion Plan

> **For Codex:** Use multiple subagents before and after implementation: a lead implementer, a CLI-contract/test reviewer, and a privacy/security reviewer. Use the strongest available coding model for implementation and reviews. If native subagents are unavailable, stop and report that blocker rather than claiming multi-agent work occurred.

**Goal:** Finish Habits v0 as a safe, local-only Rust command-line utility that inspects an explicitly selected shell-history file and reports command frequency, timestamp coverage, batches, and repeated-sequence candidates.

**Architecture:** Keep all deterministic parsing and analysis in `src/lib.rs`. Add a thin `std`-only binary adapter at `src/main.rs`; it parses arguments, calls the existing library, and renders human or machine-readable aggregate reports. No database, daemon, shell hook, model, configuration mutation, or remote service belongs in this phase.

**Tech stack:** Rust 2024, standard library only, Cargo integration tests. Do **not** add dependencies. If any dependency seems necessary, stop and explain why rather than installing it.

---

## Product and privacy rules

1. All history paths are explicit user input. `habits` must not scan arbitrary directories.
2. History formats are explicit: `bash-plain`, `bash-timestamped`, `zsh-plain`, `zsh-extended`, `powershell`. Never auto-detect a format from command text.
3. Default output must never disclose commands, sequences, arguments, paths, or other history content.
4. Raw commands may appear only when the user supplies `--show-commands`; print a clear warning to stderr before emitting them.
5. `--json` is aggregate-only unless combined with `--show-commands`.
6. CLI reads files only. It must not write shell configuration, history files, databases, caches, or network data.
7. Preserve the existing parser and analysis behavior; do not regress multiline timestamped history, source positions, or timestamp boundary rules.

## Public CLI contract

### `habits inspect`

```text
habits inspect --format <FORMAT> --path <PATH> [--gap-seconds <SECONDS>] [--top <N>] [--json] [--show-commands]
```

- `--format` and `--path` are required.
- Supported formats are exactly those in the privacy rules.
- Defaults: `--gap-seconds 300`, `--top 10`.
- Invalid/missing values exit non-zero, print a concise error plus usage to stderr, and emit no report.
- File-read/parse failures exit non-zero and identify the requested path without exposing file contents.
- Human output always includes: selected format, entry count, timestamped count, batch count, top-frequency rows, and repeated-sequence rows.
- Without `--show-commands`, top-frequency rows show counts only; sequence rows show rank, occurrence count, and length only.
- With `--show-commands`, top-frequency rows include the command and sequence rows include commands joined by ` → `.
- JSON is deterministic and valid. It includes only aggregate fields by default; command fields appear only with `--show-commands`.

### `habits paths`

```text
habits paths [--json]
```

Report only conventional history locations, whether each exact path exists, and the format a user should explicitly select. Supported candidates:

- Zsh: `~/.zsh_history` → `zsh-plain` (user chooses `zsh-extended` after enabling it)
- Bash: `~/.bash_history` → `bash-plain` (user chooses `bash-timestamped` after enabling it)
- PowerShell / PSReadLine: `~/.local/share/powershell/PSReadLine/ConsoleHost_history.txt` → `powershell`

This command may stat these exact conventional paths but must not read their contents. It must never claim that the suggested format was detected.

### Help/version

- `habits --help`, `habits help`, and unknown commands show concise usage and non-zero only for unknown commands.
- `habits --version` prints the package version.

## Implementation tasks

### Task 1: Establish CLI contract tests

**Files:**
- Create: `tests/cli.rs`

Write integration tests that execute the compiled binary through `CARGO_BIN_EXE_habits` with temporary fixture histories. Cover:

- successful safe human `inspect`;
- safe output does not contain a sentinel command or sequence;
- `--show-commands` emits the sentinel and warning;
- valid aggregate JSON parses and omits command fields by default;
- `paths --json` emits known candidates without reading their content;
- missing `--format`, invalid format, invalid numeric option, missing file, and unknown subcommand fail clearly;
- `--version` matches `Cargo.toml`.

Run the new test target red before implementation.

### Task 2: Add a dependency-free binary adapter

**Files:**
- Create: `src/main.rs`
- Modify only if needed: `src/lib.rs`, `Cargo.toml`

Implement argument parsing and report rendering with `std::env`, `std::fs`, and existing public library functions. If JSON serialization helpers are needed, implement the small fixed schema safely in-tree; do not add `serde` or other dependencies.

Keep raw-command rendering isolated behind `--show-commands`; test the default path first.

### Task 3: Add safe conventional-path reporting

**Files:**
- Modify: `src/main.rs`
- Test: `tests/cli.rs`

Implement `paths` using only exact conventional locations rooted at the current user home directory. Report existence by metadata/stat only. Do not infer format, recursively inspect, or read the files.

### Task 4: Documentation and scope cleanup

**Files:**
- Modify: `README.md`
- Modify: `docs/ROADMAP.md`
- Modify: `TODO.md` only if a completed TODO changes

Document installation/build (`cargo build --release`), all CLI commands and privacy defaults, explicit formats, safe examples using fixtures, and the continuing non-goals. Keep CWD capture, timestamp config writing, shell hooks, retrieval UI, models, database/indexing, and macro execution explicitly deferred.

### Task 5: Verification and review

Run:

```bash
cargo test
cargo check --all-targets
cargo run -- --help
cargo run -- --version
```

`rustfmt` and `clippy` are absent from this local toolchain. Do not install them; state that precise environmental limitation in the final report.

Before declaring completion, have a separate privacy/security subagent inspect the CLI paths for accidental raw-history output, unsafe path discovery, shell execution, and dependency additions. Have another subagent check the CLI contract/test coverage against this document. Fix only concrete findings and rerun the full suite.

## Definition of done

- `cargo build --release` produces a `habits` binary.
- All CLI contract tests and existing history-analysis tests pass.
- `inspect` supports each explicit format and safe default output.
- `paths` is exact-path, metadata-only, and never guesses a format.
- No dependencies are added.
- No raw command reaches default stdout, JSON, errors, or usage.
- README accurately documents usage and privacy boundaries.
- No shell configuration or history data is mutated.
- A multi-agent implementation/review report records which agents were used, their model(s), findings, fixes, and exact verification commands/results.
