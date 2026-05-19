# Security Policy

## Core Invariant

**Awen always suggests, never executes.** Every suggestion requires explicit user acceptance via keypress (→, Ctrl+→). Awen does not execute commands, modify files, or take any autonomous action.

## What Awen Collects

Awen's daemon collects the following from your terminal session:

- **Current working directory** — to weight suggestions by context
- **Recent commands** — stored in a local SQLite database for history-based completions
- **Exit codes** — to detect command failures
- **stderr output** — only when `AWEN_CAPTURE_STDERR=1` is set (off by default)
- **Git context** — branch name, ahead/behind count, dirty state
- **Project type** — detected from files like `Cargo.toml`, `package.json`, etc.

All data stays local on your machine unless AI completion is enabled.

## What Gets Sent to AI Providers

When AI completion is enabled (`ai.enabled = true`), the following sanitized context is sent to the configured AI provider (DeepSeek or Ollama):

- Working directory (sensitive paths like `.ssh`, `.env` are replaced with `[SENSITIVE_PATH]`)
- Git branch and status (sensitive tokens redacted)
- Recent commands (sensitive values redacted)
- Current input being typed
- Last error output if available (sensitive tokens redacted)

### What Gets Filtered

Before sending to AI, Awen automatically redacts:

- API keys (`sk-*`, `ghp_*`, `gho_*`, `AKIA*`)
- Bearer tokens
- Database URLs with embedded passwords
- Environment variables with sensitive key names (password, token, secret, credential, etc.)
- Docker login passwords
- Export statements with sensitive values

**Important:** Sanitization is best-effort pattern matching. It is not a guarantee that all sensitive data will be caught. Do not type raw secrets, private keys, or passwords directly into the command line.

## What Awen Does NOT Do

- Does not read file contents (no `.env`, `.ssh/id_rsa`, `kubeconfig`, etc.)
- Does not auto-execute any command
- Does not modify any file
- Does not act as an agent or automation tool
- Does not store or transmit data to any service other than the configured AI provider

## History Safety

Commands containing detected sensitive patterns (API keys, tokens, passwords, docker login credentials) are **not recorded** in the history database.

## How to Disable Features

### Disable AI completely

```toml
# ~/.config/awen/config.toml
[ai]
enabled = false
```

All local features (history, specs, risk detection, failure recovery) continue to work without AI.

### Disable stderr capture

Stderr capture is **off by default**. It is only enabled when you set:

```bash
export AWEN_CAPTURE_STDERR=1
```

### Disable risk detection

```toml
[ui]
risk_detection = false
```

### Disable the plugin entirely

Remove or comment out `source ~/.config/awen/awen.zsh` from your `~/.zshrc`.

## Reporting Security Issues

If you discover a security vulnerability, please report it via:

- GitHub Issues: https://github.com/zzf2333/Awen/issues
- Email: zzfzl2022@gmail.com

Please include steps to reproduce the issue if possible.
