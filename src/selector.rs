use std::io::{self, IsTerminal, Write};
use std::time::Duration;

use crossterm::cursor::{Hide, MoveTo, Show};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::style::{Attribute, Print, SetAttribute};
use crossterm::terminal::{
    self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode,
    enable_raw_mode,
};
use crossterm::{execute, queue};

use crate::{HistoryEntry, ModelKind, ScoredSuggestion, SuggestionGrid, suggestion_grid};

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
    grid: SuggestionGrid,
    active_lane: usize,
    selected_row: usize,
}

impl SelectorState {
    pub fn new(histories: &[Vec<HistoryEntry>], query: &str) -> Self {
        Self {
            original_query: query.to_owned(),
            query: query.to_owned(),
            grid: suggestion_grid(histories, query),
            active_lane: 0,
            selected_row: 0,
        }
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    pub fn original_query(&self) -> &str {
        &self.original_query
    }

    pub fn grid(&self) -> &SuggestionGrid {
        &self.grid
    }

    pub fn highlighted(&self) -> usize {
        self.selected_row
    }

    pub fn active_lane(&self) -> usize {
        self.active_lane
    }

    pub fn selected_row(&self) -> usize {
        self.selected_row
    }

    pub fn handle_key(&mut self, key: KeyEvent, histories: &[Vec<HistoryEntry>]) -> SelectorAction {
        if key.kind == KeyEventKind::Release {
            return SelectorAction::Continue;
        }
        match key.code {
            KeyCode::Esc => SelectorAction::Cancel,
            KeyCode::Enter => self
                .selected_command()
                .map(|command| SelectorAction::Confirm(command.to_owned()))
                .unwrap_or(SelectorAction::Cancel),
            KeyCode::Up | KeyCode::Char('k')
                if key.code == KeyCode::Up || key.modifiers.contains(KeyModifiers::CONTROL) =>
            {
                self.selected_row = self.selected_row.saturating_sub(1);
                SelectorAction::Continue
            }
            KeyCode::Down | KeyCode::Char('j')
                if key.code == KeyCode::Down || key.modifiers.contains(KeyModifiers::CONTROL) =>
            {
                let last = self.grid.lanes[self.active_lane].suggestions.len();
                self.selected_row = (self.selected_row + 1).min(last);
                SelectorAction::Continue
            }
            KeyCode::Left => {
                self.active_lane = self.active_lane.saturating_sub(1);
                self.clamp_row();
                SelectorAction::Continue
            }
            KeyCode::Right => {
                self.active_lane = (self.active_lane + 1).min(self.grid.lanes.len() - 1);
                self.clamp_row();
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
        self.grid = suggestion_grid(histories, &self.query);
        self.active_lane = 0;
        self.selected_row = 0;
    }

    fn clamp_row(&mut self) {
        if self.selected_row > 0 {
            self.selected_row = self
                .selected_row
                .min(self.grid.lanes[self.active_lane].suggestions.len());
        }
    }

    fn selected_command(&self) -> Option<&str> {
        if self.selected_row == 0 {
            return Some(&self.grid.typed_input);
        }
        self.grid.lanes[self.active_lane]
            .suggestions
            .get(self.selected_row - 1)
            .map(|suggestion| suggestion.command.as_str())
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
    let (columns, _) = terminal::size().unwrap_or((80, 10));
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

    let column_width = (columns as usize / 4).max(1);
    for (lane_index, lane) in state.grid().lanes.iter().enumerate() {
        let heading = format!(
            "{:<width$}",
            model_heading(lane.model),
            width = column_width
        );
        queue!(stderr, Print(truncate_for_terminal(&heading, column_width)))
            .map_err(|error| format!("could not render selector: {error}"))?;
        if lane_index == 3 {
            queue!(stderr, Print("\r\n"))
                .map_err(|error| format!("could not render selector: {error}"))?;
        }
    }
    for row in 1..=5 {
        for lane_index in 0..4 {
            let selected = cell_is_selected(state, row, lane_index);
            if selected {
                queue!(stderr, SetAttribute(Attribute::Reverse))
                    .map_err(|error| format!("could not render selector: {error}"))?;
            }
            let cell = state.grid().lanes[lane_index]
                .suggestions
                .get(row - 1)
                .map(|suggestion| candidate_row(suggestion, column_width))
                .unwrap_or_default();
            queue!(
                stderr,
                Print(format!("{cell:<column_width$}")),
                SetAttribute(Attribute::Reset)
            )
            .map_err(|error| format!("could not render selector: {error}"))?;
        }
        queue!(stderr, Print("\r\n"))
            .map_err(|error| format!("could not render selector: {error}"))?;
    }
    if state.selected_row() == 0 {
        queue!(stderr, SetAttribute(Attribute::Reverse))
            .map_err(|error| format!("could not render selector: {error}"))?;
    }
    let typed = truncate_for_terminal(
        &format!("typed input  {}", state.grid().typed_input),
        columns as usize,
    );
    queue!(
        stderr,
        Print(typed),
        SetAttribute(Attribute::Reset),
        Print("\r\n")
    )
    .map_err(|error| format!("could not render selector: {error}"))?;
    stderr
        .flush()
        .map_err(|error| format!("could not flush selector display: {error}"))
}

pub fn candidate_row(candidate: &ScoredSuggestion, width: usize) -> String {
    let score = format!("{:02}", candidate.score);
    let score_width = score.chars().count();
    if width <= score_width {
        return score.chars().take(width).collect();
    }
    let command = truncate_for_terminal(&candidate.command, width - score_width - 1);
    format!("{command} {score}")
}

fn model_heading(model: ModelKind) -> &'static str {
    match model {
        ModelKind::Prefix => "Prefix",
        ModelKind::Fuzzy => "Fuzzy",
        ModelKind::Frequency => "Frequency",
        ModelKind::Sequence => "Sequence",
    }
}

fn cell_is_selected(state: &SelectorState, row: usize, lane: usize) -> bool {
    state.selected_row() == row && state.active_lane() == lane
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
        assert_eq!(state.active_lane(), 0);
        assert_eq!(state.selected_row(), 0);
        state.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE), &histories);
        assert_eq!(state.query(), "git");
        assert_eq!(state.original_query(), "git");
        assert_eq!(state.selected_row(), 1);
        state.handle_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE), &histories);
        assert_eq!(state.query(), "git");
        assert_eq!(state.selected_row(), 0);
    }

    #[test]
    fn release_events_do_not_change_selector_state() {
        let histories = histories();
        let mut state = SelectorState::new(&histories, "git");
        assert_eq!(
            state.handle_key(
                KeyEvent::new_with_kind(KeyCode::Down, KeyModifiers::NONE, KeyEventKind::Release),
                &histories,
            ),
            SelectorAction::Continue
        );
        assert_eq!(state.highlighted(), 0);
        assert_eq!(state.query(), "git");
    }

    #[test]
    fn control_j_and_k_navigate_without_becoming_query_text() {
        let histories = histories();
        let mut state = SelectorState::new(&histories, "git");
        state.handle_key(
            KeyEvent::new(KeyCode::Char('j'), KeyModifiers::CONTROL),
            &histories,
        );
        assert_eq!(state.selected_row(), 1);
        assert_eq!(state.query(), "git");
        state.handle_key(
            KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL),
            &histories,
        );
        assert_eq!(state.selected_row(), 0);
        assert_eq!(state.query(), "git");
    }

    #[test]
    fn horizontal_navigation_preserves_or_clamps_rank_in_sparse_lanes() {
        let histories = histories();
        let mut state = SelectorState::new(&histories, "git");
        state.grid.lanes[0].suggestions = vec![
            ScoredSuggestion {
                command: "one".into(),
                score: 100,
            },
            ScoredSuggestion {
                command: "two".into(),
                score: 50,
            },
        ];
        state.grid.lanes[1].suggestions = vec![ScoredSuggestion {
            command: "only".into(),
            score: 100,
        }];
        state.selected_row = 2;
        state.handle_key(
            KeyEvent::new(KeyCode::Right, KeyModifiers::NONE),
            &histories,
        );
        assert_eq!(state.active_lane(), 1);
        assert_eq!(state.selected_row(), 1);
        state.handle_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE), &histories);
        assert_eq!(state.active_lane(), 0);
        assert_eq!(state.selected_row(), 1);
    }

    #[test]
    fn typed_input_stays_selected_during_horizontal_navigation() {
        let histories = histories();
        let mut state = SelectorState::new(&histories, "git");
        state.handle_key(
            KeyEvent::new(KeyCode::Right, KeyModifiers::NONE),
            &histories,
        );
        assert_eq!(state.active_lane(), 1);
        assert_eq!(state.selected_row(), 0);
        assert_eq!(
            state.handle_key(
                KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
                &histories
            ),
            SelectorAction::Confirm("git".to_owned())
        );
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
        assert_eq!(state.active_lane(), 0);
        assert_eq!(state.highlighted(), 0);
        assert!(
            state
                .grid()
                .lanes
                .iter()
                .flat_map(|lane| &lane.suggestions)
                .any(|row| row.command == "git status")
        );
        assert!(
            !state
                .grid()
                .lanes
                .iter()
                .flat_map(|lane| &lane.suggestions)
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

    #[test]
    fn rendered_history_cell_is_compact_and_omits_debug_evidence() {
        let histories = histories();
        let grid = suggestion_grid(&histories, "git");
        let candidate = grid
            .lanes
            .iter()
            .flat_map(|lane| &lane.suggestions)
            .find(|candidate| candidate.command == "git status")
            .unwrap();

        let row = candidate_row(candidate, 120);
        assert!(row.contains("git status"));
        assert!(row.split_whitespace().last().unwrap().parse::<u8>().is_ok());
        for forbidden in [
            "×",
            "←",
            "→",
            "freq",
            "hist",
            "predecessor",
            "successor",
            "depth",
        ] {
            assert!(!row.contains(forbidden));
        }
    }

    #[test]
    fn compact_cell_reserves_space_for_score_when_command_is_long() {
        let candidate = ScoredSuggestion {
            command: "git status --short --branch".to_owned(),
            score: 87,
        };

        let row = candidate_row(&candidate, 20);

        assert!(row.chars().count() <= 20);
        assert!(row.ends_with(" 87"));
    }

    #[test]
    fn grid_has_four_headings_five_rows_and_one_selected_cell() {
        let histories = histories();
        let mut state = SelectorState::new(&histories, "git");
        state.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE), &histories);
        assert_eq!(
            state
                .grid()
                .lanes
                .iter()
                .map(|lane| model_heading(lane.model))
                .collect::<Vec<_>>(),
            ["Prefix", "Fuzzy", "Frequency", "Sequence"]
        );
        assert_eq!((1..=5).count(), 5);
        assert_eq!(
            (1..=5)
                .flat_map(|row| (0..4).map(move |lane| (row, lane)))
                .filter(|&(row, lane)| cell_is_selected(&state, row, lane))
                .count(),
            1
        );
    }
}
