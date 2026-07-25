# Habits

Habits is a local, deterministic visual reverse-history picker for Zsh, Bash,
and PowerShell/PSReadLine histories.

## Install

```sh
cargo build --release
install target/release/habits ~/.local/bin/habits
```

## Use

Discover the three conventional history files, parser choices, and entry
counts:

```sh
habits paths
habits paths --json
habits paths --path ./history --format zsh-extended
```

Open the interactive picker with an optional initial command:

```sh
habits select --query "git st"
```

The grid narrows as you type and shows up to five suggestions in each of four
deterministic columns: Prefix, Fuzzy, Frequency, and Sequence. Scores are local
to each model. Sequence uses the most recent prior command and repeated history
paths to suggest single-command jumps up to three steps ahead; it never executes
a multi-command sequence.

Up/Down or Ctrl-K/Ctrl-J move between the typed-input row and suggestions.
Left/Right changes columns. Enter executes exactly the highlighted command;
Escape returns to the untouched shell buffer.

Enable the optional Zsh Ctrl-R widget by explicitly adding this sourceable
output to your shell setup:

```sh
habits shell-init zsh >> ~/.zshrc
```

`shell-init` only prints setup text; Habits never edits `.zshrc`. The widget
leaves the current buffer unchanged on cancellation, errors, or confirmation
of the typed-input row.

## Privacy

History stays local. Automatic discovery reads only the conventional Zsh,
Bash, and PSReadLine files under the home directory; a manual path replaces
those targets. Default, help, discovery, and error output never contains
commands. Raw commands appear in an explicitly invoked interactive selector or
through the legacy diagnostic `inspect --show-commands` opt-in, which emits a
warning. Habits does not log or persist commands, execute selections, call a
network service, or write shell configuration.

See [the roadmap](docs/ROADMAP.md) for deliberately excluded work.
