# Awen

[中文文档](README_CN.md)

> **Terminal Intelligence Layer — Smart when you need it. Silent when you don't.**

Awen is an open-source terminal intelligence layer that brings smart input experience beyond Warp to any modern terminal — Ghostty, Kitty, WezTerm, Alacritty, and more.

Awen **always suggests, never executes**. Every suggestion requires your explicit acceptance (→, Tab, Enter). It won't make decisions for you, execute commands, or modify files.

## Features

- **Ghost Text Completion** — Inline grey suggestion text; local suggestions (history + specs) appear instantly, AI suggestions refresh asynchronously after you stop typing
- **Failure Recovery** — Suggests fix commands after the previous command fails (suggest only, never execute)
- **Risk Detection** — Inline warnings for dangerous commands like `rm -rf`, `git push --force`, `chmod 777`
- **Command Specs Completion** — Deterministic argument completion in TOML format, built-in for git/docker/npm/cargo/brew/curl/ssh
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
| Dropdown Menu | Planned |

## Architecture

```
Terminal (Ghostty / Kitty / WezTerm / Alacritty)
  └─ zsh (ZLE Widget)
       └─ Shell Plugin (awen.zsh)
            │  Phase 1 (sync): skip_ai=true  → local results in <20ms
            │  Phase 2 (async): skip_ai=false → AI result after idle delay
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

Then add to your `~/.zshrc`:

```bash
source ~/.config/awen/awen.zsh
```

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
awen start     # Start the daemon
awen stop      # Stop the daemon
awen status    # Show status
awen logs      # Show logs
awen config    # Show configuration
awen context   # Show current context
```

## Configuration

Config file at `~/.config/awen/config.toml`:

```toml
[ai]
enabled = true                  # Toggle AI completion
provider = "deepseek"           # deepseek | ollama
debounce_ms = 300               # Delay before triggering AI after typing stops
timeout_ms = 30000              # AI request timeout in ms (async, never blocks input)
max_tokens = 1024               # Max tokens for AI generation (reasoning models need more)

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
capture_stderr = false          # Experimental: capture stderr for failure recovery

[ui]
ghost_text_color = 242          # Ghost text color (ANSI 256)
dropdown_max_items = 8          # Max items in candidate menu (planned)
risk_detection = true           # Dangerous command warnings
command_explanation = false     # Command explanation (planned, not yet implemented)
```

### Custom Specs

Create TOML files in `~/.config/awen/specs/`:

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
| `AWEN_CAPTURE_STDERR` | `0` | Set to `1` to enable experimental stderr capture |
| `AWEN_ENABLE_KEYBIND_OVERRIDE` | `1` | Set to `0` to disable Awen's keybinding overrides |
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

### Build

```bash
cargo build
```

### Test

```bash
cargo test                         # Rust unit + E2E tests
zsh tests/zsh_smoke_test.zsh       # zsh plugin smoke tests
```

### Lint

```bash
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
