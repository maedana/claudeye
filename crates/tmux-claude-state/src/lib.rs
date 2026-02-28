//! Detect and monitor [Claude Code](https://docs.anthropic.com/en/docs/claude-code)
//! session states inside tmux panes.
//!
//! This crate provides helpers for:
//!
//! - **State detection** — parse captured tmux pane content and classify it as
//!   [`ClaudeState::Working`], [`ClaudeState::WaitingForApproval`], or
//!   [`ClaudeState::Idle`] (see [`claude_state::detect_state`]).
//! - **Tmux interaction** — list Claude panes, capture their content (with or
//!   without ANSI escape sequences), and switch the client to a pane
//!   (see the [`tmux`] module).
//! - **Background polling** — continuously monitor all Claude sessions and
//!   expose the latest state through a shared [`monitor::MonitorState`].

pub mod claude_state;
pub mod monitor;
pub mod tmux;
