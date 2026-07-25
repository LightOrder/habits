# Habits Roadmap

## Product thesis

A terminal history should become useful local memory: retrieve commands quickly, understand repeated work, and suggest macros only from evidence.

## Current milestone — v0: local inspection CLI

**Goal:** provide a tested, dependency-free CLI and core that inspect one
explicitly selected historical file without pretending it contains information
it does not and without disclosing commands by default.

### In scope

1. Pure parsers for explicitly selected Bash plain/timestamped, Zsh plain/extended, and PowerShell/PSReadLine payloads.
2. A stable `HistoryEntry` representation with source, file order, command, and optional Unix timestamp.
3. Exact frequency counting.
4. Time-valid batching, broken by missing timestamps or a configurable idle gap.
5. Per-batch repeated contiguous n-gram discovery and a deterministic, explainable ranking.
6. Fixture-driven unit/integration tests.
7. A safe-default `habits inspect` command with human and deterministic JSON reports.
8. A metadata-only `habits paths` command for three exact conventional locations.

### Out of scope

- Automatic filesystem discovery, format detection, and shell configuration. `inspect` accepts only an explicit caller-selected path; `paths` stats only three documented candidates.
- Merging multiple sources into one global timeline.
- Exit status, CWD, project root, session ID, or command duration capture.
- Persistent index/database.
- Search UI, fzf binding, or a `Ctrl-R` replacement.
- Automatic macro creation or execution.
- Timestamp configuration writing, models, remote calls, and history mutation.

## Next milestones (not implementation commitments)

### v0.1 — reliable capture

Add opt-in shell hooks to record command, timestamp, exit status, CWD/repository root, and session. This is when batch and sequence analysis becomes materially reliable.

### v0.2 — retrieval

Build a lightweight local SQLite index and a shell-buffer integration. Use lexical/fuzzy matching plus transparent context, frequency, recency, and success scoring. Keep fzf optional as the picker rather than recreating its UI.

### v0.3 — macro proposals

Show repeated sequences with evidence: occurrences, examples, context, arguments that vary, and commands with sensitive-looking material redacted. Require explicit user approval and make generated macros editable, local, and reversible.

## Design constraints

- Rust-first, dependency-light, and local-only by default.
- Deterministic utilities before inference.
- No reinterpretation of shell syntax during normalization.
- Never silently execute a suggested sequence.
- Preserve provenance and distinguish observed facts from derived ranks.
- Optimize for a small trusted core, not a generic terminal platform.

## Open questions to resolve with real usage

1. What histories and timestamp formats do users actually have?
2. Is a 5-minute default idle gap useful, or should batching be session-aware only after hook capture?
3. Which sequence patterns are useful enough to surface without creating suggestion noise?
4. How should sensitive arguments be detected/redacted before analysis output or macro proposals?
