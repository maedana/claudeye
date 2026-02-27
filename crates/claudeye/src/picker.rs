use std::io::{self, Stdout};

use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState},
};

use tmux_claude_state::claude_state::{ClaudeState, detect_state};
use tmux_claude_state::monitor::ClaudeSession;
use tmux_claude_state::tmux;

pub struct PickerState {
    pub sessions: Vec<ClaudeSession>,
    pub selected: usize,
}

impl PickerState {
    pub fn new(sessions: Vec<ClaudeSession>) -> Self {
        Self {
            sessions,
            selected: 0,
        }
    }

    pub fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    pub fn move_down(&mut self) {
        if self.selected + 1 < self.sessions.len() {
            self.selected += 1;
        }
    }

    pub fn selected_pane_id(&self) -> Option<&str> {
        self.sessions.get(self.selected).map(|s| s.pane.id.as_str())
    }

    pub fn pane_id_at(&self, idx: usize) -> Option<&str> {
        self.sessions.get(idx).map(|s| s.pane.id.as_str())
    }
}

pub fn run_picker() -> io::Result<()> {
    let panes = tmux::list_claude_panes();
    let sessions: Vec<ClaudeSession> = panes
        .into_iter()
        .map(|pane| {
            let content = tmux::capture_pane(&pane.id);
            let state = detect_state(&content);
            ClaudeSession {
                pane,
                state,
                state_changed_at: std::time::Instant::now(),
            }
        })
        .collect();

    if sessions.is_empty() {
        println!("No Claude sessions found");
        return Ok(());
    }

    let mut picker = PickerState::new(sessions);

    enable_raw_mode()?;
    execute!(io::stdout(), EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;

    let selected_pane = run_loop(&mut terminal, &mut picker);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;

    if let Some(pane_id) = selected_pane {
        tmux::switch_to_pane(&pane_id);
    }

    Ok(())
}

fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    picker: &mut PickerState,
) -> Option<String> {
    loop {
        if terminal.draw(|f| render(f, picker)).is_err() {
            return None;
        }

        match event::read() {
            Ok(Event::Key(key)) if key.kind == KeyEventKind::Press => match key.code {
                KeyCode::Char('j') | KeyCode::Down => picker.move_down(),
                KeyCode::Char('k') | KeyCode::Up => picker.move_up(),
                KeyCode::Enter => return picker.selected_pane_id().map(|s| s.to_string()),
                KeyCode::Char('q') | KeyCode::Esc => return None,
                KeyCode::Char(c @ '1'..='9') => {
                    let idx = (c as usize) - ('1' as usize);
                    if let Some(id) = picker.pane_id_at(idx) {
                        return Some(id.to_string());
                    }
                }
                _ => {}
            },
            Err(_) => return None,
            _ => {}
        }
    }
}

fn render(f: &mut ratatui::Frame, picker: &PickerState) {
    let items: Vec<ListItem> = picker
        .sessions
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let (indicator, color, label) = state_display(&s.state);
            let prefix = if i < 9 {
                format!("{}. ", i + 1)
            } else {
                "   ".to_string()
            };
            ListItem::new(Line::from(Span::styled(
                format!(
                    "{}{} {}  {}  [{}]",
                    prefix, indicator, s.pane.id, s.pane.project_name, label
                ),
                Style::default().fg(color),
            )))
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("1-9: jump  j/k: move  Enter: switch  q: quit"),
        )
        .highlight_style(Style::default().bg(Color::DarkGray))
        .highlight_symbol("▶ ");

    let mut list_state = ListState::default();
    list_state.select(Some(picker.selected));

    f.render_stateful_widget(list, f.area(), &mut list_state);
}

fn state_display(state: &ClaudeState) -> (&'static str, Color, &'static str) {
    match state {
        ClaudeState::Working => ("●", Color::Green, "Running"),
        ClaudeState::WaitingForApproval => ("●", Color::Yellow, "Approval"),
        ClaudeState::Idle => ("○", Color::Gray, "Idle"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tmux_claude_state::tmux::PaneInfo;

    fn make_session(id: &str) -> ClaudeSession {
        ClaudeSession {
            pane: PaneInfo {
                id: id.to_string(),
                pid: 0,
                cwd: "/tmp".to_string(),
                project_name: "test".to_string(),
            },
            state: ClaudeState::Idle,
            state_changed_at: std::time::Instant::now(),
        }
    }

    #[test]
    fn move_down_advances_selection() {
        let mut state = PickerState::new(vec![
            make_session("a"),
            make_session("b"),
            make_session("c"),
        ]);
        assert_eq!(state.selected, 0);
        state.move_down();
        assert_eq!(state.selected, 1);
        state.move_down();
        assert_eq!(state.selected, 2);
    }

    #[test]
    fn move_down_stops_at_last() {
        let mut state = PickerState::new(vec![make_session("a"), make_session("b")]);
        state.move_down();
        state.move_down(); // 末尾で止まる
        assert_eq!(state.selected, 1);
    }

    #[test]
    fn move_up_decreases_selection() {
        let mut state = PickerState::new(vec![make_session("a"), make_session("b")]);
        state.move_down();
        state.move_up();
        assert_eq!(state.selected, 0);
    }

    #[test]
    fn move_up_stops_at_first() {
        let mut state = PickerState::new(vec![make_session("a"), make_session("b")]);
        state.move_up(); // 先頭で止まる
        assert_eq!(state.selected, 0);
    }

    #[test]
    fn selected_pane_id_returns_current() {
        let mut state = PickerState::new(vec![make_session("pane1"), make_session("pane2")]);
        assert_eq!(state.selected_pane_id(), Some("pane1"));
        state.move_down();
        assert_eq!(state.selected_pane_id(), Some("pane2"));
    }

    #[test]
    fn pane_id_at_returns_correct_id() {
        let state = PickerState::new(vec![
            make_session("alpha"),
            make_session("beta"),
            make_session("gamma"),
        ]);
        assert_eq!(state.pane_id_at(0), Some("alpha"));
        assert_eq!(state.pane_id_at(1), Some("beta"));
        assert_eq!(state.pane_id_at(2), Some("gamma"));
    }

    #[test]
    fn pane_id_at_returns_none_for_out_of_bounds() {
        let state = PickerState::new(vec![make_session("only")]);
        assert_eq!(state.pane_id_at(1), None);
        assert_eq!(state.pane_id_at(9), None);
    }
}
