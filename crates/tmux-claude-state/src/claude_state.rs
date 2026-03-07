use regex::Regex;
use std::sync::OnceLock;

/// The high-level state of a Claude Code session.
#[derive(Debug, PartialEq, Clone)]
pub enum ClaudeState {
    /// Claude is actively processing (e.g. thinking, running tools).
    Working,
    /// Claude is waiting for user approval (e.g. permission prompts, confirmations).
    WaitingForApproval,
    /// Claude is idle at the input prompt.
    Idle,
}

impl ClaudeState {
    /// Human-readable label for display (e.g. "Running", "Approval", "Idle").
    pub fn display_label(&self) -> &'static str {
        match self {
            ClaudeState::Working => "Running",
            ClaudeState::WaitingForApproval => "Approval",
            ClaudeState::Idle => "Idle",
        }
    }
}

/// The permission mode of a Claude Code session.
#[derive(Debug, PartialEq, Clone)]
pub enum PermissionMode {
    /// Default mode — Claude asks before making edits.
    AskBeforeEdits,
    /// Claude automatically applies edits without asking.
    EditAutomatically,
    /// Claude only plans but does not execute.
    PlanMode,
}

impl PermissionMode {
    /// Human-readable label for display.
    pub fn display_label(&self) -> &'static str {
        match self {
            PermissionMode::AskBeforeEdits => "Ask",
            PermissionMode::EditAutomatically => "Auto Edit",
            PermissionMode::PlanMode => "Plan",
        }
    }
}

/// Detect the permission mode from captured tmux pane content.
///
/// Scans only lines below the last separator (status bar area) for mode indicators.
pub fn detect_permission_mode(content: &str) -> PermissionMode {
    // Collect lines below the last separator — these form the status bar area.
    for line in content.split('\n').rev() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if is_separator_line(trimmed) || trimmed.starts_with('❯') {
            break;
        }
        if trimmed.contains("accept edits on") {
            return PermissionMode::EditAutomatically;
        }
        if trimmed.contains("plan mode on") {
            return PermissionMode::PlanMode;
        }
    }
    PermissionMode::AskBeforeEdits
}

const LAST_LINES_COUNT: usize = 30;

/// Classify the state of a Claude Code session from captured tmux pane content.
///
/// Inspects the last non-empty lines of `content` and returns the detected
/// [`ClaudeState`]. When no recognisable pattern is found the function
/// falls back to [`ClaudeState::Idle`].
pub fn detect_state(content: &str) -> ClaudeState {
    let lines: Vec<&str> = content.split('\n').collect();
    let last_lines = last_non_empty_lines(&lines, LAST_LINES_COUNT);
    let combined = last_lines.join("\n");

    // Running check (highest priority)
    // Format 1: (esc to interrupt · 1m 45s · ...) — time after middle dot
    if running_pattern().is_match(&combined) {
        return ClaudeState::Working;
    }

    // Format 2: (1m 52s · ...) — time at beginning of parentheses
    if running_pattern_time_first().is_match(&combined) {
        return ClaudeState::Working;
    }

    // Fallback: "(esc to interrupt)" or "(ctrl+c to interrupt)" — no time
    if running_fallback_pattern().is_match(&combined) {
        return ClaudeState::Working;
    }

    if esc_to_interrupt_end_pattern().is_match(&combined) {
        return ClaudeState::Working;
    }

    // Catches the initial thinking phase before a timer appears (e.g., "(thinking)")
    if running_generic_pattern().is_match(&combined) {
        return ClaudeState::Working;
    }

    if is_claude_prompt_line(&lines) {
        return ClaudeState::Idle;
    }

    for &pattern in WAITING_PATTERNS.iter() {
        if combined.contains(pattern) {
            return ClaudeState::WaitingForApproval;
        }
    }

    if interview_pattern().is_match(&combined) {
        return ClaudeState::WaitingForApproval;
    }

    if selection_menu_pattern().is_match(&combined) {
        return ClaudeState::WaitingForApproval;
    }

    if idle_pattern().is_match(&combined) {
        return ClaudeState::Idle;
    }

    ClaudeState::Idle // no Unknown state
}

static WAITING_PATTERNS: &[&str] = &[
    "Yes, allow once",
    "Yes, allow always",
    "Allow once",
    "Allow always",
    "❯ Yes",
    "❯ No",
    "Do you trust",
    "Run this command?",
    "Allow this MCP server",
    "Continue?",
    "Proceed?",
    "Do you want to proceed?",
    "(Y/n)",
    "(y/N)",
    "[Y/n]",
    "[y/N]",
];

fn running_pattern() -> &'static Regex {
    static P: OnceLock<Regex> = OnceLock::new();
    P.get_or_init(|| {
        // Format 1: (esc to interrupt · 1m 45s · ...) — time after middle dot
        Regex::new(r"(?m)^[✢✽✶✻·]\s+.+?…?\s*\([^)]*·\s*((?:\d+[smh]\s*)+)").unwrap()
    })
}

fn running_pattern_time_first() -> &'static Regex {
    static P: OnceLock<Regex> = OnceLock::new();
    P.get_or_init(|| {
        // Format 2: (1m 52s · ...) — time at beginning of parentheses
        Regex::new(r"(?m)^[✢✽✶✻·]\s+.+?…?\s*\(((?:\d+[smh]\s*)+)\s*·").unwrap()
    })
}

fn running_fallback_pattern() -> &'static Regex {
    static P: OnceLock<Regex> = OnceLock::new();
    P.get_or_init(|| Regex::new(r"(?m)^[✢✽✶✻·]\s+.+?…?\s*\((esc|ctrl\+c) to interrupt").unwrap())
}

fn esc_to_interrupt_end_pattern() -> &'static Regex {
    static P: OnceLock<Regex> = OnceLock::new();
    P.get_or_init(|| Regex::new(r"(?m)·\s*esc to interrupt(\s|·|$)").unwrap())
}

fn running_generic_pattern() -> &'static Regex {
    static P: OnceLock<Regex> = OnceLock::new();
    P.get_or_init(|| {
        // ^ excludes indented lines (quoted text)
        Regex::new(r"(?m)^[✢✽✶✻·]\s+.+?…").unwrap()
    })
}

fn selection_menu_pattern() -> &'static Regex {
    static P: OnceLock<Regex> = OnceLock::new();
    P.get_or_init(|| Regex::new(r"❯\s+\d+\.").unwrap())
}

fn file_changes_pattern() -> &'static Regex {
    static P: OnceLock<Regex> = OnceLock::new();
    P.get_or_init(|| Regex::new(r"^\s*\d+\s+files?\s+[+\-]").unwrap())
}

fn idle_pattern() -> &'static Regex {
    static P: OnceLock<Regex> = OnceLock::new();
    P.get_or_init(|| Regex::new(r"(?m)^\s*❯").unwrap())
}

fn interview_pattern() -> &'static Regex {
    static P: OnceLock<Regex> = OnceLock::new();
    P.get_or_init(|| Regex::new(r"Enter to select.*↑/↓ to navigate.*Esc to cancel").unwrap())
}

fn last_non_empty_lines<'a>(lines: &[&'a str], n: usize) -> Vec<&'a str> {
    let mut result = Vec::new();
    for &line in lines.iter().rev() {
        if result.len() >= n {
            break;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if is_separator_line(trimmed) {
            continue;
        }
        result.push(line);
    }
    result.reverse();
    result
}

/// Returns true for empty strings (matches Go's isSeparatorLine behavior).
fn is_separator_line(line: &str) -> bool {
    line.chars().all(|c| ('\u{2500}'..='\u{257F}').contains(&c))
}

fn is_claude_prompt_line(lines: &[&str]) -> bool {
    let sel = selection_menu_pattern();
    let file_changes = file_changes_pattern();

    for &line in lines.iter().rev() {
        let trimmed = line.trim();

        if trimmed.is_empty() {
            continue;
        }
        if is_separator_line(trimmed) {
            continue;
        }

        // Skip status bar lines that appear below the ❯ prompt.
        // These are not user content and should be ignored when searching
        // upward for the prompt line.
        if trimmed.contains("? for shortcuts")
            || trimmed.contains("ctrl+")
            || trimmed.contains("shift+")
            || file_changes.is_match(trimmed)
            // Vim mode indicators: "-- INSERT --", "-- NORMAL --", etc.
            || trimmed.starts_with("-- ") && trimmed.ends_with(" --")
            // Model/context display: "[Opus 4.6] Context: 0%", "[Sonnet 4.6] Context: 18%"
            || trimmed.contains("] Context:")
        {
            continue;
        }

        if trimmed.starts_with('❯') {
            if sel.is_match(trimmed) {
                return false; // "❯ 1. Yes" style selection menu
            }
            // ❯-prefixed waiting patterns (e.g., "❯ Yes", "❯ No") are not prompts
            for &pattern in WAITING_PATTERNS.iter() {
                if pattern.starts_with('❯') && trimmed == pattern {
                    return false;
                }
            }
            return true;
        }

        return false;
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn separator_line_box_drawing() {
        assert!(is_separator_line("─────────────────"));
    }

    #[test]
    fn separator_line_double() {
        assert!(is_separator_line("═════════════════"));
    }

    #[test]
    fn separator_line_mixed_box_drawing() {
        assert!(is_separator_line("─═─═─═─═─"));
    }

    #[test]
    fn separator_line_text_content() {
        assert!(!is_separator_line("Some text"));
    }

    #[test]
    fn separator_line_mixed_with_text() {
        assert!(!is_separator_line("───text───"));
    }

    #[test]
    fn separator_line_empty() {
        assert!(is_separator_line(""));
    }
}
