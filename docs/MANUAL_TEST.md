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
