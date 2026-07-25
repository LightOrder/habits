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

The list narrows as you type. Up and Down move the highlight without changing
the typed command. Each history row shows observed frequency and, when known,
its most common adjacent commands. Enter executes the highlighted history or
typed-input row; Escape returns to the untouched shell buffer.

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
