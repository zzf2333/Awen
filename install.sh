#!/usr/bin/env bash
set -euo pipefail

BOLD='\033[1m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
RED='\033[0;31m'
RESET='\033[0m'

info() { echo -e "${GREEN}[info]${RESET} $1"; }
warn() { echo -e "${YELLOW}[warn]${RESET} $1"; }
error() { echo -e "${RED}[error]${RESET} $1"; }

INSTALL_DIR="${HOME}/.local/bin"
CONFIG_DIR="${HOME}/.config/awen"
PLUGIN_DIR="${CONFIG_DIR}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Check for Rust toolchain
if ! command -v cargo &>/dev/null; then
    error "Rust toolchain not found. Install via: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
    exit 1
fi

# Check for zsh
if ! command -v zsh &>/dev/null; then
    warn "zsh not found. Awen requires zsh for the shell plugin."
fi

# Check for recommended tools
if ! command -v jq &>/dev/null; then
    warn "jq not found. Install jq for more robust JSON handling (recommended)."
fi
if ! command -v socat &>/dev/null; then
    warn "socat not found. The plugin will fall back to zsh's built-in zsocket."
fi

info "Building awen (release mode)..."
cargo build --release --manifest-path "${SCRIPT_DIR}/Cargo.toml"

info "Installing binary to ${INSTALL_DIR}..."
mkdir -p "${INSTALL_DIR}"
cp "${SCRIPT_DIR}/target/release/awen" "${INSTALL_DIR}/awen"
chmod +x "${INSTALL_DIR}/awen"

# macOS: clear quarantine/provenance attrs and re-sign so Gatekeeper won't SIGKILL
if [[ "$(uname)" == "Darwin" ]]; then
    xattr -cr "${INSTALL_DIR}/awen" 2>/dev/null
    codesign -fs - "${INSTALL_DIR}/awen" 2>/dev/null
fi

info "Installing specs to ${CONFIG_DIR}/specs/..."
mkdir -p "${CONFIG_DIR}/specs"
cp "${SCRIPT_DIR}"/specs/*.toml "${CONFIG_DIR}/specs/"

info "Installing zsh plugin to ${PLUGIN_DIR}/..."
cp "${SCRIPT_DIR}/plugin/awen.zsh" "${PLUGIN_DIR}/awen.zsh"

# Create default config if not exists
if [[ ! -f "${CONFIG_DIR}/config.toml" ]]; then
    info "Creating default config at ${CONFIG_DIR}/config.toml..."
    cat > "${CONFIG_DIR}/config.toml" << 'EOF'
[ai]
enabled = true
provider = "deepseek"
debounce_ms = 300
timeout_ms = 30000
max_tokens = 1024
cache_ttl_minutes = 30

[ai.deepseek]
api_key = ""
model = "deepseek-chat"
base_url = "https://api.deepseek.com"

[ai.ollama]
model = "qwen2.5-coder:7b"
base_url = "http://localhost:11434"

[context]
session_history_size = 20
stderr_max_chars = 500
repo_detect = true
git_context = true
capture_stderr = false

[ui]
ghost_text_color = 242
dropdown_max_items = 8
hint_style = "above"
risk_detection = true
command_explanation = false
EOF
else
    info "Config file already exists, skipping."
fi

# Create data directory
mkdir -p "${HOME}/.local/share/awen"

echo ""
info "${BOLD}Awen installed successfully!${RESET}"
echo ""

# Interactive prompt helper: defaults to yes in non-interactive mode (pipe)
ask_yes() {
    local prompt="$1"
    if [[ ! -t 0 ]]; then
        echo -e "${prompt} [Y/n] Y (non-interactive, auto-yes)"
        return 0
    fi
    local answer
    read -rp "$(echo -e "${prompt} [Y/n] ")" answer
    answer="${answer:-Y}"
    [[ "$answer" =~ ^[Yy]$ ]]
}

# Check PATH and offer to fix
if [[ ":${PATH}:" != *":${INSTALL_DIR}:"* ]]; then
    ZSHRC="${HOME}/.zshrc"
    if [[ -f "$ZSHRC" ]] && grep -qF '.local/bin' "$ZSHRC"; then
        info "${INSTALL_DIR} PATH entry already in .zshrc"
    elif ask_yes "${BOLD}${INSTALL_DIR} is not in PATH. Add to ~/.zshrc?"; then
        {
            echo ""
            echo "# Added by Awen installer"
            # shellcheck disable=SC2016
            echo 'export PATH="${HOME}/.local/bin:${PATH}"'
        } >> "${HOME}/.zshrc"
        info "PATH entry added to ~/.zshrc"
        export PATH="${INSTALL_DIR}:${PATH}"
    else
        # shellcheck disable=SC2016
        warn 'Skipped. Add manually: export PATH="${HOME}/.local/bin:${PATH}"'
    fi
fi

# Source the zsh plugin in .zshrc
ZSHRC="${HOME}/.zshrc"
SOURCE_LINE="source ${PLUGIN_DIR}/awen.zsh"

if [[ -f "$ZSHRC" ]] && grep -qF "awen.zsh" "$ZSHRC"; then
    info "awen.zsh is already sourced in .zshrc"
elif ask_yes "${BOLD}Add 'source ~/.config/awen/awen.zsh' to ~/.zshrc?"; then
    {
        echo ""
        echo "# Awen — Terminal Intelligence Layer"
        echo "${SOURCE_LINE}"
    } >> "$ZSHRC"
    info "Added to ~/.zshrc"
else
    warn "Skipped. Add manually: ${SOURCE_LINE}"
fi

echo ""
echo -e "${BOLD}Quick start:${RESET}"
echo "  Open a new terminal — Awen will start automatically."
echo "  awen status   — check daemon status"
echo "  awen stop     — stop the daemon"
echo "  awen config   — view configuration"
echo ""
echo "  On first launch, Awen will import your zsh history automatically."
echo "  To import manually: awen history import"
echo ""
echo "For AI completions, set your API key in ${CONFIG_DIR}/config.toml"
echo "or export DEEPSEEK_API_KEY=sk-your-key"
echo ""
echo "To disable AI completions: set ai.enabled = false in ${CONFIG_DIR}/config.toml"
echo ""
echo -e "${BOLD}To uninstall:${RESET}"
echo "  rm ~/.local/bin/awen"
echo "  rm -rf ~/.config/awen"
echo "  rm -rf ~/.local/share/awen"
echo "  # Remove 'source ~/.config/awen/awen.zsh' from ~/.zshrc"
