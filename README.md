# claudeye

A transparent always-on-top overlay that monitors Claude Code sessions running in tmux panes.

## What it does

claudeye periodically polls all tmux panes, finds those running the `claude` command, and displays their status in a small floating window at the top center of your screen. The window is click-through, so it never interferes with your workflow.

Each session is shown as a mini Clawd (the Claude robot mascot) with a color-coded speech bubble indicating its current state and elapsed time:

| Label       | Color  | Meaning                                |
|-------------|--------|----------------------------------------|
| `Running`   | Green  | Claude is processing                   |
| `Approval`  | Yellow | Waiting for your approval              |
| `Idle`      | Gray   | Prompt shown, waiting for input        |

## Requirements

- **tmux** — must be installed and running. claudeye uses `tmux list-panes` and `tmux capture-pane` to discover and read Claude sessions.
- **Claude Code** (`claude` CLI) — sessions must be running inside tmux panes. claudeye identifies panes where the current command is `claude`.
- **Rust toolchain** — install via [rustup](https://rustup.rs/)
- **Linux with X11** — tested on X11; Wayland is untested. Standard X11 libraries are required (typically pre-installed).

## Installation

```sh
cargo install claudeye
```

## Usage

```sh
claudeye [--compact] [--position <POSITION>] [--alert-on-stale]
claudeye picker
```

### Overlay mode

```sh
# Run with default settings (show all sessions at top center)
claudeye

# Position the overlay at a specific screen location
claudeye --position bottom-right
claudeye -p top-left
```

| Option | Description |
|--------|-------------|
| `--compact` | Show state summary with session counts |
| `--position`, `-p` | Window position on screen (default: `top-center`). Values: `top-left`, `top-center`, `top-right`, `middle-left`, `middle-center`, `middle-right`, `bottom-left`, `bottom-center`, `bottom-right` |
| `--alert-on-stale` | Move overlay to screen center when any session stays in Approval/Idle for 5–15 seconds, then return to its configured position |

![Overlay mode][1]

#### Compact mode

```sh
# Run in compact mode (state summary with session counts)
claudeye --compact
```

In compact mode, sessions are grouped by state and displayed as a single row of robot + bubble pairs. Each bubble shows the number of sessions in that state. States with no sessions are hidden.

![Compact mode][3]

### Picker mode

```sh
claudeye picker
```

An interactive TUI session picker. Use it to quickly switch to any Claude session:

| Key | Action |
|-----|--------|
| `1`–`9` | Jump directly to that session |
| `j` / `↓` | Move selection down |
| `k` / `↑` | Move selection up |
| `Enter` | Switch to selected session |
| `q` / `Esc` | Quit without switching |

Sessions beyond the 9th are accessible via `j`/`k` navigation.

![Picker mode][2]

## Auto-quiet mode

When the focused tmux pane is running Claude, claudeye automatically suppresses pulse animations, stale-idle highlights, and `--alert-on-stale` centering so the overlay stays unobtrusive while you're actively working with Claude.

This feature relies on tmux focus events. Enable them in your `~/.tmux.conf`:

```
set-option -g focus-events on
```

When `focus-events` is off (or the terminal doesn't support it), quiet mode is simply disabled and the overlay behaves as before.

## How it works

### Overlay mode

1. A background thread periodically polls `tmux list-panes -a` to find all panes running `claude`.
2. For each matching pane, it runs `tmux capture-pane` to read the terminal content.
3. The terminal content is analyzed with regex patterns to determine Claude's current state (working, waiting for approval, idle, etc.).
4. The egui overlay window updates to reflect the latest state of each session.

The overlay window is:
- Positioned at the top center of your primary monitor by default (configurable with `--position`)
- Dynamically sized to fit session content
- Always on top of other windows
- Click-through (mouse events pass through to windows below)
- Fully transparent background

### Picker mode

1. Runs `tmux list-panes -a` once to collect all panes running `claude`.
2. Captures each pane's content to determine its current state.
3. Displays the sessions in a ratatui TUI list with state indicators and numeric labels.
4. On selection, runs `tmux switch-client` to jump to the chosen pane.

## Development

```sh
cargo test    # Run tests
cargo clippy  # Run linter
cargo fmt     # Format code
cargo run     # Run in development mode
```

[1]: https://raw.githubusercontent.com/maedana/claudeye/main/demo/demo.png
[2]: https://raw.githubusercontent.com/maedana/claudeye/main/demo/demo2.gif
[3]: https://raw.githubusercontent.com/maedana/claudeye/main/demo/demo3.png
