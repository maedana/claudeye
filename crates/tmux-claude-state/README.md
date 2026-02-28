# tmux-claude-state

Detect and monitor [Claude Code](https://docs.anthropic.com/en/docs/claude-code) session states inside tmux panes.

## Features

- **State detection** — parse captured tmux pane content and classify it as `Working`, `WaitingForApproval`, or `Idle`.
- **Tmux interaction** — list Claude panes, capture their content (with or without ANSI escape sequences), and switch the client to a pane.
- **Background polling** — continuously monitor all Claude sessions and expose the latest state through a shared `MonitorState`.

## Usage

```rust
use tmux_claude_state::tmux;
use tmux_claude_state::claude_state::detect_state;

// List all tmux panes running Claude Code
let panes = tmux::list_claude_panes();

for pane in &panes {
    // Capture pane content and detect state
    let content = tmux::capture_pane(&pane.id);
    let state = detect_state(&content);
    println!("{} ({}): {:?}", pane.project_name, pane.id, state);
}
```

## License

MIT
