use tmux_claude_state::claude_state::{ClaudeState, PermissionMode, detect_permission_mode, detect_state};

// Ported from tcmux status_claude_test.go

#[test]
fn idle_with_prompt_only() {
    let content = "Some output\n\
───────────────────────────────────────\n\
❯\n\
───────────────────────────────────────";
    assert_eq!(detect_state(content), ClaudeState::Idle);
}

#[test]
fn idle_with_completion_suggestion() {
    let content = "Some output\n\
───────────────────────────────────────\n\
❯ Try \"edit file.go to...\"\n\
───────────────────────────────────────";
    assert_eq!(detect_state(content), ClaudeState::Idle);
}

#[test]
fn running_with_clauding() {
    let content = "Some output\n\
✢ Clauding… (esc to interrupt · 1m 45s · ↓ 1.2k tokens)";
    assert_eq!(detect_state(content), ClaudeState::Working);
}

#[test]
fn running_with_moseying() {
    let content = "Some output\n\
✽ Moseying… (esc to interrupt · 30s · ↓ 500 tokens)";
    assert_eq!(detect_state(content), ClaudeState::Working);
}

#[test]
fn running_with_thinking() {
    let content = "Some output\n\
✶ Thinking… (esc to interrupt · 2m 10s · ↓ 3k tokens)";
    assert_eq!(detect_state(content), ClaudeState::Working);
}

#[test]
fn running_with_time_first_format() {
    let content = "Some output\n\
✢ Reticulating… (1m 52s · ↓ 11.5k tokens · thought for 7s)";
    assert_eq!(detect_state(content), ClaudeState::Working);
}

#[test]
fn running_with_esc_to_interrupt_at_end_of_status_line() {
    let content = "Some output\n\
✶ Proofing… (thinking)\n\
───────────────────────────────────────\n\
❯\n\
───────────────────────────────────────\n\
  4 files +20 -0 · esc to interrupt";
    assert_eq!(detect_state(content), ClaudeState::Working);
}

#[test]
fn running_with_jitterbugging_middle_dot() {
    let content = "Some output\n\
· Jitterbugging… (esc to interrupt · 1m 8s · ↓ 3.6k tokens · thinking)";
    assert_eq!(detect_state(content), ClaudeState::Working);
}

#[test]
fn running_fallback_ctrl_c_to_interrupt() {
    let content = "Some output\n\
✻ Thinking… (ctrl+c to interrupt)";
    assert_eq!(detect_state(content), ClaudeState::Working);
}

#[test]
fn running_fallback_esc_to_interrupt() {
    let content = "Some output\n\
✻ Processing… (esc to interrupt)";
    assert_eq!(detect_state(content), ClaudeState::Working);
}

#[test]
fn not_running_when_text_mentions_esc_to_interrupt_in_quotes() {
    let content = "Some output about \"esc to interrupt\" in quotes\n\
❯ ";
    assert_eq!(detect_state(content), ClaudeState::Idle);
}

#[test]
fn not_running_when_indented_status_line_quoted_text() {
    // \n\ を使うと行継続でインデントが消えるため、\n を直接文字列に埋め込む
    let content = concat!(
        "⏺ 現在の内容は：\n",
        "  ✻ Galloping… (esc to interrupt · 1m 19s · ↓ 5.9k tokens · thinking)\n",
        "\n",
        "✻ Cooked for 1m 29s\n",
        "\n",
        "───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────\n",
        "❯\n",
        "───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────",
    );
    assert_eq!(detect_state(content), ClaudeState::Idle);
}

#[test]
fn waiting_with_permission_prompt_yes_allow_once() {
    let content = "Some output\n\
Yes, allow once\n\
Yes, allow always";
    assert_eq!(detect_state(content), ClaudeState::WaitingForApproval);
}

#[test]
fn waiting_with_confirmation_prompt() {
    let content = "Some output\n\
Continue? (Y/n)";
    assert_eq!(detect_state(content), ClaudeState::WaitingForApproval);
}

#[test]
fn idle_after_task_completion() {
    let content = "Some output\n\
✻ Cooked for 43s\n\
───────────────────────────────────────\n\
❯ ";
    assert_eq!(detect_state(content), ClaudeState::Idle);
}

#[test]
fn idle_after_task_completion_with_file_changes() {
    let content = "⏺ Window 10 is now Idle (accept edits), Window 11 shows Running (4m 48s) correctly.\n\
\n\
✻ Sautéed for 2m 55s\n\
\n\
! make install\n\
  ⎿  go install completed\n\
\n\
───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────\n\
❯\n\
───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────\n\
  4 files +73 -3";
    assert_eq!(detect_state(content), ClaudeState::Idle);
}

#[test]
fn idle_with_plan_mode() {
    let content = "Some output\n\
⏸ plan mode on\n\
───────────────────────────────────────\n\
❯\n\
───────────────────────────────────────";
    assert_eq!(detect_state(content), ClaudeState::Idle);
}

#[test]
fn idle_with_accept_edits() {
    let content = "Some output\n\
⏵⏵ accept edits on\n\
───────────────────────────────────────\n\
❯\n\
───────────────────────────────────────";
    assert_eq!(detect_state(content), ClaudeState::Idle);
}

#[test]
fn waiting_interview_mode() {
    let content = "  3. ドキュメントのレビュー\n\
  4. Issue の修正\n\
  5. Type something.\n\
  Chat about this\n\
  Skip interview and plan immediately\n\
Enter to select · ↑/↓ to navigate · Esc to cancel";
    assert_eq!(detect_state(content), ClaudeState::WaitingForApproval);
}

#[test]
fn running_with_japanese_status_and_todo_list() {
    let content = "· importパスを更新中… (esc to interrupt · ctrl+t to hide todos · 1m 32s · ↑ 3.4k tokens · thinking)\n\
  ⎿  ☒ go.mod のモジュール名を変更\n\
     ☐ 全ファイルのimportパスを更新\n\
     ☐ README.md を更新\n\
     ☐ その他の参照を更新\n\
     ☐ ビルドとテストの確認\n\
\n\
───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────\n\
❯\n\
───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────";
    assert_eq!(detect_state(content), ClaudeState::Working);
}

#[test]
fn idle_with_trust_dialog_overlay() {
    let content = " /home/user/projects/myapp\n\
\n\
 Claude Code may read, write, or execute files contained in this directory.\n\
\n\
 Execution allowed by:\n\
\n\
   • .claude/settings.local.json\n\
\n\
 Learn more\n\
\n\
 ❯ 1. Yes, proceed\n\
   2. No, exit\n\
\n\
 Enter to confirm · Esc to cancel\n\
\n\
╭─── Claude Code v2.1.15 ───╮\n\
│  Welcome back user!       │\n\
╰───────────────────────────╯\n\
\n\
───────────────────────────────────────\n\
❯ Try \"fix typecheck errors\"\n\
───────────────────────────────────────\n\
  ? for shortcuts";
    assert_eq!(detect_state(content), ClaudeState::Idle);
}

#[test]
fn idle_with_file_changes_status_line() {
    let content = "⏺ Some output about \"Do you want to proceed?\"\n\
\n\
✻ Churned for 3m 5s\n\
\n\
! make install\n\
  ⎿  go install completed\n\
\n\
───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────\n\
❯\n\
───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────\n\
  4 files +42 -0";
    assert_eq!(detect_state(content), ClaudeState::Idle);
}

#[test]
fn waiting_bash_command_confirmation_dialog() {
    let content = "⏺ Bash(gh issue view 123 --repo owner/repo 2>/dev/null || echo \"Issue #123 not found or closed\")\n\
  ⎿  title:     Fix bug in parser\n\
     state:     CLOSED\n\
     author:    contributor\n\
     … +22 lines (ctrl+o to expand)\n\
\n\
⏺ Bash(grep --help 2>/dev/null | head -10)\n\
  ⎿  Running…\n\
\n\
───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────\n\
 Bash command\n\
\n\
   grep --help 2>/dev/null | head -10\n\
   Check grep help\n\
\n\
 Do you want to proceed?\n\
 ❯ 1. Yes\n\
   2. Yes, and don't ask again for grep commands in /home/user/projects/myapp\n\
   3. No\n\
\n\
 Esc to cancel · Tab to amend · ctrl+e to explain";
    assert_eq!(detect_state(content), ClaudeState::WaitingForApproval);
}

#[test]
fn running_with_action_text_containing_spaces() {
    let content = "      219 + export function createUserHandler(): UserHandler<UserArgs> {\n\
      220 +   return {\n\
      221 +     kind: \"createUser\",\n\
      222 +     __args: {} as UserArgs,\n\
      223 +   };\n\
      224 + }\n\
\n\
✶ Adding handler types and functions to handlers.ts… (ctrl+c to interrupt · ctrl+t to hide todos · 3m 27s · ↑ 11.0k tokens)\n\
  ⎿  ☐ Add handler types and functions to handlers.ts\n\
     ☐ Update Handler type in index.ts\n\
     ☐ Add Zod schemas to schema.ts\n\
     ☐ Export types from types.ts\n\
     ☐ Add conversion logic to cli.ts\n\
     ☐ Run typecheck, test, and build\n\
\n\
───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────\n\
❯\n\
───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────\n\
  ⏵⏵ accept edits on (shift+tab to cycle)";
    assert_eq!(detect_state(content), ClaudeState::Working);
}

#[test]
fn running_with_spinning_and_plan_mode_ctrl_c_without_time() {
    let content = "⏺ Understanding the feature request. First, let me check the documentation and current implementation.\n\
\n\
  Explore(Explore handler implementation)\n\
  ⎿  Found 5 files\n\
     Found 18 files\n\
     Read(packages/sdk/src/cli/apply/services/handler.ts)\n\
     +27 more tool uses (ctrl+o to expand)\n\
\n\
⏺ Fetch(https://example.com/docs/guides/handlers)\n\
  ⎿  Received 51.6KB (200 OK)\n\
     ctrl+b ctrl+b (twice) to run in background\n\
\n\
✻ Spinning… (ctrl+c to interrupt)\n\
  ⎿  Tip: Type 'ultrathink' in your message to enable thinking for just that turn\n\
\n\
───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────\n\
❯\n\
───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────\n\
  ⏸ plan mode on (shift+tab to cycle)";
    assert_eq!(detect_state(content), ClaudeState::Working);
}

#[test]
fn running_with_crystallizing_and_accept_edits_mode() {
    let content = "✶ Crystallizing… (esc to interrupt · ctrl+t to hide tasks · 52s · ↓ 574 tokens)\n\
  ⎿  ✔ Add dependency to go.mod\n\
     ◻ Add configuration setting to config.go\n\
     ◻ Update library usage in parser.go\n\
     ◻ Pass format option to handler\n\
     ◻ Add and update tests\n\
     ◻ Run go test to verify\n\
\n\
───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────\n\
❯\n\
───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────\n\
  ⏵⏵ accept edits on (shift+Tab to cycle)";
    assert_eq!(detect_state(content), ClaudeState::Working);
}

#[test]
fn running_with_esc_to_interrupt_followed_by_ctrl_t_to_hide_tasks() {
    let content = "⏺ Some output here.\n\
\n\
✻ Cooked for 40s\n\
\n\
❯ Previous user input\n\
\n\
⏺ Starting implementation.\n\
\n\
✳ Thinking…\n\
  ⎿  ◻ Task 1\n\
     ◻ Task 2\n\
     ◻ Task 3\n\
\n\
───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────\n\
❯\n\
───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────\n\
  some-command --help 2>/d… (running) · 2 files +0 -0 · esc to interrupt · ctrl+t to hide tasks";
    assert_eq!(detect_state(content), ClaudeState::Working);
}

// スクリーンショットで観測された「初期思考中」パターン
// タイマー未表示・esc to interrupt なしの (thinking) / (thought for Ns) ケース

#[test]
fn running_with_just_thinking_status() {
    // ✻ Doing… (thinking) — タイマーなし・初期思考段階
    let content = "Some output\n\
✻ Doing… (thinking)";
    assert_eq!(detect_state(content), ClaudeState::Working);
}

#[test]
fn running_with_thought_for_without_middle_dot() {
    // ✻ Seasoning… (thought for 2s) — ドット区切りなしの経過時間
    let content = "Some output\n\
✻ Seasoning… (thought for 2s)";
    assert_eq!(detect_state(content), ClaudeState::Working);
}

#[test]
fn running_with_asterisk_symbol() {
    // * (U+002A) is used as a spinner symbol in Claude Code status line
    let content = "Some output\n\
* Unfurling… (esc to interrupt · 10s · ↓ 500 tokens)";
    assert_eq!(detect_state(content), ClaudeState::Working);
}

#[test]
fn unknown_state_falls_back_to_idle() {
    let content = "Some random output\n\
without any recognizable pattern";
    assert_eq!(detect_state(content), ClaudeState::Idle);
}

#[test]
fn empty_content_falls_back_to_idle() {
    let content = "";
    assert_eq!(detect_state(content), ClaudeState::Idle);
}

#[test]
fn waiting_with_no_as_last_selected_option() {
    // ❯ No が末尾行の場合（カーソルが「No」にある確認ダイアログ）、WaitingForApproval を返すべき
    // is_claude_prompt_line が ❯ No をプロンプトと誤認してはならない
    let content = "Do you want to proceed?\n  Yes\n❯ No";
    assert_eq!(detect_state(content), ClaudeState::WaitingForApproval);
}

#[test]
fn idle_with_vim_mode_and_stale_waiting_pattern_in_history() {
    // When vim mode footer lines (-- INSERT --, [Model] Context: XX%) appear below the prompt
    // and a stale WAITING_PATTERN like "Proceed?" exists in pane history,
    // the state should be Idle, not misdetected as WaitingForApproval.
    let content = "\
❯ Proceed?\n\
  ⎿  Interrupted · What should Claude do instead?\n\
\n\
──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────\n\
❯ \n\
──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────\n\
  [Opus 4.6] Context: 0%";
    assert_eq!(detect_state(content), ClaudeState::Idle);
}

// PermissionMode detection tests

#[test]
fn permission_mode_edit_automatically() {
    let content = "Some output\n\
───────────────────────────────────────\n\
❯\n\
───────────────────────────────────────\n\
  ⏵⏵ accept edits on (shift+tab to cycle)";
    assert_eq!(
        detect_permission_mode(content),
        PermissionMode::EditAutomatically
    );
}

#[test]
fn permission_mode_plan_mode() {
    let content = "Some output\n\
───────────────────────────────────────\n\
❯\n\
───────────────────────────────────────\n\
  ⏸ plan mode on (shift+tab to cycle)";
    assert_eq!(detect_permission_mode(content), PermissionMode::PlanMode);
}

#[test]
fn permission_mode_ask_before_edits_default() {
    let content = "Some output\n\
───────────────────────────────────────\n\
❯\n\
───────────────────────────────────────";
    assert_eq!(
        detect_permission_mode(content),
        PermissionMode::AskBeforeEdits
    );
}

#[test]
fn permission_mode_not_triggered_by_conversation_history() {
    // "accept edits on" in conversation history (not status bar) should not trigger detection
    let content = "⏺ The status bar shows ⏵⏵ accept edits on when edit mode is active.\n\
\n\
✻ Cooked for 43s\n\
───────────────────────────────────────\n\
❯ \n\
───────────────────────────────────────";
    assert_eq!(
        detect_permission_mode(content),
        PermissionMode::AskBeforeEdits
    );
}

#[test]
fn permission_mode_display_labels() {
    assert_eq!(PermissionMode::AskBeforeEdits.display_label(), "Ask");
    assert_eq!(
        PermissionMode::EditAutomatically.display_label(),
        "Auto Edit"
    );
    assert_eq!(PermissionMode::PlanMode.display_label(), "Plan");
}
