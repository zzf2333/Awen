#!/bin/sh
set -eu

REPO="zzf2333/Awen"
INSTALL_DIR="${HOME}/.local/bin"
CONFIG_DIR="${HOME}/.config/awen"
PLUGIN_DIR="${CONFIG_DIR}"
DATA_DIR="${HOME}/.local/share/awen"

BOLD='\033[1m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
RED='\033[0;31m'
RESET='\033[0m'

info()  { printf "${GREEN}[info]${RESET} %s\n" "$1"; }
warn()  { printf "${YELLOW}[warn]${RESET} %s\n" "$1"; }
error() { printf "${RED}[error]${RESET} %s\n" "$1"; }

detect_platform() {
    OS="$(uname -s)"
    ARCH="$(uname -m)"

    case "${OS}" in
        Darwin) OS="apple-darwin" ;;
        Linux)  OS="unknown-linux-gnu" ;;
        *)      error "Unsupported OS: ${OS}"; exit 1 ;;
    esac

    case "${ARCH}" in
        arm64|aarch64) ARCH="aarch64" ;;
        x86_64|amd64)  ARCH="x86_64" ;;
        *)             error "Unsupported architecture: ${ARCH}"; exit 1 ;;
    esac

    TARGET="${ARCH}-${OS}"
}

get_latest_version() {
    if [ -n "${AWEN_VERSION:-}" ]; then
        VERSION="${AWEN_VERSION}"
        return
    fi

    info "Fetching latest version..."
    if command -v curl >/dev/null 2>&1; then
        VERSION=$(curl -sSL "https://api.github.com/repos/${REPO}/releases/latest" \
            | grep '"tag_name"' | head -1 | sed 's/.*"v\([^"]*\)".*/\1/')
    elif command -v wget >/dev/null 2>&1; then
        VERSION=$(wget -qO- "https://api.github.com/repos/${REPO}/releases/latest" \
            | grep '"tag_name"' | head -1 | sed 's/.*"v\([^"]*\)".*/\1/')
    else
        error "Neither curl nor wget found. Install one and retry."
        exit 1
    fi

    if [ -z "${VERSION}" ]; then
        error "Failed to determine latest version. Set AWEN_VERSION=x.y.z and retry."
        exit 1
    fi
}

download() {
    url="$1"
    output="$2"
    if command -v curl >/dev/null 2>&1; then
        curl -sSL -o "${output}" "${url}"
    elif command -v wget >/dev/null 2>&1; then
        wget -qO "${output}" "${url}"
    fi
}

verify_checksum() {
    tarball="$1"
    checksums="$2"
    if command -v shasum >/dev/null 2>&1; then
        grep "$(basename "${tarball}")" "${checksums}" | shasum -a 256 -c --quiet -
    elif command -v sha256sum >/dev/null 2>&1; then
        grep "$(basename "${tarball}")" "${checksums}" | sha256sum -c --quiet -
    else
        warn "No sha256 tool found, skipping checksum verification."
        return 0
    fi
}

ask_yes() {
    prompt="$1"
    if [ ! -t 0 ]; then
        printf "%s [Y/n] Y (non-interactive, auto-yes)\n" "${prompt}"
        return 0
    fi
    printf "%s [Y/n] " "${prompt}"
    read -r answer
    answer="${answer:-Y}"
    case "${answer}" in
        [Yy]*) return 0 ;;
        *)     return 1 ;;
    esac
}

main() {
    detect_platform
    get_latest_version

    TARBALL="awen-${VERSION}-${TARGET}.tar.gz"
    BASE_URL="https://github.com/${REPO}/releases/download/v${VERSION}"
    TMPDIR_DL="$(mktemp -d)"

    trap 'rm -rf "${TMPDIR_DL}"' EXIT

    info "Downloading Awen v${VERSION} for ${TARGET}..."
    download "${BASE_URL}/${TARBALL}" "${TMPDIR_DL}/${TARBALL}"
    download "${BASE_URL}/SHA256SUMS" "${TMPDIR_DL}/SHA256SUMS"

    info "Verifying checksum..."
    if ! (cd "${TMPDIR_DL}" && verify_checksum "${TARBALL}" "SHA256SUMS"); then
        error "Checksum verification failed!"
        exit 1
    fi

    info "Extracting..."
    tar xzf "${TMPDIR_DL}/${TARBALL}" -C "${TMPDIR_DL}"

    EXTRACTED="${TMPDIR_DL}/awen-${VERSION}-${TARGET}"

    # Install binary
    info "Installing binary to ${INSTALL_DIR}/awen..."
    mkdir -p "${INSTALL_DIR}"
    cp "${EXTRACTED}/awen" "${INSTALL_DIR}/awen"
    chmod +x "${INSTALL_DIR}/awen"

    # macOS: clear quarantine + codesign
    if [ "$(uname -s)" = "Darwin" ]; then
        xattr -cr "${INSTALL_DIR}/awen" 2>/dev/null || true
        codesign -fs - "${INSTALL_DIR}/awen" 2>/dev/null || true
    fi

    # Install plugin
    info "Installing plugin to ${PLUGIN_DIR}/awen.zsh..."
    mkdir -p "${PLUGIN_DIR}"
    cp "${EXTRACTED}/awen.zsh" "${PLUGIN_DIR}/awen.zsh"

    # Create default config if missing
    if [ ! -f "${CONFIG_DIR}/config.toml" ]; then
        info "Creating default config at ${CONFIG_DIR}/config.toml..."
        cat > "${CONFIG_DIR}/config.toml" << 'TOML'
[ai]
enabled = true
base_url = "https://api.deepseek.com"
model = "deepseek-chat"
api_key = ""
debounce_ms = 300
timeout_ms = 30000
max_tokens = 1024
cache_ttl_minutes = 30

[context]
session_history_size = 20
stderr_max_chars = 500
repo_detect = true
git_context = true
capture_stderr = true

[ui]
ghost_text_color = 242
dropdown_max_items = 8
hint_style = "above"
risk_detection = true
command_explanation = false
TOML
    else
        info "Config already exists, preserving."
    fi

    # Create data directory
    mkdir -p "${DATA_DIR}"

    printf "\n"
    info "${BOLD}Awen v${VERSION} installed successfully!${RESET}"
    printf "\n"

    # PATH setup
    case ":${PATH}:" in
        *":${INSTALL_DIR}:"*) ;;
        *)
            ZSHRC="${HOME}/.zshrc"
            if [ -f "${ZSHRC}" ] && grep -qF '.local/bin' "${ZSHRC}"; then
                info "${INSTALL_DIR} PATH entry already in .zshrc"
            elif ask_yes "Add ${INSTALL_DIR} to PATH in ~/.zshrc?"; then
                # shellcheck disable=SC2016
                printf '\n# Added by Awen installer\nexport PATH="${HOME}/.local/bin:${PATH}"\n' >> "${ZSHRC}"
                info "PATH entry added to ~/.zshrc"
            else
                warn "Skipped. Add manually: export PATH=\"\${HOME}/.local/bin:\${PATH}\""
            fi
            ;;
    esac

    # Source plugin in .zshrc
    ZSHRC="${HOME}/.zshrc"
    if [ -f "${ZSHRC}" ] && grep -qF "awen.zsh" "${ZSHRC}"; then
        info "awen.zsh is already sourced in .zshrc"
    elif ask_yes "Add 'source ~/.config/awen/awen.zsh' to ~/.zshrc?"; then
        printf '\n# Awen — Terminal Intelligence Layer\nsource %s/awen.zsh\n' "${PLUGIN_DIR}" >> "${ZSHRC}"
        info "Added to ~/.zshrc"
    else
        warn "Skipped. Add manually: source ${PLUGIN_DIR}/awen.zsh"
    fi

    # Optional dependency hints
    if ! command -v jq >/dev/null 2>&1; then
        warn "jq not found (optional). Install for more robust JSON handling."
    fi
    if ! command -v socat >/dev/null 2>&1; then
        warn "socat not found (optional). Falls back to zsh zsocket."
    fi

    printf "\n"
    printf '%sQuick start:%s\n' "${BOLD}" "${RESET}"
    printf '  Open a new terminal — Awen will start automatically.\n'
    printf '  awen status   — check daemon status\n'
    printf '  awen stop     — stop the daemon\n'
    printf '  awen config   — view configuration\n'
    printf '\n'
    printf '  On first launch, Awen will import your zsh history automatically.\n'
    printf '  To import manually: awen history import\n'
    printf '\n'
    printf 'For AI completions, set your API key in %s/config.toml\n' "${CONFIG_DIR}"
    printf 'or export AWEN_API_KEY=your-key\n'
    printf '\n'
    printf '%sTo uninstall:%s\n' "${BOLD}" "${RESET}"
    printf '  rm ~/.local/bin/awen\n'
    printf '  rm -rf ~/.config/awen\n'
    printf '  rm -rf ~/.local/share/awen\n'
    printf '  # Remove source ~/.config/awen/awen.zsh from ~/.zshrc\n'
}

main
