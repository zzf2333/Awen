#!/usr/bin/env zsh
# Awen — Terminal Intelligence Layer
# Smart when you need it. Silent when you don't.

typeset -g _AWEN_SUGGESTION=""
typeset -g _AWEN_HINT=""
typeset -g _AWEN_WARNING=""
typeset -g _AWEN_LAST_STDERR_FILE="${TMPDIR:-/tmp}/awen-stderr-$$"
typeset -g _AWEN_SOCKET=""
typeset -g _AWEN_BIN=""
typeset -g _AWEN_GHOST_HIGHLIGHT=""

# Extract a JSON string value, handling escaped quotes
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

_awen_send() {
    if [[ ! -S "$_AWEN_SOCKET" ]]; then
        return 1
    fi
    local request="$1"
    echo "$request" | socat - UNIX-CONNECT:"$_AWEN_SOCKET" 2>/dev/null
}

_awen_send_nc() {
    if [[ ! -S "$_AWEN_SOCKET" ]]; then
        return 1
    fi
    local request="$1"
    # Try socat first, fall back to direct zsh TCP
    if command -v socat &>/dev/null; then
        echo "$request" | socat - UNIX-CONNECT:"$_AWEN_SOCKET" 2>/dev/null
    else
        # Use zsh's built-in zsocket if available
        if zmodload zsh/net/socket 2>/dev/null; then
            local fd
            zsocket "$_AWEN_SOCKET" && fd=$REPLY
            if [[ -n "$fd" ]]; then
                echo "$request" >&$fd
                local response
                read -r response <&$fd
                exec {fd}>&-
                echo "$response"
            fi
        fi
    fi
}

_awen_clear_ghost() {
    if [[ -n "$_AWEN_SUGGESTION" ]]; then
        _AWEN_SUGGESTION=""
        if [[ -n "$_AWEN_GHOST_HIGHLIGHT" ]]; then
            region_highlight=("${(@)region_highlight:#$_AWEN_GHOST_HIGHLIGHT}")
            _AWEN_GHOST_HIGHLIGHT=""
        fi
        POSTDISPLAY=""
        zle -R
    fi
}

_awen_clear_hint() {
    _AWEN_HINT=""
    _AWEN_WARNING=""
}

_awen_render_ghost() {
    local suggestion="$1"

    # Remove previous ghost highlight
    if [[ -n "$_AWEN_GHOST_HIGHLIGHT" ]]; then
        region_highlight=("${(@)region_highlight:#$_AWEN_GHOST_HIGHLIGHT}")
        _AWEN_GHOST_HIGHLIGHT=""
    fi

    if [[ -z "$suggestion" ]]; then
        _AWEN_SUGGESTION=""
        POSTDISPLAY=""
        return
    fi

    local input="$BUFFER"
    local full_suggestion=""

    # Reconstruct full command from suggestion
    if [[ "$suggestion" == "$input"* ]]; then
        # Full prefix match (e.g., history: "git checkout" starts with "git ch")
        full_suggestion="$suggestion"
    else
        local last_word="${input##* }"
        if [[ -n "$last_word" && "$suggestion" == "$last_word"* ]]; then
            # Word completion (e.g., specs: "checkout" completes "ch")
            full_suggestion="${input%$last_word}${suggestion}"
        elif [[ "$input" == *" " ]]; then
            # Input ends with space, append suggestion
            full_suggestion="${input}${suggestion}"
        else
            # Append with space (e.g., AI: "pods" after "kubectl get")
            full_suggestion="${input} ${suggestion}"
        fi
    fi

    _AWEN_SUGGESTION="$full_suggestion"

    local ghost_part="${full_suggestion#$input}"
    if [[ -n "$ghost_part" ]]; then
        POSTDISPLAY="$ghost_part"
        _AWEN_GHOST_HIGHLIGHT="$#BUFFER $(( $#BUFFER + $#ghost_part )) fg=242"
        region_highlight+=("$_AWEN_GHOST_HIGHLIGHT")
    else
        POSTDISPLAY=""
    fi
}

_awen_render_hint() {
    if [[ -n "$_AWEN_WARNING" ]]; then
        # Show warning above current line
        local warning_text="  ╭ ⚠ ${_AWEN_WARNING}"
        zle -M "$warning_text"
    elif [[ -n "$_AWEN_HINT" ]]; then
        local hint_text="  ╭ ℹ ${_AWEN_HINT}"
        zle -M "$hint_text"
    fi
}

_awen_suggest() {
    if [[ -z "$BUFFER" || ! -S "$_AWEN_SOCKET" ]]; then
        POSTDISPLAY=""
        _AWEN_SUGGESTION=""
        _awen_clear_hint
        return
    fi

    local last_exit="${_AWEN_LAST_EXIT_CODE:-0}"
    local last_stderr=""
    if [[ -f "$_AWEN_LAST_STDERR_FILE" ]]; then
        last_stderr=$(head -c 500 "$_AWEN_LAST_STDERR_FILE" 2>/dev/null | tr '\n' ' ' | tr '"' "'" )
    fi

    local cwd="$(pwd)"
    local git_branch=""
    if command -v git &>/dev/null; then
        git_branch=$(git rev-parse --abbrev-ref HEAD 2>/dev/null)
    fi

    local last_cmd="${_AWEN_LAST_COMMAND:-}"

    # Build JSON request
    local request
    if [[ "$_AWEN_HAS_JQ" == "1" ]]; then
        request=$(jq -cn \
            --arg input "$BUFFER" \
            --argjson cursor "$CURSOR" \
            --arg cwd "$cwd" \
            --arg last_cmd "${last_cmd:-}" \
            --argjson exit_code "${last_exit:-0}" \
            --arg stderr "${last_stderr:-}" \
            --arg branch "${git_branch:-}" \
            '{type: "suggest", input: $input, cursor_pos: $cursor, context: {
                cwd: $cwd,
                last_command: (if $last_cmd == "" then null else $last_cmd end),
                last_exit_code: $exit_code,
                last_stderr: (if $stderr == "" then null else $stderr end),
                git_branch: (if $branch == "" then null else $branch end),
                git_status: null, session_commands: [], env_hints: []
            }}' 2>/dev/null)
    else
        local esc_input=$(_awen_json_escape "$BUFFER")
        local esc_cwd=$(_awen_json_escape "$cwd")
        local cmd_json="null"
        if [[ -n "$last_cmd" ]]; then
            cmd_json="\"$(_awen_json_escape "$last_cmd")\""
        fi
        local stderr_json="null"
        if [[ -n "$last_stderr" ]]; then
            stderr_json="\"$(_awen_json_escape "$last_stderr")\""
        fi
        local branch_json="null"
        if [[ -n "$git_branch" ]]; then
            branch_json="\"$(_awen_json_escape "$git_branch")\""
        fi
        request=$(printf '{"type":"suggest","input":"%s","cursor_pos":%d,"context":{"cwd":"%s","last_command":%s,"last_exit_code":%s,"last_stderr":%s,"git_branch":%s,"git_status":null,"session_commands":[],"env_hints":[]}}' \
            "$esc_input" "$CURSOR" "$esc_cwd" "$cmd_json" "${last_exit:-0}" "$stderr_json" "$branch_json")
    fi

    local response
    response=$(_awen_send_nc "$request")

    if [[ -z "$response" ]]; then
        POSTDISPLAY=""
        return
    fi

    # Parse response
    local suggestion_text=""
    local hint_text=""
    local warning_text=""

    if [[ "$_AWEN_HAS_JQ" == "1" ]]; then
        suggestion_text=$(echo "$response" | jq -r '.suggestions[0].text // empty' 2>/dev/null)
        hint_text=$(echo "$response" | jq -r '.hint.text // empty' 2>/dev/null)
        warning_text=$(echo "$response" | jq -r '.warning.text // empty' 2>/dev/null)
    else
        # Fallback: manual parsing with escaped-quote handling
        if [[ "$response" == *'"suggestions":'*'"text":"'* ]]; then
            local tmp="${response#*\"suggestions\":*\"text\":\"}"
            suggestion_text=$(_awen_extract_json_value "$tmp")
        fi
        if [[ "$response" == *'"hint":'*'"text":"'* ]]; then
            local tmp="${response#*\"hint\":*\"text\":\"}"
            hint_text=$(_awen_extract_json_value "$tmp")
        fi
        if [[ "$response" == *'"warning":'*'"text":"'* ]]; then
            local tmp="${response#*\"warning\":*\"text\":\"}"
            warning_text=$(_awen_extract_json_value "$tmp")
        fi
    fi

    _AWEN_WARNING="$warning_text"
    _AWEN_HINT="$hint_text"

    if [[ -n "$suggestion_text" ]]; then
        _awen_render_ghost "$suggestion_text"
    else
        POSTDISPLAY=""
        _AWEN_SUGGESTION=""
    fi

    _awen_render_hint
}

# Accept full ghost text suggestion
_awen_accept() {
    if [[ -n "$_AWEN_SUGGESTION" ]]; then
        BUFFER="$_AWEN_SUGGESTION"
        CURSOR=${#BUFFER}
        _AWEN_SUGGESTION=""
        POSTDISPLAY=""
        _awen_clear_hint
    else
        # Default right arrow behavior
        zle forward-char
    fi
}

# Accept next word from ghost text
_awen_accept_word() {
    if [[ -n "$_AWEN_SUGGESTION" ]]; then
        local input="$BUFFER"
        local remaining="${_AWEN_SUGGESTION#$input}"

        # Get next word (up to next space)
        local next_word="${remaining%% *}"
        if [[ "$next_word" == "$remaining" ]]; then
            BUFFER="$_AWEN_SUGGESTION"
            _AWEN_SUGGESTION=""
            POSTDISPLAY=""
        else
            BUFFER="${input}${next_word} "
            _awen_render_ghost "$_AWEN_SUGGESTION"
        fi
        CURSOR=${#BUFFER}
    else
        zle forward-word
    fi
}

# Dismiss suggestion
_awen_dismiss() {
    if [[ -n "$_AWEN_SUGGESTION" || -n "$_AWEN_HINT" || -n "$_AWEN_WARNING" ]]; then
        _AWEN_SUGGESTION=""
        POSTDISPLAY=""
        _awen_clear_hint
        zle -M ""
        zle -R
    else
        # Default Esc behavior (vi mode or cancel)
        zle send-break
    fi
}

# Hook: after each command finishes
_awen_precmd() {
    _AWEN_LAST_EXIT_CODE=$?

    # Restore original stderr if we redirected it
    if [[ -n "${_AWEN_STDERR_BACKUP:-}" ]]; then
        exec 2>&${_AWEN_STDERR_BACKUP}
        exec {_AWEN_STDERR_BACKUP}>&-
        unset _AWEN_STDERR_BACKUP
        # Allow tee subprocess to flush
        sleep 0.01
    fi

    # Record command to daemon
    if [[ -n "$_AWEN_LAST_COMMAND" && -S "$_AWEN_SOCKET" ]]; then
        local stderr_content=""
        if [[ -f "$_AWEN_LAST_STDERR_FILE" && -s "$_AWEN_LAST_STDERR_FILE" ]]; then
            stderr_content=$(head -c 500 "$_AWEN_LAST_STDERR_FILE" 2>/dev/null | tr '\n' ' ' | tr '"' "'" )
        fi

        local record_request
        if [[ "$_AWEN_HAS_JQ" == "1" ]]; then
            record_request=$(jq -cn \
                --arg cmd "$_AWEN_LAST_COMMAND" \
                --argjson exit "$_AWEN_LAST_EXIT_CODE" \
                --arg stderr "${stderr_content:-}" \
                --arg cwd "$(pwd)" \
                '{type: "record", command: $cmd, exit_code: $exit,
                  stderr: (if $stderr == "" then null else $stderr end),
                  cwd: $cwd}' 2>/dev/null)
        else
            local esc_cmd=$(_awen_json_escape "$_AWEN_LAST_COMMAND")
            local esc_cwd=$(_awen_json_escape "$(pwd)")
            local stderr_json="null"
            if [[ -n "$stderr_content" ]]; then
                stderr_json="\"$(_awen_json_escape "$stderr_content")\""
            fi
            record_request=$(printf '{"type":"record","command":"%s","exit_code":%d,"stderr":%s,"cwd":"%s"}' \
                "$esc_cmd" "$_AWEN_LAST_EXIT_CODE" "$stderr_json" "$esc_cwd")
        fi

        # Send async, don't block prompt
        _awen_send_nc "$record_request" &>/dev/null &!
    fi
}

# Hook: before each command runs
_awen_preexec() {
    _AWEN_LAST_COMMAND="$1"
    : > "$_AWEN_LAST_STDERR_FILE"
    # Stderr capture is experimental — opt-in via AWEN_CAPTURE_STDERR=1
    if [[ "${AWEN_CAPTURE_STDERR:-0}" == "1" ]]; then
        exec {_AWEN_STDERR_BACKUP}>&2
        exec 2> >(tee "$_AWEN_LAST_STDERR_FILE" >&${_AWEN_STDERR_BACKUP})
    fi
}

# Self-insert wrapper: trigger suggest after each keystroke
_awen_self_insert() {
    zle .self-insert
    _awen_suggest
}

_awen_backward_delete_char() {
    zle .backward-delete-char
    _awen_suggest
}

# Initialize Awen
awen_init() {
    _awen_find_binary
    _awen_find_socket

    if [[ -z "$_AWEN_BIN" ]]; then
        echo "awen: binary not found. Install with: cargo install --path ."
        return 1
    fi

    # Detect jq for robust JSON parsing
    if command -v jq &>/dev/null; then
        typeset -g _AWEN_HAS_JQ=1
    else
        typeset -g _AWEN_HAS_JQ=0
    fi

    # Cleanup stderr capture file on shell exit
    trap 'rm -f "$_AWEN_LAST_STDERR_FILE" 2>/dev/null' EXIT

    _awen_ensure_daemon

    # Register ZLE widgets
    zle -N _awen_self_insert
    zle -N _awen_backward_delete_char
    zle -N _awen_accept
    zle -N _awen_accept_word
    zle -N _awen_dismiss
    zle -N _awen_suggest

    # Keybinding setup (disable with AWEN_ENABLE_KEYBIND_OVERRIDE=0)
    if [[ "${AWEN_ENABLE_KEYBIND_OVERRIDE:-1}" == "1" ]]; then
        bindkey -M main '\e[C' _awen_accept          # Right arrow
        bindkey -M main '\e[1;5C' _awen_accept_word  # Ctrl+Right
        bindkey -M main '\e[27;5;67~' _awen_accept_word  # Ctrl+Right (alternate)
        bindkey -M main '\e\e[C' _awen_accept_word   # Alt+Right (fallback)
        bindkey -M main '\e[Z' _awen_dismiss          # Shift+Tab dismiss
        bindkey -M main '^[' _awen_dismiss            # Esc

        # Override self-insert to trigger suggestions on every keystroke
        local -a printable
        printable=({a..z} {A..Z} {0..9} ' ' '-' '_' '.' '/' '~' ':' '=' '+' '@' ',' ';' '!' '?' '#' '$' '%' '^' '&' '*' '(' ')' '[' ']' '{' '}' '<' '>' '|' "'" '"' '`' '\\')
        local key
        for key in "${printable[@]}"; do
            bindkey -M main -- "$key" _awen_self_insert
        done

        # Backspace also triggers re-suggest
        bindkey -M main '^?' _awen_backward_delete_char   # Backspace
        bindkey -M main '^H' _awen_backward_delete_char   # Ctrl+H
    fi

    # Register hooks
    autoload -Uz add-zsh-hook
    add-zsh-hook precmd _awen_precmd
    add-zsh-hook preexec _awen_preexec
}

# Auto-initialize
awen_init
