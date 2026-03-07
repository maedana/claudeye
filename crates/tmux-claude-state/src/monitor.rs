use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

const POLL_INTERVAL_SECS: u64 = 2;

use crate::claude_state::{ClaudeState, PermissionMode, detect_permission_mode, detect_state};
use crate::tmux::{self, PaneInfo};

/// A single Claude Code session with its detected state.
#[derive(Debug, Clone)]
pub struct ClaudeSession {
    /// The tmux pane where this session is running.
    pub pane: PaneInfo,
    /// The last detected state.
    pub state: ClaudeState,
    /// The detected permission mode.
    pub permission_mode: PermissionMode,
    /// When the current state was first observed.
    pub state_changed_at: Instant,
}

/// Shared snapshot of all monitored Claude sessions.
#[derive(Debug, Clone, Default)]
pub struct MonitorState {
    /// All currently active Claude sessions.
    pub sessions: Vec<ClaudeSession>,
    /// Whether a Claude pane is currently focused in the terminal.
    pub any_claude_focused: bool,
}

/// Spawn a background thread that polls tmux every 2 seconds, updating
/// `state` with the latest [`MonitorState`].
pub fn start_polling(state: Arc<Mutex<MonitorState>>) {
    thread::spawn(move || {
        loop {
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
                    let permission_mode = detect_permission_mode(&content);
                    let state_changed_at = prev
                        .iter()
                        .find(|s| s.pane.id == pane.id && s.state == new_state)
                        .map(|s| s.state_changed_at)
                        .unwrap_or(now);
                    ClaudeSession {
                        pane,
                        state: new_state,
                        permission_mode,
                        state_changed_at,
                    }
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
        }
    });
}
