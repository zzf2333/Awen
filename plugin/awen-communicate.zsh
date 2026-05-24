#!/usr/bin/env zsh
# Awen — Request building, response parsing, async AI, hooks

_awen_build_context_json() {
    local cwd="$(pwd)"
    local git_branch=""
    if command -v git &>/dev/null; then
        git_branch=$(git rev-parse --abbrev-ref HEAD 2>/dev/null)
    fi
    local last_cmd="${_AWEN_LAST_COMMAND:-}"
    local last_exit="${_AWEN_LAST_EXIT_CODE:-0}"
    local last_stderr=""
    if [[ -f "$_AWEN_LAST_STDERR_FILE" ]]; then
        last_stderr=$(head -c $_AWEN_STDERR_MAX "$_AWEN_LAST_STDERR_FILE" 2>/dev/null | tr '\n' ' ' | tr '"' "'" )
    fi

    if [[ "$_AWEN_HAS_JQ" == "1" ]]; then
        jq -cn \
            --arg cwd "$cwd" \
            --arg last_cmd "${last_cmd:-}" \
            --argjson exit_code "${last_exit:-0}" \
            --arg stderr "${last_stderr:-}" \
            --arg branch "${git_branch:-}" \
            '{cwd: $cwd,
              last_command: (if $last_cmd == "" then null else $last_cmd end),
              last_exit_code: $exit_code,
              last_stderr: (if $stderr == "" then null else $stderr end),
              git_branch: (if $branch == "" then null else $branch end),
              git_status: null, session_commands: [], env_hints: []}' 2>/dev/null
    else
        local esc_cwd=$(_awen_json_escape "$cwd")
        local cmd_json="null"
        [[ -n "$last_cmd" ]] && cmd_json="\"$(_awen_json_escape "$last_cmd")\""
        local stderr_json="null"
        [[ -n "$last_stderr" ]] && stderr_json="\"$(_awen_json_escape "$last_stderr")\""
        local branch_json="null"
        [[ -n "$git_branch" ]] && branch_json="\"$(_awen_json_escape "$git_branch")\""
        printf '{"cwd":"%s","last_command":%s,"last_exit_code":%s,"last_stderr":%s,"git_branch":%s,"git_status":null,"session_commands":[],"env_hints":[]}' \
            "$esc_cwd" "$cmd_json" "${last_exit:-0}" "$stderr_json" "$branch_json"
    fi
}

_awen_build_request() {
    local input="$1" cursor="$2" skip_ai="$3"

    if [[ "$_AWEN_HAS_JQ" == "1" ]]; then
        local ctx=$(_awen_build_context_json)
        jq -cn \
            --arg input "$input" \
            --argjson cursor "$cursor" \
            --argjson skip_ai "$skip_ai" \
            --argjson ctx "$ctx" \
            '{type: "suggest", input: $input, cursor_pos: $cursor, skip_ai: $skip_ai, context: $ctx}' 2>/dev/null
    else
        local esc_input=$(_awen_json_escape "$input")
        local ctx=$(_awen_build_context_json)
        printf '{"type":"suggest","input":"%s","cursor_pos":%d,"skip_ai":%s,"context":%s}' \
            "$esc_input" "$cursor" "$skip_ai" "$ctx"
    fi
}

_awen_build_nl_request() {
    local query="$1"

    if [[ "$_AWEN_HAS_JQ" == "1" ]]; then
        local ctx=$(_awen_build_context_json)
        jq -cn \
            --arg query "$query" \
            --argjson ctx "$ctx" \
            '{type: "nl_generate", query: $query, context: $ctx}' 2>/dev/null
    else
        local esc_query=$(_awen_json_escape "$query")
        local ctx=$(_awen_build_context_json)
        printf '{"type":"nl_generate","query":"%s","context":%s}' \
            "$esc_query" "$ctx"
    fi
}

_awen_apply_response() {
    local response="$1"

    if [[ -z "$response" ]]; then
        _awen_menu_reset
        _awen_hl_clear
        POSTDISPLAY=""
        _AWEN_SUGGESTION=""
        return
    fi

    local hint_text="" warning_text=""

    if [[ "$_AWEN_HAS_JQ" == "1" ]]; then
        hint_text=$(printf '%s\n' "$response" | jq -r '.hint.text // empty' 2>/dev/null)
        warning_text=$(printf '%s\n' "$response" | jq -r '.warning.text // empty' 2>/dev/null)
    else
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

    if [[ -z "${AWEN_UI_MODE:-}" ]]; then
        local response_mode=""
        if [[ "$_AWEN_HAS_JQ" == "1" ]]; then
            response_mode=$(printf '%s\n' "$response" | jq -r '.ui_mode // empty' 2>/dev/null)
        else
            if [[ "$response" == *'"ui_mode":"'* ]]; then
                local tmp="${response#*\"ui_mode\":\"}"
                response_mode="${tmp%%\"*}"
            fi
        fi
        [[ -n "$response_mode" ]] && _AWEN_UI_MODE="$response_mode"
    fi

    if [[ "$_AWEN_HAS_JQ" == "1" ]]; then
        _AWEN_NEED_AI=$(printf '%s\n' "$response" | jq -r '.need_ai // "false"' 2>/dev/null)
    else
        if [[ "$response" == *'"need_ai":true'* ]]; then
            _AWEN_NEED_AI="true"
        else
            _AWEN_NEED_AI="false"
        fi
    fi

    _AWEN_MENU_TEXTS=()
    _AWEN_MENU_SOURCES=()
    _AWEN_MENU_DESCS=()

    if [[ "$_AWEN_HAS_JQ" == "1" ]]; then
        local s_text s_source s_desc
        while IFS=$'\t' read -r s_text s_source s_desc; do
            [[ -z "$s_text" ]] && continue
            _AWEN_MENU_TEXTS+=("$s_text")
            _AWEN_MENU_SOURCES+=("$s_source")
            _AWEN_MENU_DESCS+=("$s_desc")
        done < <(printf '%s\n' "$response" | jq -r '.suggestions[] | "\(.text)\t\(.source)\t\(.description // "")"' 2>/dev/null)
    else
        local _remaining="${response#*\"suggestions\":\[}"
        while [[ "$_remaining" == *'"text":"'* ]]; do
            local _after_text="${_remaining#*\"text\":\"}"
            local s_text=$(_awen_extract_json_value "$_after_text")
            local s_source="history"
            local s_desc=""
            if [[ "$_remaining" == *'"source":"'* ]]; then
                local _after_src="${_remaining#*\"source\":\"}"
                s_source=$(_awen_extract_json_value "$_after_src")
            fi
            local _obj_end="${_after_text#*\}}"
            local _obj_chunk="${_after_text%"$_obj_end"}"
            if [[ "$_obj_chunk" == *'"description":"'* ]]; then
                local _after_desc="${_obj_chunk#*\"description\":\"}"
                s_desc=$(_awen_extract_json_value "$_after_desc")
            fi
            if [[ -n "$s_text" ]]; then
                _AWEN_MENU_TEXTS+=("$s_text")
                _AWEN_MENU_SOURCES+=("${s_source:-history}")
                _AWEN_MENU_DESCS+=("$s_desc")
            fi
            _remaining="${_after_text#*\}}"
        done
    fi

    local count=${#_AWEN_MENU_TEXTS[@]}

    _AWEN_MENU_FULL_CMDS=()
    local input="$BUFFER"
    local i
    for (( i=1; i<=count; i++ )); do
        _AWEN_MENU_FULL_CMDS+=("$(_awen_reconstruct_full_cmd "$input" "${_AWEN_MENU_TEXTS[$i]}" "${_AWEN_MENU_SOURCES[$i]}")")
    done

    local failure_idx=0
    if [[ -n "$_AWEN_HINT" ]]; then
        for (( i=1; i<=count; i++ )); do
            if [[ "${_AWEN_MENU_SOURCES[$i]}" == "failure" ]]; then
                failure_idx=$i
                break
            fi
        done
    fi

    if [[ "$_AWEN_UI_MODE" == "minimal" ]]; then
        if [[ -n "$_AWEN_WARNING" ]]; then
            zle -M "⚠ $_AWEN_WARNING"
        fi
        if (( failure_idx > 0 )); then
            _awen_menu_reset
            _awen_render_ghost "${_AWEN_MENU_TEXTS[$failure_idx]}" "${_AWEN_MENU_SOURCES[$failure_idx]}"
        elif [[ $count -ge 1 ]]; then
            _awen_menu_reset
            _awen_render_ghost "${_AWEN_MENU_TEXTS[1]}" "${_AWEN_MENU_SOURCES[1]}"
        else
            _awen_menu_reset
            _awen_hl_clear
            POSTDISPLAY=""
            _AWEN_SUGGESTION=""
        fi
    else
        if [[ -n "$_AWEN_WARNING" ]]; then
            _awen_render_risk_panel "$_AWEN_WARNING"
        elif (( failure_idx > 0 )); then
            _AWEN_MENU_COUNT=$count
            _awen_render_failure_panel "$failure_idx"
        elif [[ "$_AWEN_MENU_ENABLED" == "1" && $count -ge 2 ]]; then
            _AWEN_MENU_COUNT=$count
            _AWEN_MENU_INDEX=1
            _AWEN_MENU_ACTIVE=1
            _AWEN_SUGGESTION="${_AWEN_MENU_FULL_CMDS[1]}"
            _awen_render_menu
        elif [[ $count -ge 1 ]]; then
            _awen_menu_reset
            _awen_render_ghost "${_AWEN_MENU_TEXTS[1]}" "${_AWEN_MENU_SOURCES[1]}"
        else
            _awen_menu_reset
            _awen_hl_clear
            POSTDISPLAY=""
            _AWEN_SUGGESTION=""
        fi
    fi

    if [[ -z "$_AWEN_WARNING" ]]; then
        _awen_render_hint
    fi
}

_awen_suggest_next() {
    [[ ! -S "$_AWEN_SOCKET" ]] && return
    [[ -n "$BUFFER" ]] && return

    local request=$(_awen_build_request "" 0 true)
    local response
    response=$(_awen_send_nc "$request")
    [[ -z "$response" ]] && return

    local has_hint=""
    if [[ "$_AWEN_HAS_JQ" == "1" ]]; then
        has_hint=$(printf '%s\n' "$response" | jq -r 'if .hint != null then "1" else "" end' 2>/dev/null)
    elif [[ "$response" == *'"hint":'*'"text":"'* ]]; then
        has_hint="1"
    fi
    [[ -z "$has_hint" ]] && return

    _awen_apply_response "$response"
}

_awen_suggest_local() {
    _awen_cancel_delete_debounce
    if [[ -z "$BUFFER" || ! -S "$_AWEN_SOCKET" ]]; then
        _awen_hl_clear
        POSTDISPLAY=""
        _AWEN_SUGGESTION=""
        _awen_clear_hint
        _awen_cancel_pending_ai
        return
    fi

    if [[ "$BUFFER" == "# "* && ${#BUFFER} -ge 4 ]]; then
        _awen_check_ai_result
        _awen_schedule_ai
        return
    fi

    local now_ms=$(_awen_now_ms)
    local elapsed=$(( now_ms - _AWEN_LAST_LOCAL_MS ))
    if (( elapsed < _AWEN_LOCAL_THROTTLE_MS )); then
        [[ "$_AWEN_NEED_AI" != "false" ]] && _awen_schedule_ai
        return
    fi
    _AWEN_LAST_LOCAL_MS=$now_ms

    _awen_check_ai_result

    local request=$(_awen_build_request "$BUFFER" "$CURSOR" true)
    local response
    response=$(_awen_send_nc "$request")
    _awen_apply_response "$response"

    [[ "$_AWEN_NEED_AI" != "false" ]] && _awen_schedule_ai
}

_awen_cancel_pending_ai() {
    if [[ -n "$_AWEN_AI_PID" ]]; then
        kill "$_AWEN_AI_PID" 2>/dev/null
        _AWEN_AI_PID=""
    fi
    _AWEN_AI_LOADING=0
}

_awen_schedule_delete_debounce() {
    _awen_cancel_delete_debounce
    exec {_AWEN_DELETE_FD}< <(sleep 0.1; echo x)
    zle -F $_AWEN_DELETE_FD _awen_delete_debounce_callback
}

_awen_cancel_delete_debounce() {
    if (( _AWEN_DELETE_FD > 0 )); then
        zle -F $_AWEN_DELETE_FD 2>/dev/null
        exec {_AWEN_DELETE_FD}<&- 2>/dev/null
        _AWEN_DELETE_FD=0
    fi
}

_awen_delete_debounce_callback() {
    local fd=$1
    zle -F $fd 2>/dev/null
    exec {fd}<&- 2>/dev/null
    _AWEN_DELETE_FD=0
    _awen_suggest_local
    zle -R
}

_awen_schedule_ai() {
    _awen_cancel_pending_ai

    local is_error_recovery=0
    if [[ -z "$BUFFER" && -n "$_AWEN_LAST_EXIT_CODE" && "$_AWEN_LAST_EXIT_CODE" -ne 0 ]]; then
        is_error_recovery=1
    fi

    if (( ! is_error_recovery )); then
        [[ ${#BUFFER} -lt 2 ]] && return
    fi
    [[ ! -S "$_AWEN_SOCKET" ]] && return
    command -v socat &>/dev/null || return

    local delay="$_AWEN_AI_DELAY"
    local request
    if [[ "$BUFFER" == "# "* ]]; then
        delay=0.3
        local query="${BUFFER#\# }"
        request=$(_awen_build_nl_request "$query")
    else
        request=$(_awen_build_request "$BUFFER" "$CURSOR" false)
    fi

    _AWEN_AI_SNAPSHOT="$BUFFER"
    (( _AWEN_AI_SEQ++ ))
    _AWEN_AI_ACTIVE_SEQ=$_AWEN_AI_SEQ
    local socket="$_AWEN_SOCKET"
    local seq=$_AWEN_AI_SEQ
    local result_file="$_AWEN_AI_RESULT_FILE"
    local token_file="${TMPDIR:-/tmp}/.awen-ai-token-$$"

    echo "$seq" > "$token_file"

    local parent_pid=$$

    (
        sleep "$delay" 2>/dev/null
        [[ "$(cat "$token_file" 2>/dev/null)" != "$seq" ]] && exit 0
        local result
        result=$(printf '%s\n' "$request" | socat -t 35 - UNIX-CONNECT:"$socket" 2>/dev/null)
        if [[ -n "$result" ]]; then
            printf '%s\n' "$result" > "$result_file" 2>/dev/null
            kill -USR1 "$parent_pid" 2>/dev/null
        fi
    ) &!
    _AWEN_AI_PID=$!
    _AWEN_AI_LOADING=1
    if (( _AWEN_MENU_ACTIVE )); then
        local _has_failure=0
        local _fi
        for (( _fi=1; _fi<=${#_AWEN_MENU_SOURCES[@]}; _fi++ )); do
            [[ "${_AWEN_MENU_SOURCES[$_fi]}" == "failure" ]] && _has_failure=1 && break
        done
        if (( _has_failure && _AWEN_MENU_INDEX == _fi )); then
            _awen_render_failure_panel "$_fi"
        else
            _awen_render_menu
        fi
        zle -R
    fi
}

_awen_check_ai_result() {
    [[ ! -s "$_AWEN_AI_RESULT_FILE" ]] && return
    local response
    response=$(<"$_AWEN_AI_RESULT_FILE")
    : > "$_AWEN_AI_RESULT_FILE"
    _AWEN_AI_LOADING=0
    [[ -z "$response" ]] && return

    if [[ "$BUFFER" != "$_AWEN_AI_SNAPSHOT" ]]; then
        return
    fi

    if [[ "$BUFFER" == "# "* ]]; then
        local nl_cmd=""
        if [[ "$_AWEN_HAS_JQ" == "1" ]]; then
            nl_cmd=$(printf '%s' "$response" | jq -r '.command // empty' 2>/dev/null)
        else
            if [[ "$response" == *'"command":"'* ]]; then
                local _after="${response#*\"command\":\"}"
                nl_cmd=$(_awen_extract_json_value "$_after")
            fi
        fi
        if [[ -n "$nl_cmd" ]]; then
            _awen_render_nl_suggestion "$nl_cmd"
        fi
        return
    fi

    local prev_selected=""
    if (( _AWEN_MENU_ACTIVE )); then
        prev_selected="${_AWEN_MENU_TEXTS[$_AWEN_MENU_INDEX]}"
    fi

    _awen_apply_response "$response"

    if [[ -n "$prev_selected" ]] && (( _AWEN_MENU_ACTIVE )); then
        local i
        for (( i=1; i<=${#_AWEN_MENU_TEXTS[@]}; i++ )); do
            if [[ "${_AWEN_MENU_TEXTS[$i]}" == "$prev_selected" ]]; then
                _AWEN_MENU_INDEX=$i
                _AWEN_SUGGESTION="${_AWEN_MENU_FULL_CMDS[$i]}"
                _awen_render_menu
                break
            fi
        done
    fi

    zle -R
}

_awen_on_ai_signal() {
    _awen_check_ai_result
}

_awen_precmd() {
    _AWEN_LAST_EXIT_CODE=$?

    if [[ -n "${_AWEN_STDERR_BACKUP:-}" ]]; then
        exec 2>&${_AWEN_STDERR_BACKUP}
        exec {_AWEN_STDERR_BACKUP}>&-
        unset _AWEN_STDERR_BACKUP
        sleep 0.01
    fi

    if [[ -n "$_AWEN_LAST_COMMAND" && -S "$_AWEN_SOCKET" ]]; then
        local stderr_content=""
        if [[ -f "$_AWEN_LAST_STDERR_FILE" && -s "$_AWEN_LAST_STDERR_FILE" ]]; then
            stderr_content=$(head -c $_AWEN_STDERR_MAX "$_AWEN_LAST_STDERR_FILE" 2>/dev/null | tr '\n' ' ' | tr '"' "'" )
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

        _awen_send_nc "$record_request" &>/dev/null &!
    fi
}

_awen_preexec() {
    _AWEN_LAST_COMMAND="$1"
    _AWEN_FAILURE_SHOWN=0
    : > "$_AWEN_LAST_STDERR_FILE"
    if [[ "${AWEN_CAPTURE_STDERR:-1}" == "1" ]]; then
        exec {_AWEN_STDERR_BACKUP}>&2
        exec 2> >(tee "$_AWEN_LAST_STDERR_FILE" >&${_AWEN_STDERR_BACKUP})
    fi
}
