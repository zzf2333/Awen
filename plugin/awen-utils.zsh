#!/usr/bin/env zsh
# Awen — Utility functions (JSON, socket, time, layout helpers)

_awen_extract_json_value() {
    local input="$1"
    local result=""
    local i=0
    local ch prev_ch=""
    while [[ $i -lt ${#input} ]]; do
        ch="${input:$i:1}"
        if [[ "$ch" == '"' && "$prev_ch" != '\' ]]; then
            break
        fi
        result+="$ch"
        prev_ch="$ch"
        ((i++))
    done
    printf '%s\n' "$result"
}

_awen_json_escape() {
    local s="$1"
    s="${s//\\/\\\\}"
    s="${s//\"/\\\"}"
    s="${s//$'\n'/\\n}"
    s="${s//$'\r'/\\r}"
    s="${s//$'\t'/\\t}"
    printf '%s' "$s"
}

_awen_find_binary() {
    if [[ -x "${HOME}/.local/bin/awen" ]]; then
        _AWEN_BIN="${HOME}/.local/bin/awen"
    elif command -v awen &>/dev/null; then
        _AWEN_BIN="$(command -v awen)"
    elif [[ -x "${0:A:h}/../target/release/awen" ]]; then
        _AWEN_BIN="${0:A:h}/../target/release/awen"
    elif [[ -x "${0:A:h}/../target/debug/awen" ]]; then
        _AWEN_BIN="${0:A:h}/../target/debug/awen"
    fi
}

_awen_find_socket() {
    local uid=$(id -u)
    local xdg="${XDG_RUNTIME_DIR:-${TMPDIR:-/tmp}}"
    _AWEN_SOCKET="${xdg}/awen-${uid}.sock"
}

_awen_ensure_daemon() {
    if [[ -z "$_AWEN_BIN" ]]; then
        return 1
    fi
    if [[ ! -S "$_AWEN_SOCKET" ]]; then
        "$_AWEN_BIN" start &!
        sleep 0.3
    fi
}

_awen_send_nc() {
    if [[ ! -S "$_AWEN_SOCKET" ]]; then
        return 1
    fi
    local request="$1"
    if command -v socat &>/dev/null; then
        printf '%s\n' "$request" | socat -T 0.1 -t 0.5 - UNIX-CONNECT:"$_AWEN_SOCKET" 2>/dev/null
    else
        if zmodload zsh/net/socket 2>/dev/null; then
            local fd
            zsocket "$_AWEN_SOCKET" && fd=$REPLY
            if [[ -n "$fd" ]]; then
                printf '%s\n' "$request" >&$fd
                local response
                read -r response <&$fd
                exec {fd}>&-
                printf '%s\n' "$response"
            fi
        fi
    fi
}

_awen_now_ms() {
    if [[ "$_AWEN_HAS_ZDATE" == "1" ]]; then
        local secs="${EPOCHREALTIME%.*}"
        local frac="${EPOCHREALTIME#*.}"
        printf '%s' "${secs}${frac:0:3}"
    else
        printf '%d' $(( $(date +%s) * 1000 ))
    fi
}

_awen_pad_right() {
    local text="$1" width="$2"
    local len=${#text}
    if (( len >= width )); then
        printf '%s' "${text[1,$width]}"
    else
        printf '%s%s' "$text" "$(_awen_repeat ' ' $(( width - len )))"
    fi
}

_awen_repeat() {
    local char="$1" count="$2" out=""
    local i
    for (( i=0; i<count; i++ )); do
        out+="$char"
    done
    printf '%s' "$out"
}

_awen_keycap_line() {
    local width="$1"
    local logo="${_AWEN_LOGO:-Awen}"
    local actions="↑↓ select   ↵ run   ⇥ edit   → next word   esc dismiss"
    local gap=$(( width - ${#actions} - ${#logo} ))
    if (( gap < 2 )); then
        printf '%s' "$actions"
    else
        printf '%s%s%s' "$actions" "$(_awen_repeat ' ' "$gap")" "$logo"
    fi
}

_awen_footer_line() {
    local width="$1" actions="$2"
    local logo="${_AWEN_LOGO:-Awen}"
    local gap=$(( width - ${#actions} - ${#logo} ))
    if (( gap < 2 )); then
        printf '%s' "$actions"
    else
        printf '%s%s%s' "$actions" "$(_awen_repeat ' ' "$gap")" "$logo"
    fi
}
