use std::collections::HashSet;
use std::process::Command;
use std::sync::OnceLock;

#[derive(Debug, Clone)]
pub struct PaneInfo {
    pub id: String,
    #[allow(dead_code)]
    pub pid: u32,
    #[allow(dead_code)]
    pub cwd: String,
    pub project_name: String,
}

pub fn list_claude_panes() -> Vec<PaneInfo> {
    let output = Command::new("tmux")
        .args([
            "list-panes",
            "-a",
            "-F",
            "#{session_name}:#{window_index}.#{pane_index} #{pane_pid} #{pane_current_path} #{pane_current_command}",
        ])
        .output();

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            stdout
                .lines()
                .filter_map(parse_pane_line)
                .collect()
        }
        Err(e) => {
            eprintln!("[claudeye] tmux list-panes failed: {e}");
            vec![]
        }
    }
}

pub fn parse_pane_line(line: &str) -> Option<PaneInfo> {
    let parts: Vec<&str> = line.splitn(4, ' ').collect();
    if parts.len() < 4 {
        return None;
    }
    let id = parts[0].to_string();
    let pid: u32 = parts[1].parse().ok()?;
    let cwd = parts[2].to_string();
    let command = parts[3].to_string();

    if !is_claude_command(command.trim()) {
        return None;
    }

    let project_name = std::path::Path::new(&cwd)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    Some(PaneInfo {
        id,
        pid,
        cwd,
        project_name,
    })
}


pub fn switch_to_pane(pane_id: &str) {
    let result = Command::new("tmux")
        .args(["switch-client", "-t", pane_id])
        .output();
    if let Err(e) = result {
        eprintln!("[claudeye] tmux switch-client failed: {e}");
    }
}

/// Check whether the tmux pane command corresponds to a claude process.
fn is_claude_command(command: &str) -> bool {
    if command == "claude" {
        return true;
    }
    claude_version_names().contains(command)
}

/// On macOS, the `claude` binary is a symlink to a versioned path
/// (e.g. `~/.local/share/claude/versions/2.1.50`), so tmux resolves
/// the symlink and reports the version number as the command name.
/// Since multiple versions may coexist (older sessions survive across
/// upgrades), we cache all filenames in the versions directory at startup.
/// On Linux (or when `claude` is not a symlink), this returns an empty set
/// and detection falls back to the `command == "claude"` check above.
fn claude_version_names() -> &'static HashSet<String> {
    static NAMES: OnceLock<HashSet<String>> = OnceLock::new();
    NAMES.get_or_init(|| resolve_claude_versions().unwrap_or_default())
}

fn resolve_claude_versions() -> Option<HashSet<String>> {
    let output = Command::new("which").arg("claude").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let target = std::fs::read_link(&path).ok()?;
    let versions_dir = target.parent()?;
    let entries = std::fs::read_dir(versions_dir)
        .ok()?
        .filter_map(|e| e.ok())
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    Some(entries)
}

/// Check whether any Claude pane is currently focused AND the tmux terminal window
/// has OS-level focus.
///
/// Requires `set-option -g focus-events on` in tmux for the terminal focus detection
/// to work. When focus-events is off (or the terminal doesn't support it), `client_flags`
/// won't contain "focused" and this function returns `false` (no suppression — safe default).
pub fn any_claude_pane_focused() -> bool {
    let session = match focused_client_session() {
        Some(s) => s,
        None => return false,
    };

    let output = Command::new("tmux")
        .args([
            "list-panes",
            "-a",
            "-F",
            "#{session_name} #{pane_active} #{window_active} #{pane_current_command}",
        ])
        .output();

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            stdout
                .lines()
                .any(|line| is_focused_claude_line_in_session(line, &session))
        }
        Err(_) => false,
    }
}

/// Return the session name of the focused tmux client, if any.
fn focused_client_session() -> Option<String> {
    let output = Command::new("tmux")
        .args(["list-clients", "-F", "#{client_flags} #{client_session}"])
        .output()
        .ok()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout.lines().find_map(parse_focused_session)
}

/// Parse a line of `tmux list-clients -F "#{client_flags} #{client_session}"`
/// and return the session name if the client has the "focused" flag.
pub fn parse_focused_session(line: &str) -> Option<String> {
    let (flags, session) = line.split_once(' ')?;
    if flags.split(',').any(|flag| flag.trim() == "focused") {
        Some(session.to_string())
    } else {
        None
    }
}

/// Parse a single line of `tmux list-panes -a -F "#{session_name} #{pane_active} #{window_active} #{pane_current_command}"`
/// and return true if the pane belongs to the given session, is active, and running a Claude process.
pub fn is_focused_claude_line_in_session(line: &str, session: &str) -> bool {
    let parts: Vec<&str> = line.splitn(4, ' ').collect();
    if parts.len() < 4 {
        return false;
    }
    parts[0] == session
        && parts[1] == "1"
        && parts[2] == "1"
        && is_claude_command(parts[3].trim())
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- parse_focused_session ---

    #[test]
    fn parse_focused_session_with_focused_client() {
        assert_eq!(
            parse_focused_session("attached,focused,UTF-8 my-session"),
            Some("my-session".to_string())
        );
    }

    #[test]
    fn parse_focused_session_focused_only() {
        assert_eq!(
            parse_focused_session("focused dev"),
            Some("dev".to_string())
        );
    }

    #[test]
    fn parse_focused_session_not_focused() {
        assert_eq!(
            parse_focused_session("attached,UTF-8 my-session"),
            None
        );
    }

    #[test]
    fn parse_focused_session_empty_line() {
        assert_eq!(parse_focused_session(""), None);
    }

    #[test]
    fn parse_focused_session_no_session_field() {
        assert_eq!(parse_focused_session("focused"), None);
    }

    #[test]
    fn parse_focused_session_session_with_special_chars() {
        assert_eq!(
            parse_focused_session("attached,focused work-2"),
            Some("work-2".to_string())
        );
    }

    // --- is_focused_claude_line with session ---

    #[test]
    fn focused_claude_pane_matching_session() {
        assert!(is_focused_claude_line_in_session(
            "my-session 1 1 claude",
            "my-session"
        ));
    }

    #[test]
    fn focused_claude_pane_different_session() {
        assert!(!is_focused_claude_line_in_session(
            "other-session 1 1 claude",
            "my-session"
        ));
    }

    #[test]
    fn inactive_pane_matching_session() {
        assert!(!is_focused_claude_line_in_session(
            "my-session 0 1 claude",
            "my-session"
        ));
    }

    #[test]
    fn inactive_window_matching_session() {
        assert!(!is_focused_claude_line_in_session(
            "my-session 1 0 claude",
            "my-session"
        ));
    }

    #[test]
    fn non_claude_matching_session() {
        assert!(!is_focused_claude_line_in_session(
            "my-session 1 1 vim",
            "my-session"
        ));
    }

    #[test]
    fn malformed_session_line() {
        assert!(!is_focused_claude_line_in_session("my-session 1 1", "my-session"));
    }

    #[test]
    fn empty_session_line() {
        assert!(!is_focused_claude_line_in_session("", "my-session"));
    }
}

pub fn capture_pane(pane_id: &str) -> String {
    let output = Command::new("tmux")
        .args(["capture-pane", "-p", "-t", pane_id])
        .output();

    match output {
        Ok(out) => String::from_utf8_lossy(&out.stdout).into_owned(),
        Err(e) => {
            eprintln!("[claudeye] tmux capture-pane failed for {pane_id}: {e}");
            String::new()
        }
    }
}
