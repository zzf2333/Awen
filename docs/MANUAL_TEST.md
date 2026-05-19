# Manual Test Procedures

## Prerequisites

- macOS or Linux with zsh
- Awen installed via `install.sh` or manual build
- `source ~/.config/awen/awen.zsh` in `.zshrc`

## Test 1: Daemon Lifecycle

```bash
awen start      # Should print "awen daemon running"
awen status     # Should show running=true, pid, uptime
awen stop       # Should print "awen daemon stopped"
awen status     # Should show "daemon is not running"
```

## Test 2: Ghost Text (History)

1. Start daemon: `awen start`
2. Open a new shell (or source the plugin)
3. Run `cargo build` (or any command)
4. Start typing `car` — ghost text should appear in grey showing `cargo build`
5. Press `→` to accept the suggestion
6. Verify the full command is in the buffer

## Test 3: Ghost Text (Specs)

1. Type `git ch` — should show suggestions like `checkout`, `cherry-pick`
2. Type `docker r` — should show `run`
3. Type `npm i` — should show `install`

## Test 4: Accept Word

1. Get a ghost text suggestion (e.g., type `git` to get `git checkout`)
2. Press `Ctrl+→` to accept one word at a time
3. Verify only one word is accepted per press

## Test 5: Dismiss Suggestion

1. Get a ghost text suggestion
2. Press `Esc` — suggestion should disappear
3. Buffer should remain unchanged

## Test 6: Risk Warning

1. Type `rm -rf /` — should show inline warning about permanent deletion
2. Type `git push --force` — should show warning
3. Type `chmod 777 /etc` — should show warning
4. Type `ls -la` — should NOT show warning

## Test 7: Failure Recovery

1. Run a command that fails, e.g. `cargo build` in a project with a missing dependency
2. Start typing the fix command — ghost text should suggest the recovery
3. Verify the hint message appears above the prompt

## Test 8: AI Completion (if configured)

1. Ensure `ai.enabled = true` and API key is set
2. Type a 3+ character partial command
3. Wait for AI suggestion (may take up to 500ms)
4. Verify suggestion appears in ghost text

## Test 9: AI Disabled

1. Set `ai.enabled = false` in config
2. Restart daemon: `awen stop && awen start`
3. Verify history, specs, risk, failure all still work
4. No errors in log file

## Test 10: No API Key

1. Set `ai.enabled = true` but leave `api_key = ""`
2. Ensure `DEEPSEEK_API_KEY` is not set
3. Restart daemon
4. Verify no error messages shown to user
5. Local features work normally

## Test 11: stderr Capture (Experimental)

1. Set `export AWEN_CAPTURE_STDERR=1` before sourcing plugin
2. Run a failing command
3. Check that `$_AWEN_LAST_STDERR_FILE` has content
4. Verify TUI apps still work (try `vim`, `htop`)

## Test 12: Keybind Override Disable

1. Set `export AWEN_ENABLE_KEYBIND_OVERRIDE=0` before sourcing plugin
2. Source the plugin
3. Verify that default keybindings are not overridden
4. Ghost text should not appear on keystrokes

## Test 13: jq/socat Fallback

1. Temporarily rename `jq` binary (e.g., `sudo mv /usr/local/bin/jq /usr/local/bin/jq.bak`)
2. Open new shell, source plugin
3. Test ghost text — should work via fallback parser
4. Restore `jq`: `sudo mv /usr/local/bin/jq.bak /usr/local/bin/jq`

## Test 14: Uninstall

```bash
rm ~/.local/bin/awen
rm -rf ~/.config/awen
rm -rf ~/.local/share/awen
# Remove "source ~/.config/awen/awen.zsh" from ~/.zshrc
```

Verify: open new shell, no errors related to Awen.
