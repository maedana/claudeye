use tmux_claude_state::tmux::{parse_pane_line, read_version_entries, refresh_version_cache};

#[test]
fn parse_valid_pane_line_claude() {
    let line = "main:0.1 12345 /home/user/projects/myapp claude";
    let result = parse_pane_line(line);
    assert!(result.is_some());
    let pane = result.unwrap();
    assert_eq!(pane.id, "main:0.1");
    assert_eq!(pane.pid, 12345);
    assert_eq!(pane.cwd, "/home/user/projects/myapp");
    assert_eq!(pane.project_name, "myapp");
}

#[test]
fn parse_pane_line_non_claude_command_returns_none() {
    let line = "main:0.0 9999 /home/user bash";
    let result = parse_pane_line(line);
    assert!(result.is_none());
}

#[test]
fn parse_pane_line_insufficient_fields_returns_none() {
    let line = "main:0.1 12345 /home/user";
    let result = parse_pane_line(line);
    assert!(result.is_none());
}

#[test]
fn parse_pane_line_invalid_pid_returns_none() {
    let line = "main:0.1 notanumber /home/user claude";
    let result = parse_pane_line(line);
    assert!(result.is_none());
}

#[test]
fn project_name_is_basename_of_cwd() {
    let line = "work:2.3 54321 /home/maedana/tmp/claudeye claude";
    let result = parse_pane_line(line);
    assert!(result.is_some());
    let pane = result.unwrap();
    assert_eq!(pane.project_name, "claudeye");
}

#[test]
fn non_worktree_has_no_worktree_name() {
    let line = "main:0.1 12345 /home/user/projects/myapp claude";
    let pane = parse_pane_line(line).unwrap();
    assert_eq!(pane.worktree_name, None);
}

#[test]
fn worktree_resolves_original_project_name() {
    // Create a fake git worktree structure
    let dir = std::env::temp_dir().join(format!("tmux_wt_test_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);

    // Original repo at dir/original-repo/.git (directory)
    let original_git = dir.join("original-repo").join(".git");
    std::fs::create_dir_all(&original_git).unwrap();

    // Worktree at dir/my-worktree/.git (file pointing to original)
    let wt_dir = dir.join("my-worktree");
    std::fs::create_dir_all(&wt_dir).unwrap();
    std::fs::write(
        wt_dir.join(".git"),
        format!("gitdir: {}/.git/worktrees/my-worktree", dir.join("original-repo").display()),
    )
    .unwrap();

    let line = format!("main:0.1 12345 {} claude", wt_dir.display());
    let pane = parse_pane_line(&line).unwrap();
    assert_eq!(pane.project_name, "original-repo");
    assert_eq!(pane.worktree_name, Some("my-worktree".to_string()));

    let _ = std::fs::remove_dir_all(&dir);
}

/// On macOS, tmux reports the resolved symlink target name (e.g. "2.1.50")
/// instead of "claude". Multiple versions may coexist in the versions
/// directory, so all of them should be detected.
///
/// This test requires `claude` to be installed as a symlink to a versioned
/// binary. It is skipped on CI or Linux where the symlink does not exist.
#[test]
fn parse_pane_line_any_installed_claude_version_detected() {
    let output = std::process::Command::new("which").arg("claude").output();
    let Ok(out) = output else { return };
    if !out.status.success() {
        return;
    }
    let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let Ok(target) = std::fs::read_link(&path) else {
        return;
    };
    let Some(versions_dir) = target.parent() else {
        return;
    };
    let Ok(rd) = std::fs::read_dir(versions_dir) else {
        return;
    };

    let entries: Vec<_> = rd
        .filter_map(|e| e.ok())
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();

    for name in &entries {
        let line = format!("main:1.1 74988 /Users/user/myapp {name}");
        assert!(
            parse_pane_line(&line).is_some(),
            "should detect installed claude version '{name}'"
        );
    }
}

#[test]
fn parse_pane_line_non_existent_version_not_detected() {
    let line = "main:1.1 74988 /Users/user/myapp 99.99.99";
    assert!(parse_pane_line(line).is_none());
}

#[test]
fn read_version_entries_returns_filenames_from_directory() {
    let dir = std::env::temp_dir().join(format!(
        "claudeye_test_versions_{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("2.1.50"), "").unwrap();
    std::fs::write(dir.join("2.2.0"), "").unwrap();

    let entries = read_version_entries(&dir).unwrap();
    assert!(entries.contains("2.1.50"));
    assert!(entries.contains("2.2.0"));
    assert_eq!(entries.len(), 2);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn read_version_entries_returns_none_for_missing_directory() {
    let dir = std::env::temp_dir().join(format!(
        "claudeye_test_nonexistent_{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    assert!(read_version_entries(&dir).is_none());
}

/// Verifies that refresh_version_cache causes the cache to re-read
/// the versions directory, so all currently installed versions are detected.
#[test]
fn refresh_version_cache_detects_installed_versions() {
    let output = std::process::Command::new("which")
        .arg("claude")
        .output();
    let Ok(out) = output else { return };
    if !out.status.success() {
        return;
    }
    let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let Ok(target) = std::fs::read_link(&path) else { return };
    let Some(versions_dir) = target.parent() else { return };

    refresh_version_cache();

    let Ok(rd) = std::fs::read_dir(versions_dir) else { return };
    let entries: Vec<_> = rd
        .filter_map(|e| e.ok())
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    for name in &entries {
        let line = format!("main:1.1 74988 /Users/user/myapp {name}");
        assert!(
            parse_pane_line(&line).is_some(),
            "after refresh, should detect version '{name}'"
        );
    }
}
