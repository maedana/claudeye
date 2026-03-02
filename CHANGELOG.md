# Changelog

## [Unreleased]

## [0.6.0] - 2026-03-03

### Added

- Mouse hover transparency: overlay fades out when the cursor hovers over it, then restores opacity when the cursor moves away
  - macOS: uses CoreGraphics for cursor position detection
  - Linux (X11): uses `XQueryPointer` via `x11-dl` for cursor position detection
  - Wayland / other platforms: hover transparency is disabled (graceful degradation)
- TTL-based cache refresh for claude version detection to reduce filesystem polling

## [0.5.2] - 2026-03-02

### Changed

- Add keywords and categories to Cargo.toml for crates.io discoverability

## [0.5.1] - 2026-02-28

### Changed

- Extract `tmux-claude-state` as a standalone crate published to crates.io
- Add API doc comments to all public items in `tmux-claude-state`
- Include README in both crate packages for crates.io

### Added

- `capture_pane_with_ansi` function to capture tmux pane content with ANSI escape sequences
- Auto-quiet mode: suppress pulse animations and stale alerts when the focused tmux pane is running Claude
  - Detects OS-level terminal focus via tmux `focus-events` and active pane command
  - Requires `set-option -g focus-events on` in tmux config for full support

## [0.4.0] - 2026-02-25

### Added

- `--alert-on-stale` option to move overlay to screen center when a session needs attention
  - Triggers for Approval/Idle sessions between 5–15 seconds of staleness
  - After 15 seconds the overlay returns to its configured position

### Changed

- `--compact` mode now shows state summary instead of cycling individual sessions
  - Displays robot + speech bubble groups side by side (up to 3: Running / Approval / Idle)
  - Each bubble shows the session count for that state; states with 0 sessions are hidden
  - Repaint interval optimized: 100ms only when Approval pulse is active, 1s otherwise

## [0.3.0] - 2026-02-24

### Added

- `--position` (`-p`) option to place overlay at any of 9 screen positions (default: `top-center`)
- Pulse animation on speech bubble border for WaitingForApproval state (stroke width pulses 1.0–3.0)
- Elapsed time display per session in speech bubble (e.g. `[Running] 33s`)

### Changed

- Overlay window width now adjusts dynamically to fit session content
- WaitingForApproval pulse now constant full intensity instead of gradually decaying
- Robot head uses fixed state color per state instead of blinking
- Removed Idle pulse animation (stroke width is now constant)
- Removed unused `last_updated` field from `ClaudeSession`

### Fixed

- Fix idle state misdetected as Approval when vim mode status lines (`-- INSERT --`, `[Model] Context: XX%`) appear below the prompt
  - These footer lines caused `is_claude_prompt_line` to bail early, falling through to match stale WAITING_PATTERNS (e.g. `Proceed?`) in pane history.

## [0.2.1] - 2026-02-24

### Fixed

- Detect versioned `claude` binary names in tmux pane commands on macOS
  - On macOS, the `claude` binary is a symlink to a versioned path under `~/.local/share/claude/versions/`, causing tmux to report the version number as the command name instead of `claude`. Version names are now resolved at startup so claude sessions are correctly detected.

### Changed

- Remove unused `x11rb` dependency

## [0.2.0] - 2026-02-23

### Added

- `picker` subcommand — interactive TUI session picker using ratatui/crossterm
  - Number keys `1`–`9` jump directly to the corresponding session
  - `j`/`k` (or arrow keys) for navigation, `Enter` to switch, `q`/`Esc` to quit
  - `tmux switch-client` integration to jump to the selected pane
- Clawd robot mascot art rendered per session in the overlay
  - Robot head animates (color blinks) while Claude is working or waiting for approval
- `--compact` flag — show one session at a time, cycling every second

### Changed

- Overlay background is now fully transparent (removed `--opacity` option)
- Session info displayed as a speech bubble with color-coded border
- State labels renamed: `WORKING` → `Running`, `APPROVAL` → `Approval`, `IDLE` → `Idle`
- Overlay positioned 2px from the top of the screen

## [0.1.0] - 2026-02-23

### Added

- Transparent always-on-top overlay window showing Claude session states
- `--opacity` option to control overlay background transparency (default: `0.24`)
- Overlay window positioned at top center of primary monitor on startup
- Click-through overlay (mouse events pass through to windows below)
- State detection via regex analysis of captured tmux pane content
- MIT License

### Changed

- Project renamed from `ccmonitor` to `claudeye`
- Overlay window height adjusts dynamically per session row count

[Unreleased]: https://github.com/maedana/claudeye/compare/v0.6.0...HEAD
[0.6.0]: https://github.com/maedana/claudeye/compare/v0.5.2...v0.6.0
[0.5.2]: https://github.com/maedana/claudeye/compare/v0.5.1...v0.5.2
[0.5.1]: https://github.com/maedana/claudeye/compare/v0.4.0...v0.5.1
[0.4.0]: https://github.com/maedana/claudeye/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/maedana/claudeye/compare/v0.2.1...v0.3.0
[0.2.1]: https://github.com/maedana/claudeye/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/maedana/claudeye/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/maedana/claudeye/releases/tag/v0.1.0
