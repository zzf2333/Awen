#!/usr/bin/env zsh
# Awen — Terminal Intelligence Layer
# Smart when you need it. Silent when you don't.

typeset -g _AWEN_PLUGIN_DIR="${0:h}"

typeset -g _AWEN_SUGGESTION=""
typeset -g _AWEN_HINT=""
typeset -g _AWEN_WARNING=""
typeset -g _AWEN_LAST_STDERR_FILE="${TMPDIR:-/tmp}/awen-stderr-$$"
typeset -g _AWEN_SOCKET=""
typeset -g _AWEN_BIN=""
typeset -g _AWEN_GHOST_HIGHLIGHT=""
typeset -ga _AWEN_HL_ENTRIES=()
typeset -g _AWEN_GHOST_STYLE="${AWEN_GHOST_STYLE:-fg=244}"
typeset -g _AWEN_STYLE_DIM="${AWEN_STYLE_DIM:-fg=244}"
typeset -g _AWEN_STYLE_MUTED="${AWEN_STYLE_MUTED:-fg=250}"
typeset -g _AWEN_STYLE_TEXT="${AWEN_STYLE_TEXT:-fg=255}"
typeset -g _AWEN_STYLE_SELECTED="${AWEN_STYLE_SELECTED:-fg=255,bold,bg=236}"
typeset -g _AWEN_STYLE_PANEL="${AWEN_STYLE_PANEL:-fg=240}"
typeset -g _AWEN_STYLE_PANEL_BG="${AWEN_STYLE_PANEL_BG:-bg=234}"
typeset -g _AWEN_STYLE_HISTORY="${AWEN_STYLE_HISTORY:-fg=146}"
typeset -g _AWEN_STYLE_SPEC="${AWEN_STYLE_SPEC:-fg=69}"
typeset -g _AWEN_STYLE_AI="${AWEN_STYLE_AI:-fg=177}"
typeset -g _AWEN_STYLE_RISK="${AWEN_STYLE_RISK:-fg=220}"
typeset -g _AWEN_STYLE_FIX="${AWEN_STYLE_FIX:-fg=108}"
typeset -g _AWEN_STYLE_FILE="${AWEN_STYLE_FILE:-fg=73}"
typeset -g _AWEN_STYLE_VERSION="${AWEN_STYLE_VERSION:-fg=238}"

# Async AI state
typeset -g _AWEN_AI_PID=""
typeset -g _AWEN_AI_SNAPSHOT=""
typeset -g _AWEN_AI_SEQ=0
typeset -g _AWEN_AI_ACTIVE_SEQ=0
typeset -g _AWEN_NEED_AI=""
typeset -g _AWEN_AI_DELAY="${AWEN_AI_DELAY:-1}"
typeset -g _AWEN_AI_LOADING=0
typeset -g _AWEN_LOCAL_THROTTLE_MS="${AWEN_LOCAL_THROTTLE_MS:-20}"
typeset -g _AWEN_LAST_LOCAL_MS=0
typeset -g _AWEN_DELETE_FD=0

# Path completion (filesystem ghost-only)
typeset -g _AWEN_PATH_COMPLETION=""

# NL mode state
typeset -g _AWEN_NL_MODE=0
typeset -g _AWEN_FAILURE_SHOWN=0

# Menu state
typeset -g  _AWEN_MENU_ACTIVE=0
typeset -g  _AWEN_MENU_INDEX=1
typeset -ga _AWEN_MENU_TEXTS=()
typeset -ga _AWEN_MENU_SOURCES=()
typeset -ga _AWEN_MENU_DESCS=()
typeset -ga _AWEN_MENU_FULL_CMDS=()
typeset -g  _AWEN_MENU_COUNT=0
typeset -g  _AWEN_MENU_MAX="${AWEN_MENU_MAX_ITEMS:-5}"
typeset -g  _AWEN_MENU_ENABLED="${AWEN_MENU_ENABLED:-1}"
typeset -g  _AWEN_STDERR_MAX="${AWEN_STDERR_MAX_CHARS:-500}"
typeset -g  _AWEN_UI_MODE="${AWEN_UI_MODE:-full}"

# Source modules
source "${_AWEN_PLUGIN_DIR}/awen-utils.zsh"
source "${_AWEN_PLUGIN_DIR}/awen-source.zsh"
source "${_AWEN_PLUGIN_DIR}/awen-render.zsh"
source "${_AWEN_PLUGIN_DIR}/awen-interact.zsh"
source "${_AWEN_PLUGIN_DIR}/awen-communicate.zsh"

TRAPUSR1() {
    if zle 2>/dev/null; then
        zle _awen_on_ai_signal
    fi
}

awen_init() {
    _awen_find_binary
    _awen_find_socket

    if [[ -z "$_AWEN_BIN" ]]; then
        echo "awen: binary not found. Install with: cargo install --path ."
        return 1
    fi

    typeset -g _AWEN_VERSION=""
    local ver_out
    ver_out=$("$_AWEN_BIN" --version 2>/dev/null)
    _AWEN_VERSION="${ver_out#awen }"
    if [[ -n "$_AWEN_VERSION" ]]; then
        typeset -g _AWEN_LOGO="Awen v${_AWEN_VERSION}"
    else
        typeset -g _AWEN_LOGO="Awen"
    fi

    if command -v jq &>/dev/null; then
        typeset -g _AWEN_HAS_JQ=1
    else
        typeset -g _AWEN_HAS_JQ=0
    fi

    if zmodload zsh/datetime 2>/dev/null; then
        typeset -g _AWEN_HAS_ZDATE=1
    else
        typeset -g _AWEN_HAS_ZDATE=0
    fi

    typeset -g _AWEN_AI_RESULT_FILE="${TMPDIR:-/tmp}/.awen-ai-result-$$"
    : > "$_AWEN_AI_RESULT_FILE"

    trap '
        [[ -n "$_AWEN_AI_PID" ]] && kill "$_AWEN_AI_PID" 2>/dev/null
        (( _AWEN_DELETE_FD > 0 )) && { zle -F $_AWEN_DELETE_FD 2>/dev/null; exec {_AWEN_DELETE_FD}<&- 2>/dev/null; }
        rm -f "$_AWEN_LAST_STDERR_FILE" "${TMPDIR:-/tmp}/.awen-ai-token-$$" "$_AWEN_AI_RESULT_FILE" 2>/dev/null
    ' EXIT

    _awen_ensure_daemon

    trap '
        _AWEN_MENU_ACTIVE=0
        _AWEN_MENU_COUNT=0
        _AWEN_SUGGESTION=""
        { POSTDISPLAY=""; region_highlight=(); } 2>/dev/null
    ' WINCH

    typeset -ga _AWEN_CONFLICTS=()
    if (( $+functions[_zsh_autosuggest_start] )) || [[ -n "${ZSH_AUTOSUGGEST_STRATEGY:-}" ]]; then
        _AWEN_CONFLICTS+=(zsh-autosuggestions)
    fi
    if (( $+widgets[fzf-history-widget] )) || (( $+functions[fzf-history-widget] )); then
        _AWEN_CONFLICTS+=(fzf)
    fi
    if (( ${#_AWEN_CONFLICTS} > 0 )) && [[ "$_AWEN_UI_MODE" == "full" ]]; then
        _AWEN_UI_MODE="minimal"
        AWEN_UI_MODE="minimal"
        echo "awen: detected ${(j:, :)_AWEN_CONFLICTS} — switching to minimal mode" >&2
    fi

    zle -N _awen_self_insert
    zle -N _awen_backward_delete_char
    zle -N _awen_accept
    zle -N _awen_accept_word
    zle -N _awen_dismiss
    zle -N _awen_suggest_local
    zle -N _awen_menu_up
    zle -N _awen_menu_down
    zle -N _awen_menu_accept
    zle -N _awen_tab
    zle -N _awen_on_ai_signal

    if [[ "${AWEN_ENABLE_KEYBIND_OVERRIDE:-1}" == "1" ]]; then
        bindkey -M main '\e[C' _awen_accept
        bindkey -M main '\eOC' _awen_accept
        bindkey -M main '\e[1;5C' _awen_accept_word
        bindkey -M main '\e[27;5;67~' _awen_accept_word
        bindkey -M main '\e\e[C' _awen_accept_word
        bindkey -M main '\e[Z' _awen_dismiss

        if [[ "$_AWEN_UI_MODE" != "minimal" ]]; then
            bindkey -M main '\e[A' _awen_menu_up
            bindkey -M main '\eOA' _awen_menu_up
            bindkey -M main '\e[B' _awen_menu_down
            bindkey -M main '\eOB' _awen_menu_down
            bindkey -M main '^M' _awen_menu_accept
        fi

        local -a printable
        printable=({a..z} {A..Z} {0..9} ' ' '-' '_' '.' '/' '~' ':' '=' '+' '@' ',' ';' '!' '?' '#' '$' '%' '^' '&' '*' '(' ')' '[' ']' '{' '}' '<' '>' '|' "'" '"' '`' '\\')
        local key
        for key in "${printable[@]}"; do
            bindkey -M main -- "$key" _awen_self_insert
        done

        bindkey -M main '^?' _awen_backward_delete_char
        bindkey -M main '^H' _awen_backward_delete_char

        bindkey -M main '\t' _awen_tab
    fi

    autoload -Uz add-zsh-hook
    add-zsh-hook precmd _awen_precmd
    add-zsh-hook preexec _awen_preexec

    _awen_line_init() {
        if (( ! _AWEN_FAILURE_SHOWN )) \
            && [[ -n "$_AWEN_LAST_EXIT_CODE" && "$_AWEN_LAST_EXIT_CODE" -ne 0 ]] \
            && [[ "$_AWEN_LAST_EXIT_CODE" -ne 130 && "$_AWEN_LAST_EXIT_CODE" -ne 137 && "$_AWEN_LAST_EXIT_CODE" -ne 143 ]] \
            && [[ -s "$_AWEN_LAST_STDERR_FILE" ]] \
            && [[ -z "$BUFFER" ]]; then
            _AWEN_FAILURE_SHOWN=1
            _awen_suggest_next
        fi
    }
    zle -N zle-line-init _awen_line_init
}

awen_init
