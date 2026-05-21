# Awen

[中文文档](README_CN.md)

> **Terminal Intelligence Layer — Smart when you need it. Silent when you don't.**

Awen is an open-source terminal intelligence layer that brings smart input experience beyond Warp to any modern terminal — Ghostty, Kitty, WezTerm, Alacritty, and more.

Awen **always suggests, never executes**. Every suggestion requires your explicit acceptance (→, Tab, Enter). It won't make decisions for you, execute commands, or modify files.

## Features

- **Ghost Text Completion** — Inline grey suggestion text; local suggestions (history + specs) appear instantly, AI suggestions refresh asynchronously after you stop typing
- **Failure Recovery** — Suggests fix commands after the previous command fails (suggest only, never execute)
- **Risk Detection** — Inline warnings for dangerous commands like `rm -rf`, `git push --force`, `chmod 777`
- **Command Specs Completion** — Deterministic argument completion in TOML format, 77 built-in specs covering git, docker, npm, cargo, aws, gcloud, kubectl, helm, claude, codex, and more
- **AI Completion** — Supports DeepSeek and Ollama, timeout-bounded optional, can be disabled
- **Context Awareness** — Project type detection, Git status, recent commands, failure history

## Feature Maturity

| Feature | Status |
|---------|--------|
| Ghost Text (History + Specs) | **Stable** |
| Risk Detection | **Stable** |
| Failure Recovery (local patterns) | Experimental (depends on stderr capture) |
| AI Completion (DeepSeek / Ollama) | Experimental |
| stderr Capture | Experimental (off by default) |
| Command Explanation | Planned |
| Dropdown Menu | Experimental |

## Architecture

```
Terminal (Ghostty / Kitty / WezTerm / Alacritty)
  └─ zsh (ZLE Widget)
       └─ Shell Plugin (awen.zsh)
            │  Phase 1 (sync): skip_ai=true  → local results in <20ms
            │  Phase 2 (async, conditional): skip_ai=false → AI fallback when local insufficient
            │ Unix Socket
            └─ Daemon (Rust + tokio)
                 ├─ Context Engine (session / repo / git)
                 ├─ Layer 1: History (SQLite + nucleo) — < 5ms
                 ├─ Layer 1: Specs (TOML) — < 20ms
                 ├─ Layer 2: AI (DeepSeek / Ollama) — async, never blocks input
                 ├─ Layer 2: Failure Recovery (pattern + AI)
                 ├─ Layer 2: Risk Detection (regex)
                 └─ Suggestion Arbitrator
```

## Installation

### Prerequisites

- Rust toolchain (1.85+)
- zsh
- jq (recommended, for robust JSON handling)
- socat (optional, for shell-daemon communication; falls back to zsh built-in zsocket)

### Install from Source

```bash
git clone https://github.com/zzf2333/Awen.git
cd awen
./install.sh
```

The install script will:
1. Build release binary
2. Install to `~/.local/bin/awen`
3. Copy specs and zsh plugin to `~/.config/awen/`
4. Generate default config file
5. Add `source ~/.config/awen/awen.zsh` to `~/.zshrc` (interactive prompt, default yes)
6. Add `~/.local/bin` to PATH if missing

Open a new terminal — Awen starts automatically and imports your zsh history on first launch.

### Manual Installation

```bash
cargo build --release
cp target/release/awen ~/.local/bin/
cp plugin/awen.zsh ~/.config/awen/
cp specs/*.toml ~/.config/awen/specs/
# Add to .zshrc: source ~/.config/awen/awen.zsh
```

## Usage

### Keybindings

| Key | Action |
|-----|--------|
| `→` | Accept full ghost text |
| `Ctrl+→` | Accept next word |
| `Shift+Tab` | Dismiss suggestion |

### CLI Commands

```bash
awen start              # Start the daemon (auto-started by the zsh plugin)
awen stop               # Stop the daemon
awen status             # Show status
awen logs               # Show logs
awen config             # Show configuration
awen context            # Show current context
awen history import     # Import from zsh history (auto-runs on first launch)
```

The `history import` command accepts `--file <path>` for a custom history file and `--force` to re-import when the database is not empty.

## Configuration

Config file at `~/.config/awen/config.toml`:

```toml
[ai]
enabled = true                  # Toggle AI completion
provider = "deepseek"           # deepseek | ollama
debounce_ms = 300               # Delay before triggering AI after typing stops
timeout_ms = 30000              # AI request timeout in ms (async, never blocks input)
max_tokens = 1024               # Max tokens for AI generation (reasoning models need more)
min_local_candidates = 2        # AI triggers only when local results < this AND confidence < threshold
min_local_confidence = 0.6      # AI triggers only when max confidence < this AND count < threshold

[ai.deepseek]
api_key = ""                    # Or set DEEPSEEK_API_KEY env var
model = "deepseek-chat"
base_url = "https://api.deepseek.com"

[ai.ollama]
model = "qwen2.5-coder:7b"
base_url = "http://localhost:11434"

[context]
session_history_size = 20       # Number of commands to remember in session
stderr_max_chars = 500          # Max stderr length to capture
repo_detect = true              # Auto-detect project type
git_context = true              # Collect Git context
capture_stderr = true           # Capture stderr for failure recovery

[ui]
ghost_text_color = 242          # Ghost text color (ANSI 256)
dropdown_max_items = 8          # Max items in candidate menu (planned)
risk_detection = true           # Dangerous command warnings
command_explanation = false     # Command explanation (planned, not yet implemented)
```

### Built-in Specs

Awen ships with 77 built-in command specs, organized by category:

<details>
<summary>Full list (click to expand)</summary>

| Category | Commands |
|----------|----------|
| VCS & Dev Ecosystem | `git`, `docker`, `npm`, `cargo`, `brew`, `curl`, `ssh` |
| Cloud & Infrastructure | `gh`, `kubectl`, `terraform`, `aws`, `gcloud`, `az`, `helm` |
| Languages & Runtimes | `python`, `go`, `node` |
| Package Managers & Build Tools | `pip`, `pnpm`, `yarn`, `bun`, `uv`, `poetry`, `cmake`, `make` |
| AI Tools | `claude`, `codex`, `opencode`, `antigravity` |
| File Operations | `ls`, `rm`, `cp`, `mv`, `mkdir`, `touch`, `ln`, `chmod`, `chown` |
| Text Processing | `cat`, `head`, `tail`, `grep`, `sed`, `awk`, `sort`, `uniq`, `wc`, `diff`, `cut`, `tr`, `tee`, `xargs` |
| Search, Archive & Process | `find`, `tar`, `ps`, `kill`, `df`, `du`, `lsof` |
| Networking & Diagnostics | `ping`, `dig`, `wget`, `ss`, `nmap` |
| System Administration | `systemctl`, `journalctl`, `htop` |
| Terminal Multiplexers | `tmux`, `screen` |
| Testing & Linting | `pytest`, `ruff` |
| Task Runners | `just` |
| Database CLIs | `psql`, `mysql`, `redis-cli`, `mongosh`, `sqlite3` |

</details>

### Custom Specs

Create TOML files in `~/.config/awen/specs/` to add new commands or override built-ins:

```toml
[command]
name = "my-tool"
description = "My custom tool"

[[command.subcommands]]
name = "deploy"
description = "Deploy to production"

[[command.subcommands.flags]]
name = "--env"
short = "-e"
arg = "ENV"
description = "Target environment"
```

### Contributing Specs

To contribute a built-in spec:

1. Create `specs/<command>.toml` following the format above
2. Register it in the `builtin_specs!` macro in `src/layer/specs.rs`
3. Run `cargo test` to verify parsing

Conventions:
- Command and subcommand names are lowercase
- Flag names use `--kebab-case`, short flags use `-x`
- Argument placeholders are UPPERCASE (`FILE`, `NUM`, `DIR`)
- Descriptions are terse, no trailing period
- Dangerous flags (auto-execute, bypass safety) should go in risk patterns, not specs

### Custom Failure Patterns

In `~/.config/awen/failure_patterns.toml`:

```toml
[[failure_patterns]]
pattern = "my custom error: (\\w+)"
suggestion = "my-tool fix {1}"
description = "Fix the custom error"
```

### Custom Risk Patterns

In `~/.config/awen/risk_patterns.toml`:

```toml
[[risk_patterns]]
pattern = "my-dangerous-cmd --force"
warning = "This will force-execute, are you sure?"
```

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `AWEN_AI_DELAY` | `1` | Seconds to wait after typing stops before firing an AI request |
| `AWEN_LOCAL_THROTTLE_MS` | `20` | Minimum ms between local suggestion requests (keystroke throttle) |
| `AWEN_CAPTURE_STDERR` | `1` | Set to `0` to disable stderr capture |
| `AWEN_STDERR_MAX_CHARS` | `500` | Max bytes of stderr to send to daemon |
| `AWEN_ENABLE_KEYBIND_OVERRIDE` | `1` | Set to `0` to disable Awen's keybinding overrides |
| `AWEN_GHOST_STYLE` | `fg=244` | Ghost text style (zsh highlight spec) |
| `AWEN_STYLE_DIM` | `fg=244` | Dim text style |
| `AWEN_STYLE_MUTED` | `fg=250` | Muted text style |
| `AWEN_STYLE_TEXT` | `fg=255` | Normal text style |
| `AWEN_STYLE_SELECTED` | `fg=255,bold,bg=236` | Selected item style |
| `AWEN_STYLE_PANEL` | `fg=240` | Panel border style |
| `AWEN_STYLE_PANEL_BG` | `bg=234` | Panel background style |
| `AWEN_STYLE_HISTORY` | `fg=146` | History source tag color |
| `AWEN_STYLE_SPEC` | `fg=69` | Spec source tag color |
| `AWEN_STYLE_AI` | `fg=177` | AI source tag color |
| `AWEN_STYLE_RISK` | `fg=220` | Risk warning color |
| `AWEN_STYLE_FIX` | `fg=108` | Fix suggestion color |
| `DEEPSEEK_API_KEY` | — | DeepSeek API key (alternative to config file) |

## Safety Boundary

Awen's security model is minimal because it **always suggests, never executes**:

- **No auto-execution** — Every suggestion requires explicit user confirmation
- **No file modification** — Does not read or write user project files
- **No sensitive file access** — Does not access `.env`, `.ssh`, `kubeconfig`, AWS credentials, etc.
- **No privacy leaks** — Context sent to AI is sanitized (API keys, tokens, passwords filtered)
- **AI can be disabled** — Set `ai.enabled = false`, all local features keep working
- **Works offline** — History matching, specs completion, risk detection, failure patterns all run locally
- **Not an agent** — No planning, no execution, no automation, no workflow inference

## Development

### Quick Iteration

```bash
make dev       # Debug build + sync plugin + restart daemon (fastest)
make release   # Release build + sync + restart
make sync      # Sync plugin/specs only (no rebuild, for zsh-only changes)
make test      # cargo test + shellcheck + zsh smoke tests
make lint      # clippy + fmt + shellcheck
make status    # Check daemon status
make logs      # Show recent daemon logs
```

`make dev` is the primary dev loop — one command to build, deploy, and restart. Changes are live in the current shell immediately.

### Manual Build

```bash
cargo build
cargo test
cargo clippy
cargo fmt --check
```

### Project Structure

```
src/
├── main.rs           # CLI entry point
├── lib.rs            # Module exports
├── daemon.rs         # Unix socket server
├── protocol.rs       # JSON protocol definitions
├── config.rs         # Configuration loading
├── arbitrator.rs     # Suggestion arbitration
├── sanitize.rs       # Sensitive info filtering
├── context/          # Context engine
│   ├── session.rs    # Session context
│   ├── repo.rs       # Project type detection
│   └── git.rs        # Git context
└── layer/            # Completion layers
    ├── history.rs    # History matching
    ├── specs.rs      # Command specs
    ├── ai.rs         # AI completion
    ├── failure.rs    # Failure recovery
    └── risk.rs       # Risk detection
```

## Design Philosophy

**Whisper, not shout.** Ghost text is grey, translucent. Inline hints appear only in high-value moments. "Say less" matters more than "be smarter."

**Appear when you're stuck.** `cargo build` fails — ghost text quietly surfaces `cargo add tokio`.

**Exist like air.** Ultra-low latency, featherweight, never interrupts, never oversteps. Install it, your zsh gains inspiration; remove it, everything stays the same.

## Name

**Awen** [AH-wen] is a Welsh word meaning "inspiration" and "flowing spirit." It shares its root with _awel_ (breeze).

## License

MIT
