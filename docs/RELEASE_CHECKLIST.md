# Release Checklist

## Automated Checks

- [ ] `cargo fmt --check` — no formatting issues
- [ ] `cargo clippy -- -D warnings` — no warnings
- [ ] `cargo test` — all tests pass
- [ ] `shellcheck install.sh` — no issues

## Manual Tests

See [MANUAL_TEST.md](MANUAL_TEST.md) for detailed procedures.

- [ ] Fresh install via `install.sh` on clean zsh environment
- [ ] Daemon lifecycle: `awen start`, `awen status`, `awen stop`
- [ ] Ghost text appears for history matches (type a previously used command)
- [ ] Specs completion works for `git`, `docker`, `npm`, `cargo`
- [ ] Risk warning appears for `rm -rf /`
- [ ] Failure recovery suggests fix after `cargo build` with missing crate
- [ ] AI disabled mode: set `ai.enabled = false`, local features work
- [ ] DeepSeek without API key: no error shown, local features work
- [ ] jq absent: plugin falls back to manual parsing
- [ ] socat absent: plugin falls back to zsocket
- [ ] TUI apps (vim, htop) work correctly with default config (stderr OFF)
- [ ] `AWEN_CAPTURE_STDERR=1` enables stderr capture
- [ ] `AWEN_ENABLE_KEYBIND_OVERRIDE=0` disables keybindings
- [ ] Review `~/.local/share/awen/awen.log` — no sensitive data logged

## Documentation

- [ ] README.md and README_CN.md in sync
- [ ] SECURITY.md present and accurate
- [ ] Version in `Cargo.toml` matches release tag
- [ ] CLAUDE.md project guidance up to date

## Release Steps

1. Run all automated checks
2. Complete all manual tests
3. Update version in `Cargo.toml`
4. Create git tag: `git tag v0.1.0-alpha`
5. Push tag: `git push origin v0.1.0-alpha`
