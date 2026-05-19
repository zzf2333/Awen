# Manual Test Procedures

These tests require a real terminal environment and cannot be automated.
All other test scenarios are covered by `cargo test` and `zsh tests/zsh_smoke_test.zsh`.

## Prerequisites

- macOS or Linux with zsh
- Awen installed and running (`awen start`)
- `source ~/.config/awen/awen.zsh` in `.zshrc`

## Test 1: Ghost Text Rendering

Verify ghost text renders correctly in each terminal emulator:

1. **Ghostty** — type `git ch`, confirm gray ghost text appears without artifacts
2. **Kitty** — same check; verify no flicker on rapid keystrokes
3. **WezTerm** — same check; verify ghost text clears on accept/dismiss
4. **macOS Terminal.app** — same check; verify ANSI color 242 is visible

## Test 2: tmux Visual

1. Start a tmux session
2. Source the plugin and start the daemon
3. Type partial commands — verify ghost text renders within the tmux pane
4. Verify no visual glitches when switching panes or resizing

## Test 3: Ctrl+Right Word Accept

Terminal emulators send different escape codes for Ctrl+Right.
Verify word-by-word acceptance works in each:

1. Get a multi-word ghost suggestion (e.g., type `git` to get `git checkout -b main`)
2. Press Ctrl+Right — one word should be accepted
3. If Ctrl+Right doesn't work, try Alt+Right (fallback binding)
4. Note which terminals need the alternate binding

## Test 4: Long-Running Usage

Use Awen normally for 30+ minutes to check:

1. No memory leaks (daemon RSS stays stable via `ps aux | grep awen`)
2. No socket errors after many suggestions
3. Ghost text remains responsive after hundreds of keystrokes
4. History suggestions improve over time as commands are recorded

## Test 5: Dropdown Menu Rendering

Requires jq installed for multi-suggestion parsing.

1. Type a partial command that generates multiple suggestions (e.g., `git c`)
2. Verify a dropdown menu appears below the input line
3. Each item shows command text and source tag (`[hist]`, `[spec]`, `[ai]`, `[fix]`)
4. First item is highlighted (standout/inverse video)
5. Ghost text on the input line matches the highlighted item
6. Footer line shows navigation hints: `↑↓ navigate  ⏎ confirm  esc dismiss`

## Test 6: Menu Navigation

1. With menu visible, press Down arrow — highlight moves to next item, ghost text updates
2. Press Down past the last item — wraps to first item
3. Press Up arrow — highlight moves up
4. Press Up past the first item — wraps to last item
5. When more than 5 items exist, verify scroll indicators (↑/↓) appear in footer

## Test 7: Menu Accept (Enter)

1. With menu visible, navigate to a specific item
2. Press Enter — the selected command fills BUFFER but does NOT execute
3. Menu disappears, ghost text clears
4. Press Enter again to execute the command (normal zsh behavior)

## Test 8: Menu Accept (Right Arrow)

1. With menu visible, press Right arrow — selected item fills BUFFER
2. Same behavior as Enter: fills but does not execute

## Test 9: Menu Dismiss

1. With menu visible, press Escape — menu and ghost text disappear
2. With menu visible, press Shift+Tab — same behavior as Escape

## Test 10: Menu Reset on Input

1. With menu visible showing suggestions for `git c`
2. Type an additional character (e.g., `h` to make `git ch`)
3. Verify menu resets and new suggestions appear based on updated input

## Test 11: Default Behavior Without Menu

1. When no menu is showing (0-1 suggestions), press Up/Down — normal zsh history navigation
2. When no menu is showing, press Enter — normal command execution
3. Single suggestion still renders as ghost text only (no menu)
4. With `AWEN_MENU_ENABLED=0`, multiple suggestions show only ghost text (first item), no menu
