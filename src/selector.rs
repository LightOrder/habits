use std::io::{self, IsTerminal, Write};
use std::time::Duration;

use crossterm::cursor::{Hide, MoveTo, Show};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::style::{Attribute, Print, SetAttribute};
use crossterm::terminal::{
    self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode,
    enable_raw_mode,
};
use crossterm::{execute, queue};

use crate::{CandidateKind, HistoryEntry, RankedCandidate, rank_candidates};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SelectorAction {
    Continue,
    Confirm(String),
    Cancel,
}

#[derive(Clone, Debug)]
pub struct SelectorState {
    original_query: String,
    query: String,
    candidates: Vec<RankedCandidate>,
    highlighted: usize,
}

impl SelectorState {
    pub fn new(histories: &[Vec<HistoryEntry>], query: &str) -> Self {
        Self {
            original_query: query.to_owned(),
            query: query.to_owned(),
            candidates: rank_candidates(histories, query),
            highlighted: 0,
        }
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    pub fn original_query(&self) -> &str {
        &self.original_query
    }

    pub fn candidates(&self) -> &[RankedCandidate] {
        &self.candidates
    }

    pub fn highlighted(&self) -> usize {
        self.highlighted
    }

    pub fn handle_key(&mut self, key: KeyEvent, histories: &[Vec<HistoryEntry>]) -> SelectorAction {
        match key.code {
            KeyCode::Esc => SelectorAction::Cancel,
            KeyCode::Enter => self
                .candidates
                .get(self.highlighted)
                .map(|candidate| SelectorAction::Confirm(candidate.command.clone()))
                .unwrap_or(SelectorAction::Cancel),
            KeyCode::Up => {
                self.highlighted = self.highlighted.saturating_sub(1);
                SelectorAction::Continue
            }
            KeyCode::Down => {
                self.highlighted =
                    (self.highlighted + 1).min(self.candidates.len().saturating_sub(1));
                SelectorAction::Continue
            }
            KeyCode::Backspace => {
                self.query.pop();
                self.refresh(histories);
                SelectorAction::Continue
            }
            KeyCode::Char(character)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                self.query.push(character);
                self.refresh(histories);
                SelectorAction::Continue
            }
            _ => SelectorAction::Continue,
        }
    }

    fn refresh(&mut self, histories: &[Vec<HistoryEntry>]) {
        self.candidates = rank_candidates(histories, &self.query);
        self.highlighted = 0;
    }
}

struct TerminalGuard {
    active: bool,
}

impl TerminalGuard {
    fn enter() -> Result<Self, String> {
        enable_raw_mode()
            .map_err(|error| format!("could not enable terminal raw mode: {error}"))?;
        let mut guard = Self { active: true };
        if let Err(error) = execute!(io::stderr(), EnterAlternateScreen, Hide) {
            guard.restore_best_effort();
            return Err(format!("could not initialize selector display: {error}"));
        }
        Ok(guard)
    }

    fn restore(&mut self) -> Result<(), String> {
        if self.active {
            let display_result = execute!(io::stderr(), Show, LeaveAlternateScreen);
            let raw_result = disable_raw_mode();
            self.active = false;
            display_result
                .map_err(|error| format!("could not restore selector display: {error}"))?;
            raw_result.map_err(|error| format!("could not restore terminal mode: {error}"))?;
        }
        Ok(())
    }

    fn restore_best_effort(&mut self) {
        let _ = self.restore();
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        self.restore_best_effort();
    }
}

pub fn run_selector(
    histories: &[Vec<HistoryEntry>],
    initial_query: &str,
) -> Result<Option<String>, String> {
    require_interactive_terminal()?;

    let mut terminal = TerminalGuard::enter()?;
    let mut state = SelectorState::new(histories, initial_query);
    let selected = loop {
        render(&state)?;
        if !event::poll(Duration::from_millis(250))
            .map_err(|error| format!("could not read terminal input: {error}"))?
        {
            continue;
        }
        let Event::Key(key) =
            event::read().map_err(|error| format!("could not read terminal input: {error}"))?
        else {
            continue;
        };
        match state.handle_key(key, histories) {
            SelectorAction::Continue => {}
            SelectorAction::Confirm(command) => break Some(command),
            SelectorAction::Cancel => break None,
        }
    };
    terminal.restore()?;
    Ok(selected)
}

pub fn require_interactive_terminal() -> Result<(), String> {
    if !io::stdin().is_terminal() || !io::stderr().is_terminal() {
        return Err("select requires an interactive terminal".to_owned());
    }
    Ok(())
}

fn render(state: &SelectorState) -> Result<(), String> {
    let mut stderr = io::stderr().lock();
    let (columns, rows) = terminal::size().unwrap_or((80, 10));
    queue!(
        stderr,
        MoveTo(0, 0),
        Clear(ClearType::All),
        Print("habits> "),
        Print(truncate_for_terminal(
            state.query(),
            columns.saturating_sub(8) as usize
        )),
        Print("\r\n")
    )
    .map_err(|error| format!("could not render selector: {error}"))?;

    let available_rows = (rows.saturating_sub(2) as usize).max(1);
    let start = state
        .highlighted()
        .saturating_sub(available_rows.saturating_sub(1));
    for (index, candidate) in state
        .candidates()
        .iter()
        .skip(start)
        .take(available_rows)
        .enumerate()
    {
        if start + index == state.highlighted() {
            queue!(stderr, SetAttribute(Attribute::Reverse))
                .map_err(|error| format!("could not render selector: {error}"))?;
        }
        let marker = if candidate.kind == CandidateKind::TypedInput {
            "typed"
        } else {
            "     "
        };
        let command = truncate_for_terminal(&candidate.command, columns.saturating_sub(7) as usize);
        queue!(
            stderr,
            Print(format!("{marker}  {command}\r\n")),
            SetAttribute(Attribute::Reset)
        )
        .map_err(|error| format!("could not render selector: {error}"))?;
    }
    stderr
        .flush()
        .map_err(|error| format!("could not flush selector display: {error}"))
}

pub fn sanitize_for_terminal(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect()
}

pub fn truncate_for_terminal(value: &str, width: usize) -> String {
    sanitize_for_terminal(value).chars().take(width).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{HistoryFormat, parse_history};

    fn histories() -> Vec<Vec<HistoryEntry>> {
        vec![
            parse_history(
                HistoryFormat::ZshPlain,
                "git status\ncargo test\ngit status\n",
            )
            .unwrap(),
        ]
    }

    #[test]
    fn arrows_only_move_highlight_and_never_change_typed_query() {
        let histories = histories();
        let mut state = SelectorState::new(&histories, "git");
        state.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE), &histories);
        assert_eq!(state.query(), "git");
        assert_eq!(state.original_query(), "git");
        assert_eq!(state.highlighted(), 1);
        state.handle_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE), &histories);
        assert_eq!(state.query(), "git");
        assert_eq!(state.highlighted(), 0);
    }

    #[test]
    fn typing_live_narrows_and_resets_highlight_to_typed_row() {
        let histories = histories();
        let mut state = SelectorState::new(&histories, "g");
        state.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE), &histories);
        state.handle_key(
            KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE),
            &histories,
        );
        assert_eq!(state.query(), "gi");
        assert_eq!(state.highlighted(), 0);
        assert!(
            state
                .candidates()
                .iter()
                .any(|row| row.command == "git status")
        );
        assert!(
            !state
                .candidates()
                .iter()
                .any(|row| row.command == "cargo test")
        );
    }

    #[test]
    fn typed_row_is_navigable_and_enter_confirms_highlighted_command() {
        let histories = histories();
        let mut state = SelectorState::new(&histories, "git");
        assert_eq!(
            state.handle_key(
                KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
                &histories
            ),
            SelectorAction::Confirm("git".to_owned())
        );
        state.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE), &histories);
        assert_eq!(
            state.handle_key(
                KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
                &histories
            ),
            SelectorAction::Confirm("git status".to_owned())
        );
    }

    #[test]
    fn escape_cancels_without_losing_original_buffer() {
        let histories = histories();
        let mut state = SelectorState::new(&histories, "git");
        state.handle_key(
            KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE),
            &histories,
        );
        assert_eq!(
            state.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE), &histories),
            SelectorAction::Cancel
        );
        assert_eq!(state.original_query(), "git");
    }

    #[test]
    fn display_sanitization_blocks_terminal_control_sequences_only_in_rendered_copy() {
        let command = "printf '\\e]52;c;secret\\a'\nnext\t\u{1b}[31m";
        let sanitized = sanitize_for_terminal(command);
        assert!(!sanitized.chars().any(char::is_control));
        assert!(sanitized.contains("printf"));
        assert_eq!(command.as_bytes()[0], b'p');
    }

    #[test]
    fn display_truncation_does_not_change_the_selected_command() {
        let command = "a very long command with arguments";
        assert_eq!(truncate_for_terminal(command, 8), "a very l");
        assert_eq!(command, "a very long command with arguments");
    }
}
