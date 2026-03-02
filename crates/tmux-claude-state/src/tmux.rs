use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// Metadata for a tmux pane running Claude Code.
#[derive(Debug, Clone)]
pub struct PaneInfo {
    /// Tmux pane identifier (e.g. `"my-session:0.1"`).
    pub id: String,
    /// PID of the pane's foreground process.
    #[allow(dead_code)]
    pub pid: u32,
    /// Current working directory of the pane.
    #[allow(dead_code)]
    pub cwd: String,
    /// Basename of [`cwd`](Self::cwd), used as a short project label.
    pub project_name: String,
}

/// List all tmux panes across every session that are running Claude Code.
pub fn list_claude_panes() -> Vec<PaneInfo> {
    let output = Command::new("tmux")
        .args([
            "list-panes",
            "-a",
            "-F",
            "#{session_name}:#{window_index}.#{pane_index} #{pane_pid} #{pane_current_path} #{pane_current_command}",
        ])
        .output();

    let version_names = claude_version_names();
    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            stdout
                .lines()
                .filter_map(|line| parse_pane_line_with_versions(line, &version_names))
                .collect()
        }
        Err(e) => {
            eprintln!("[claudeye] tmux list-panes failed: {e}");
            vec![]
        }
    }
}

/// Parse a tmux pane line, using the caller-provided version name set.
fn parse_pane_line_with_versions(line: &str, version_names: &HashSet<String>) -> Option<PaneInfo> {
    let parts: Vec<&str> = line.splitn(4, ' ').collect();
    if parts.len() < 4 {
        return None;
    }
    let id = parts[0].to_string();
    let pid: u32 = parts[1].parse().ok()?;
    let cwd = parts[2].to_string();
    let command = parts[3].trim();

    if !is_claude_command_with_versions(command, version_names) {
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

/// Parse a single line of `tmux list-panes` output into a [`PaneInfo`].
///
/// Returns `None` if the line is malformed or the pane is not running Claude.
pub fn parse_pane_line(line: &str) -> Option<PaneInfo> {
    let version_names = claude_version_names();
    parse_pane_line_with_versions(line, &version_names)
}

/// Switch the tmux client to the given pane via `tmux switch-client`.
pub fn switch_to_pane(pane_id: &str) {
    let result = Command::new("tmux")
        .args(["switch-client", "-t", pane_id])
        .output();
    if let Err(e) = result {
        eprintln!("[claudeye] tmux switch-client failed: {e}");
    }
}

fn is_claude_command_with_versions(command: &str, version_names: &HashSet<String>) -> bool {
    command == "claude" || version_names.contains(command)
}

/// Check whether the tmux pane command corresponds to a claude process.
fn is_claude_command(command: &str) -> bool {
    is_claude_command_with_versions(command, &claude_version_names())
}

const VERSION_CACHE_TTL: Duration = Duration::from_secs(30);

struct VersionCache {
    names: HashSet<String>,
    versions_dir: Option<PathBuf>,
    last_refresh: Instant,
}

fn version_cache() -> &'static Mutex<VersionCache> {
    static CACHE: OnceLock<Mutex<VersionCache>> = OnceLock::new();
    CACHE.get_or_init(|| {
        let (versions_dir, names) = init_version_cache();
        Mutex::new(VersionCache {
            names,
            versions_dir,
            last_refresh: Instant::now(),
        })
    })
}

/// On macOS, the `claude` binary is a symlink to a versioned path
/// (e.g. `~/.local/share/claude/versions/2.1.50`), so tmux resolves
/// the symlink and reports the version number as the command name.
/// The versions directory path is resolved once at startup (`which claude`),
/// while its contents are refreshed every 30 seconds so that new CLI
/// versions installed while claudeye is running are detected.
fn claude_version_names() -> HashSet<String> {
    let mut cache = version_cache().lock().unwrap_or_else(|e| e.into_inner());
    if cache.last_refresh.elapsed() >= VERSION_CACHE_TTL {
        reload_entries(&mut cache);
    }
    cache.names.clone()
}

/// Force-refresh the version cache regardless of TTL.
pub fn refresh_version_cache() {
    let mut cache = version_cache().lock().unwrap_or_else(|e| e.into_inner());
    reload_entries(&mut cache);
}

fn reload_entries(cache: &mut VersionCache) {
    if let Some(ref dir) = cache.versions_dir
        && let Some(entries) = read_version_entries(dir)
    {
        cache.names = entries;
    }
    cache.last_refresh = Instant::now();
}

fn init_version_cache() -> (Option<PathBuf>, HashSet<String>) {
    let Some(dir) = resolve_versions_dir() else {
        return (None, HashSet::new());
    };
    let names = read_version_entries(&dir).unwrap_or_default();
    (Some(dir), names)
}

fn resolve_versions_dir() -> Option<PathBuf> {
    let output = Command::new("which").arg("claude").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let target = std::fs::read_link(&path).ok()?;
    Some(target.parent()?.to_path_buf())
}

/// Read version entries from a directory, returning filenames as a set.
pub fn read_version_entries(dir: &Path) -> Option<HashSet<String>> {
    let entries = std::fs::read_dir(dir)
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
    parts[0] == session && parts[1] == "1" && parts[2] == "1" && is_claude_command(parts[3].trim())
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
        assert_eq!(parse_focused_session("attached,UTF-8 my-session"), None);
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
        assert!(!is_focused_claude_line_in_session(
            "my-session 1 1",
            "my-session"
        ));
    }

    #[test]
    fn empty_session_line() {
        assert!(!is_focused_claude_line_in_session("", "my-session"));
    }

    // --- capture_pane_args ---

    #[test]
    fn capture_pane_args_plain_text() {
        let args = capture_pane_args("%1", false);
        assert_eq!(args, vec!["capture-pane", "-p", "-t", "%1"]);
    }

    #[test]
    fn capture_pane_args_with_ansi() {
        let args = capture_pane_args("%1", true);
        assert_eq!(args, vec!["capture-pane", "-p", "-e", "-t", "%1"]);
    }
}

fn capture_pane_args(pane_id: &str, ansi: bool) -> Vec<&str> {
    let mut args = vec!["capture-pane", "-p"];
    if ansi {
        args.push("-e");
    }
    args.push("-t");
    args.push(pane_id);
    args
}

fn run_capture_pane(pane_id: &str, ansi: bool) -> String {
    let args = capture_pane_args(pane_id, ansi);
    let output = Command::new("tmux").args(&args).output();

    match output {
        Ok(out) => String::from_utf8_lossy(&out.stdout).into_owned(),
        Err(e) => {
            eprintln!("[claudeye] tmux capture-pane failed for {pane_id}: {e}");
            String::new()
        }
    }
}

/// Capture the visible content of a tmux pane as plain text.
///
/// Equivalent to `tmux capture-pane -p -t <pane_id>`.
pub fn capture_pane(pane_id: &str) -> String {
    run_capture_pane(pane_id, false)
}

/// Capture the visible content of a tmux pane with ANSI escape sequences.
///
/// Equivalent to `tmux capture-pane -p -e -t <pane_id>`.
pub fn capture_pane_with_ansi(pane_id: &str) -> String {
    run_capture_pane(pane_id, true)
}
