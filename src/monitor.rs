use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

const POLL_INTERVAL_SECS: u64 = 2;

use crate::claude_state::{detect_state, ClaudeState};
use crate::tmux::{self, PaneInfo};

#[derive(Debug, Clone)]
pub struct ClaudeSession {
    pub pane: PaneInfo,
    pub state: ClaudeState,
    pub state_changed_at: Instant,
}

#[derive(Debug, Clone, Default)]
pub struct MonitorState {
    pub sessions: Vec<ClaudeSession>,
    pub any_claude_focused: bool,
}

pub fn start_polling(state: Arc<Mutex<MonitorState>>) {
    thread::spawn(move || loop {
        let panes = tmux::list_claude_panes();
        let prev = state
            .lock()
            .ok()
            .map(|g| g.sessions.clone())
            .unwrap_or_default();
        let now = Instant::now();
        let updated: Vec<ClaudeSession> = panes
            .into_iter()
            .map(|pane| {
                let content = tmux::capture_pane(&pane.id);
                let new_state = detect_state(&content);
                let state_changed_at = prev
                    .iter()
                    .find(|s| s.pane.id == pane.id && s.state == new_state)
                    .map(|s| s.state_changed_at)
                    .unwrap_or(now);
                ClaudeSession { pane, state: new_state, state_changed_at }
            })
            .collect();

        let any_claude_focused = tmux::any_claude_pane_focused();

        if let Ok(mut lock) = state.lock() {
            *lock = MonitorState {
                sessions: updated,
                any_claude_focused,
            };
        }

        thread::sleep(Duration::from_secs(POLL_INTERVAL_SECS));
    });
}
