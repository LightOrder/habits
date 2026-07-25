# Habits

**Habits is a local memory for terminal work.**

The first release is deliberately narrow: load shell histories, preserve what the sources actually know, and derive inspectable statistics for a future improved reverse search and macro-suggestion workflow.

## v0: safe local inspection CLI

- Parse explicitly selected Bash plain/timestamped, Zsh plain/extended, and PowerShell / PSReadLine history formats.
- Preserve source, source order, raw command text (trimmed only at edges), and optional timestamps.
- Count exact command frequency without rewriting shell syntax.
- Build work batches from contiguous timestamped commands only.
- Find repeated contiguous command sequences inside a batch.
- Inspect one explicitly selected history file without disclosing commands by default.
- Report conventional history paths using exact metadata checks without reading their contents.

### Explicit non-goals

No shell hook, command recorder, database, fuzzy picker, fzf integration, macro execution, LLM/embedding layer, cloud sync, or background daemon.

## Data honesty

History formats vary. Habits requires the caller to select the exact format rather than guessing whether a legal command line is actually metadata. Zsh extended history and timestamp-enabled Bash history can record Unix timestamps. Plain Zsh/Bash and standard PSReadLine history usually cannot. Habits never infers timestamps from line order; untimestamped commands participate in frequency analysis but form hard boundaries for time-based batches.

## Sequence ranking

The first heuristic is transparent:

```text
rank = occurrences × sequence_length²
```

That makes a repeated three-command workflow more interesting than a single popular command. This is a candidate-generation utility, not an automatic macro creator; a future UI must ask before a candidate becomes a named command.

## Development

```bash
cargo test
cargo check --all-targets
cargo build --release
```

The release binary is `target/release/habits`.

## CLI usage

Inspect a fixture or other explicitly selected file:

```bash
mkdir -p ./fixtures
printf ': 1700000000:0;echo example\n' > ./fixtures/example.zsh_history
target/release/habits inspect \
  --format zsh-extended \
  --path ./fixtures/example.zsh_history
```

Supported formats are exactly `bash-plain`, `bash-timestamped`, `zsh-plain`,
`zsh-extended`, and `powershell`. The defaults are a 300-second batch gap and
10 rows; override them with `--gap-seconds` and `--top`. Add `--json` for a
deterministic aggregate report.

By default, reports contain counts and ranks only. Commands, arguments, paths
from history content, and repeated command sequences are withheld. The
`--show-commands` flag is an explicit opt-in that prints a warning to stderr
before raw commands are emitted:

```bash
target/release/habits inspect \
  --format zsh-extended \
  --path ./fixtures/example.zsh_history \
  --show-commands
```

List the three conventional candidate paths without reading their contents:

```bash
target/release/habits paths
target/release/habits paths --json
```

These are suggested format/path pairs, not detected formats. `paths` checks
only whether each exact path exists. `inspect` reads only the path supplied by
the caller. Neither command writes history, shell configuration, caches, or
network data.

For help and version information:

```bash
target/release/habits --help
target/release/habits --version
```

### Explicit local audit harness

The checked-in `audit` example remains a diagnostic library harness. The
first-class interface is now `habits inspect`.

### Continuing non-goals

CWD and exit-code capture, timestamp configuration writing, shell hooks,
retrieval UI, models, database/indexing, and macro creation or execution remain
deferred. Habits performs no automatic directory scanning, format detection,
remote calls, or shell configuration changes.

## Roadmap

See [`docs/ROADMAP.md`](docs/ROADMAP.md).
