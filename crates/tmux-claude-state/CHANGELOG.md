# Changelog

## [Unreleased]

## [0.4.0] - 2026-03-10

### Added

- `worktree_name` field on `PaneInfo` to expose the worktree directory basename
- `project_name` now resolves to the original repository name for git worktree directories

## [0.3.1] - 2026-03-09

### Fixed

- Add ASCII asterisk (`*`) to spinner symbol regex, fixing intermittent missed Running state detection

## [0.3.0] - 2026-03-07

### Added

- `permission_mode` field on `ClaudeSession` to expose detected permission mode during polling

## [0.2.0] - 2026-03-07

### Added

- `PermissionMode` enum and `detect_permission_mode()` for detecting Claude Code permission modes
  - `AskBeforeEdits` (default), `EditAutomatically`, `PlanMode`
  - Scans only the tmux status bar area (below the last separator) to avoid false positives from conversation history
  - `display_label()` returns human-readable names (`"Ask"`, `"Auto Edit"`, `"Plan"`)

## [0.1.4] - 2026-03-05

### Added

- Detection patterns for additional Claude Code running states (vim mode, file changes status, esc to interrupt variants)

## [0.1.3] - 2026-03-03

### Added

- TTL-based cache refresh for claude version detection to reduce filesystem polling
- CHANGELOG

## [0.1.2] - 2026-03-02

### Changed

- Add keywords and categories to Cargo.toml for crates.io discoverability

## [0.1.1] - 2026-02-28

### Added

- `capture_pane_with_ansi` function to capture tmux pane content with ANSI escape sequences
- API doc comments for all public items
- README for crates.io

## [0.1.0] - 2026-02-28

### Added

- Initial release extracted from `claudeye` as a standalone crate
- Claude session state detection via regex analysis of tmux pane content
- Tmux helpers: `list_panes`, `capture_pane`, `switch_client`
- Version cache for detecting claude binary names in tmux pane commands

[Unreleased]: https://github.com/maedana/claudeye/compare/tmux-claude-state-v0.4.0...HEAD
[0.4.0]: https://github.com/maedana/claudeye/compare/tmux-claude-state-v0.3.1...tmux-claude-state-v0.4.0
[0.3.1]: https://github.com/maedana/claudeye/compare/tmux-claude-state-v0.3.0...tmux-claude-state-v0.3.1
[0.3.0]: https://github.com/maedana/claudeye/compare/tmux-claude-state-v0.2.0...tmux-claude-state-v0.3.0
[0.2.0]: https://github.com/maedana/claudeye/compare/tmux-claude-state-v0.1.4...tmux-claude-state-v0.2.0
[0.1.4]: https://github.com/maedana/claudeye/compare/tmux-claude-state-v0.1.3...tmux-claude-state-v0.1.4
[0.1.3]: https://github.com/maedana/claudeye/compare/tmux-claude-state-v0.1.2...tmux-claude-state-v0.1.3
[0.1.2]: https://github.com/maedana/claudeye/compare/tmux-claude-state-v0.1.1...tmux-claude-state-v0.1.2
[0.1.1]: https://github.com/maedana/claudeye/compare/tmux-claude-state-v0.1.0...tmux-claude-state-v0.1.1
[0.1.0]: https://github.com/maedana/claudeye/releases/tag/tmux-claude-state-v0.1.0
