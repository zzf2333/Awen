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
typeset -g _AWEN_GHOST_STYLE="fg=244"
typeset -g _AWEN_STYLE_DIM="fg=244"
typeset -g _AWEN_STYLE_MUTED="fg=250"
typeset -g _AWEN_STYLE_TEXT="fg=255"
typeset -g _AWEN_STYLE_SELECTED="fg=255,bold,bg=236"
typeset -g _AWEN_STYLE_PANEL="fg=240"
typeset -g _AWEN_STYLE_PANEL_BG="bg=234"
typeset -g _AWEN_STYLE_HISTORY="fg=146"
typeset -g _AWEN_STYLE_SPEC="fg=69"
typeset -g _AWEN_STYLE_AI="fg=177"
typeset -g _AWEN_STYLE_RISK="fg=220"
typeset -g _AWEN_STYLE_FIX="fg=108"

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

_awen_send_nc() {
    if [[ ! -S "$_AWEN_SOCKET" ]]; then
        return 1
    fi
    local request="$1"
    # Try socat first, fall back to direct zsh TCP
    if command -v socat &>/dev/null; then
        printf '%s\n' "$request" | socat -T 0.1 -t 0.5 - UNIX-CONNECT:"$_AWEN_SOCKET" 2>/dev/null
    else
        # Use zsh's built-in zsocket if available
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

_awen_menu_reset() {
    _AWEN_MENU_ACTIVE=0
    _AWEN_MENU_INDEX=1
    _AWEN_MENU_COUNT=0
    _AWEN_MENU_TEXTS=()
    _AWEN_MENU_SOURCES=()
    _AWEN_MENU_DESCS=()
    _AWEN_MENU_FULL_CMDS=()
}

_awen_remove_ghost_highlight() {
    region_highlight=("${(@)region_highlight:#*$_AWEN_GHOST_STYLE}")
    region_highlight=("${(@)region_highlight:#*standout*}")
    region_highlight=("${(@)region_highlight:#*bg=237*}")
    region_highlight=("${(@)region_highlight:#*bg=236*}")
    region_highlight=("${(@)region_highlight:#*bg=235*}")
    region_highlight=("${(@)region_highlight:#*bg=234*}")
    region_highlight=("${(@)region_highlight:#*fg=252*}")
    region_highlight=("${(@)region_highlight:#*fg=253*}")
    region_highlight=("${(@)region_highlight:#*fg=250*}")
    region_highlight=("${(@)region_highlight:#*fg=255*}")
    region_highlight=("${(@)region_highlight:#*fg=245*}")
    region_highlight=("${(@)region_highlight:#*fg=244*}")
    region_highlight=("${(@)region_highlight:#*fg=240*}")
    region_highlight=("${(@)region_highlight:#*fg=241*}")
    region_highlight=("${(@)region_highlight:#*fg=146*}")
    region_highlight=("${(@)region_highlight:#*fg=75*}")
    region_highlight=("${(@)region_highlight:#*fg=69*}")
    region_highlight=("${(@)region_highlight:#*fg=177*}")
    region_highlight=("${(@)region_highlight:#*fg=220*}")
    region_highlight=("${(@)region_highlight:#*fg=114*}")
    region_highlight=("${(@)region_highlight:#*fg=108*}")
    region_highlight=("${(@)region_highlight:#*fg=214*}")
    region_highlight=("${(@)region_highlight:#*fg=82*}")
    region_highlight=("${(@)region_highlight:#*fg=82,bold*}")
    _AWEN_GHOST_HIGHLIGHT=""
}

_awen_source_label() {
    case "$1" in
        history) printf '%s' "history" ;;
        specs)   printf '%s' "spec" ;;
        ai)      printf '%s' "ai" ;;
        failure) printf '%s' "fix" ;;
        *)       printf '%s' "$1" ;;
    esac
}

_awen_source_style() {
    case "$1" in
        history) printf '%s' "$_AWEN_STYLE_HISTORY" ;;
        specs)   printf '%s' "$_AWEN_STYLE_SPEC" ;;
        ai)      printf '%s' "$_AWEN_STYLE_AI" ;;
        failure) printf '%s' "$_AWEN_STYLE_FIX" ;;
        risk)    printf '%s' "$_AWEN_STYLE_RISK" ;;
        *)       printf '%s' "$_AWEN_STYLE_DIM" ;;
    esac
}

_awen_source_title() {
    case "$1" in
        history) printf '%s' "history" ;;
        specs)   printf '%s' "options" ;;
        ai)      printf '%s' "ai suggestions" ;;
        failure) printf '%s' "fix" ;;
        *)       printf '%s' "suggestions" ;;
    esac
}

_awen_source_icon() {
    case "$1" in
        history) printf '%s' "↺" ;;
        specs)   printf '%s' "◇" ;;
        ai)      printf '%s' "✦" ;;
        failure) printf '%s' "✓" ;;
        risk)    printf '%s' "!" ;;
        *)       printf '%s' "•" ;;
    esac
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
    local logo="Awen"
    local actions="↑↓ select   ↵ accept   → next word   esc dismiss"
    local gap=$(( width - ${#actions} - ${#logo} ))
    if (( gap < 2 )); then
        printf '%s' "$actions"
    else
        printf '%s%s%s' "$actions" "$(_awen_repeat ' ' "$gap")" "$logo"
    fi
}

_awen_footer_line() {
    local width="$1" actions="$2"
    local logo="Awen"
    local gap=$(( width - ${#actions} - ${#logo} ))
    if (( gap < 2 )); then
        printf '%s' "$actions"
    else
        printf '%s%s%s' "$actions" "$(_awen_repeat ' ' "$gap")" "$logo"
    fi
}

_awen_reconstruct_full_cmd() {
    local input="$1" suggestion="$2" source="${3:-}"
    if [[ "$source" == "history" || "$source" == "failure" ]]; then
        printf '%s' "$suggestion"
        return
    fi
    if [[ "$suggestion" == "$input"* ]]; then
        printf '%s' "$suggestion"
        return
    fi
    local input_cmd="${input%% *}"
    local sugg_cmd="${suggestion%% *}"
    if [[ -n "$input_cmd" && "$input_cmd" == "$sugg_cmd" ]]; then
        printf '%s' "$suggestion"
        return
    fi
    local last_word="${input##* }"
    if [[ -n "$last_word" && "$suggestion" == "$last_word"* ]]; then
        printf '%s' "${input%$last_word}${suggestion}"
        return
    fi
    if [[ "$input" == *" " ]]; then
        printf '%s' "${input}${suggestion}"
    else
        printf '%s' "${input} ${suggestion}"
    fi
}

_awen_clear_ghost() {
    if [[ -n "$_AWEN_SUGGESTION" ]] || (( _AWEN_MENU_ACTIVE )); then
        _AWEN_SUGGESTION=""
        _awen_remove_ghost_highlight
        _awen_menu_reset
        POSTDISPLAY=""
        zle -R
    fi
}

_awen_clear_hint() {
    _AWEN_HINT=""
    _AWEN_WARNING=""
    zle -M "" 2>/dev/null
}

_awen_render_ghost() {
    local suggestion="$1" source="${2:-}"

    _awen_remove_ghost_highlight

    if [[ -z "$suggestion" ]]; then
        _AWEN_SUGGESTION=""
        POSTDISPLAY=""
        return
    fi

    local full_suggestion
    full_suggestion=$(_awen_reconstruct_full_cmd "$BUFFER" "$suggestion" "$source")
    _AWEN_SUGGESTION="$full_suggestion"

    if [[ "$full_suggestion" == "$BUFFER"* ]]; then
        local ghost_part="${full_suggestion#$BUFFER}"
        if [[ -n "$ghost_part" ]]; then
            POSTDISPLAY="$ghost_part"
            _AWEN_GHOST_HIGHLIGHT="$#BUFFER $(( $#BUFFER + $#ghost_part )) $_AWEN_GHOST_STYLE"
            region_highlight+=("$_AWEN_GHOST_HIGHLIGHT")
        else
            POSTDISPLAY=""
        fi
    else
        POSTDISPLAY=""
    fi
}

_awen_render_nl_suggestion() {
    local command="$1"
    _awen_remove_ghost_highlight
    if [[ -z "$command" ]]; then
        POSTDISPLAY=""
        _AWEN_SUGGESTION=""
        return
    fi
    _AWEN_SUGGESTION="$command"
    _AWEN_NL_MODE=1
    local offset=$#BUFFER
    local line=$'\n'"  → ${command}"
    POSTDISPLAY="$line"
    region_highlight+=("$(( offset + 1 )) $(( offset + 4 )) fg=240")
    region_highlight+=("$(( offset + 4 )) $(( offset + ${#line} )) fg=82,bold")
    zle -R
}

_awen_render_menu() {
    _awen_remove_ghost_highlight

    local input="$BUFFER"
    local selected_full="${_AWEN_MENU_FULL_CMDS[$_AWEN_MENU_INDEX]}"
    local ghost_part=""
    [[ "$selected_full" == "$input"* ]] && ghost_part="${selected_full#$input}"

    local max_visible=$_AWEN_MENU_MAX
    (( max_visible > LINES - 3 )) && max_visible=$(( LINES - 3 ))
    (( max_visible < 1 )) && max_visible=1

    local scroll_start=1
    if (( _AWEN_MENU_COUNT > max_visible )); then
        if (( _AWEN_MENU_INDEX > max_visible )); then
            scroll_start=$(( _AWEN_MENU_INDEX - max_visible + 1 ))
        fi
    fi
    local scroll_end=$(( scroll_start + max_visible - 1 ))
    (( scroll_end > _AWEN_MENU_COUNT )) && scroll_end=$_AWEN_MENU_COUNT

    local pd=""
    local offset=$#BUFFER

    if [[ -n "$ghost_part" ]]; then
        pd="$ghost_part"
        region_highlight+=("$offset $(( offset + ${#ghost_part} )) $_AWEN_GHOST_STYLE")
        (( offset += ${#ghost_part} ))
    fi

    local content_width=$(( COLUMNS - 8 ))
    (( content_width > 86 )) && content_width=86
    (( content_width < 36 )) && content_width=36

    local current_source=""
    local i item_text item_source item_desc tag tag_style title title_icon title_text title_content title_line
    local cmd_col desc_col tag_col content entry entry_len line_start
    local tag_width=8
    local cmd_width=$(( content_width * 48 / 100 ))
    local desc_width=$(( content_width - cmd_width - tag_width - 4 ))
    (( cmd_width < 18 )) && cmd_width=18
    (( desc_width < 8 )) && desc_width=8

    local rule="$(_awen_repeat "─" $(( content_width + 2 )))"
    local top_line="  ╭${rule}╮"
    pd+=$'\n'"${top_line}"
    region_highlight+=("$(( offset + 1 )) $(( offset + 1 + ${#top_line} )) $_AWEN_STYLE_PANEL")
    (( offset += 1 + ${#top_line} ))

    for (( i=scroll_start; i<=scroll_end; i++ )); do
        item_text="${_AWEN_MENU_TEXTS[$i]}"
        item_source="${_AWEN_MENU_SOURCES[$i]}"
        item_desc="${_AWEN_MENU_DESCS[$i]}"

        if [[ "$item_source" != "$current_source" ]]; then
            current_source="$item_source"
            title="$(_awen_source_title "$item_source")"
            title_icon="$(_awen_source_icon "$item_source")"
            title_text="${title_icon} ${title}"
            title_content="$(_awen_pad_right "$title_text" "$content_width")"
            title_line="  │ ${title_content} │"
            pd+=$'\n'"${title_line}"
            line_start=$(( offset + 1 ))
            region_highlight+=("${line_start} $(( line_start + ${#title_line} )) $_AWEN_STYLE_PANEL")
            region_highlight+=("$(( line_start + 4 )) $(( line_start + 4 + ${#title_icon} )) $(_awen_source_style "$item_source")")
            region_highlight+=("$(( line_start + 5 + ${#title_icon} )) $(( line_start + 4 + ${#title_text} + 1 )) $_AWEN_STYLE_MUTED")
            (( offset += 1 + ${#title_line} ))
        fi

        tag="$(_awen_source_label "$item_source")"
        tag_style="$(_awen_source_style "$item_source")"

        if (( ${#item_text} > cmd_width )); then
            cmd_col="${item_text[1,$(( cmd_width - 3 ))]}..."
        else
            cmd_col="$(_awen_pad_right "$item_text" "$cmd_width")"
        fi

        if [[ -n "$item_desc" ]]; then
            if (( ${#item_desc} > desc_width )); then
                desc_col="${item_desc[1,$(( desc_width - 3 ))]}..."
            else
                desc_col="$(_awen_pad_right "$item_desc" "$desc_width")"
            fi
        else
            desc_col="$(_awen_pad_right "" "$desc_width")"
        fi

        tag_col="$(_awen_pad_right "$tag" "$tag_width")"
        if (( i == _AWEN_MENU_INDEX )); then
            content="> ${cmd_col} ${desc_col} ${tag_col}"
        else
            content="  ${cmd_col} ${desc_col} ${tag_col}"
        fi
        content="$(_awen_pad_right "$content" "$content_width")"
        entry="  │ ${content} │"
        entry_len=${#entry}
        pd+=$'\n'"${entry}"

        local base=$(( offset + 1 ))
        region_highlight+=("${base} $(( base + entry_len )) $_AWEN_STYLE_PANEL")
        if (( i == _AWEN_MENU_INDEX )); then
            region_highlight+=("$(( base + 4 )) $(( base + 4 + content_width )) $_AWEN_STYLE_SELECTED")
            region_highlight+=("$(( base + 4 + content_width - tag_width )) $(( base + 4 + content_width )) ${tag_style},bold,bg=236")
        else
            region_highlight+=("$(( base + 4 )) $(( base + 6 + cmd_width )) $_AWEN_STYLE_TEXT")
            region_highlight+=("$(( base + 7 + cmd_width )) $(( base + 7 + cmd_width + desc_width )) $_AWEN_STYLE_DIM")
            region_highlight+=("$(( base + 4 + content_width - tag_width )) $(( base + 4 + content_width )) ${tag_style}")
        fi
        (( offset += 1 + entry_len ))
    done

    if (( _AWEN_AI_LOADING )); then
        local ai_icon="$(_awen_source_icon ai)"
        local ai_title="$(_awen_source_title ai)"
        local loading_title="${ai_icon} ${ai_title}"
        local loading_title_content="$(_awen_pad_right "$loading_title" "$content_width")"
        local loading_title_line="  │ ${loading_title_content} │"
        pd+=$'\n'"${loading_title_line}"
        local lt_base=$(( offset + 1 ))
        region_highlight+=("${lt_base} $(( lt_base + ${#loading_title_line} )) $_AWEN_STYLE_PANEL")
        region_highlight+=("$(( lt_base + 4 )) $(( lt_base + 4 + ${#ai_icon} )) $_AWEN_STYLE_AI")
        region_highlight+=("$(( lt_base + 5 + ${#ai_icon} )) $(( lt_base + 4 + ${#loading_title} + 1 )) $_AWEN_STYLE_MUTED")
        (( offset += 1 + ${#loading_title_line} ))

        local loading_text="  thinking..."
        local loading_content="$(_awen_pad_right "$loading_text" "$content_width")"
        local loading_line="  │ ${loading_content} │"
        pd+=$'\n'"${loading_line}"
        local ll_base=$(( offset + 1 ))
        region_highlight+=("${ll_base} $(( ll_base + ${#loading_line} )) $_AWEN_STYLE_PANEL")
        region_highlight+=("$(( ll_base + 4 )) $(( ll_base + 4 + ${#loading_text} )) $_AWEN_STYLE_DIM")
        (( offset += 1 + ${#loading_line} ))
    fi

    local mid_line="  ├${rule}┤"
    pd+=$'\n'"${mid_line}"
    region_highlight+=("$(( offset + 1 )) $(( offset + 1 + ${#mid_line} )) $_AWEN_STYLE_PANEL")
    (( offset += 1 + ${#mid_line} ))

    local foot_content="$(_awen_pad_right "$(_awen_keycap_line "$content_width")" "$content_width")"
    local foot_line="  │ ${foot_content} │"
    pd+=$'\n'"${foot_line}"
    region_highlight+=("$(( offset + 1 )) $(( offset + 1 + ${#foot_line} )) $_AWEN_STYLE_PANEL")
    region_highlight+=("$(( offset + 4 )) $(( offset + 4 + content_width + 1 )) $_AWEN_STYLE_DIM")
    region_highlight+=("$(( offset + 4 + content_width - 4 )) $(( offset + 4 + content_width + 1 )) $_AWEN_STYLE_MUTED")
    (( offset += 1 + ${#foot_line} ))

    local bottom_line="  ╰${rule}╯"
    pd+=$'\n'"${bottom_line}"
    region_highlight+=("$(( offset + 1 )) $(( offset + 1 + ${#bottom_line} )) $_AWEN_STYLE_PANEL")

    POSTDISPLAY="$pd"
    _AWEN_SUGGESTION="$selected_full"
}

_awen_render_hint() {
    if [[ -n "$_AWEN_WARNING" ]]; then
        local warning_text="  ⚠ ${_AWEN_WARNING}"
        zle -M "$warning_text"
    elif [[ -n "$_AWEN_HINT" ]]; then
        local hint_text="  ⓘ ${_AWEN_HINT}"
        zle -M "$hint_text"
    fi
}

_awen_render_risk_panel() {
    local warning_text="$1"
    _awen_remove_ghost_highlight
    _awen_menu_reset
    _AWEN_SUGGESTION=""

    local offset=$#BUFFER
    local pd=""
    local content_width=$(( COLUMNS - 8 ))
    (( content_width > 86 )) && content_width=86
    (( content_width < 42 )) && content_width=42
    local rule="$(_awen_repeat "─" $(( content_width + 2 )))"
    local top_line="  ╭${rule}╮"
    local title_content="$(_awen_pad_right "$(_awen_source_icon risk) risk warning (risk)" "$content_width")"
    local text_content="$(_awen_pad_right "$warning_text" "$content_width")"
    local foot_content="$(_awen_pad_right "$(_awen_footer_line "$content_width" "↵ confirm   ⇥ ignore   suggests only")" "$content_width")"
    local bottom_line="  ╰${rule}╯"
    local line

    for line in "$top_line"; do
        pd+=$'\n'"${line}"
        region_highlight+=("$(( offset + 1 )) $(( offset + 1 + ${#line} )) $_AWEN_STYLE_RISK")
        (( offset += 1 + ${#line} ))
    done

    line="  │ ${title_content} │"
    pd+=$'\n'"${line}"
    region_highlight+=("$(( offset + 1 )) $(( offset + 1 + ${#line} )) $_AWEN_STYLE_RISK")
    region_highlight+=("$(( offset + 4 )) $(( offset + 4 + ${#title_content} )) ${_AWEN_STYLE_RISK},bold")
    (( offset += 1 + ${#line} ))

    line="  │ ${text_content} │"
    pd+=$'\n'"${line}"
    region_highlight+=("$(( offset + 1 )) $(( offset + 1 + ${#line} )) $_AWEN_STYLE_RISK")
    region_highlight+=("$(( offset + 4 )) $(( offset + 4 + ${#text_content} )) $_AWEN_STYLE_TEXT")
    (( offset += 1 + ${#line} ))

    line="  ├${rule}┤"
    pd+=$'\n'"${line}"
    region_highlight+=("$(( offset + 1 )) $(( offset + 1 + ${#line} )) $_AWEN_STYLE_RISK")
    (( offset += 1 + ${#line} ))

    line="  │ ${foot_content} │"
    pd+=$'\n'"${line}"
    region_highlight+=("$(( offset + 1 )) $(( offset + 1 + ${#line} )) $_AWEN_STYLE_RISK")
    region_highlight+=("$(( offset + 4 )) $(( offset + 4 + content_width + 1 )) $_AWEN_STYLE_RISK")
    (( offset += 1 + ${#line} ))

    pd+=$'\n'"${bottom_line}"
    region_highlight+=("$(( offset + 1 )) $(( offset + 1 + ${#bottom_line} )) $_AWEN_STYLE_RISK")

    POSTDISPLAY="$pd"
}

_awen_render_failure_panel() {
    local failure_idx="$1"
    local hint_text="$_AWEN_HINT"
    local fix_cmd="${_AWEN_MENU_TEXTS[$failure_idx]}"
    local fix_desc="${_AWEN_MENU_DESCS[$failure_idx]}"
    local full_cmd="${_AWEN_MENU_FULL_CMDS[$failure_idx]}"

    _awen_remove_ghost_highlight

    local ghost_part=""
    [[ "$full_cmd" == "$BUFFER"* ]] && ghost_part="${full_cmd#$BUFFER}"
    local offset=$#BUFFER
    local pd=""

    if [[ -n "$ghost_part" ]]; then
        pd="$ghost_part"
        region_highlight+=("$offset $(( offset + ${#ghost_part} )) $_AWEN_GHOST_STYLE")
        (( offset += ${#ghost_part} ))
    fi

    local display_hint="${hint_text:-$fix_desc}"
    local content_width=$(( COLUMNS - 8 ))
    (( content_width > 86 )) && content_width=86
    (( content_width < 42 )) && content_width=42
    local rule="$(_awen_repeat "─" $(( content_width + 2 )))"
    local top_line="  ╭${rule}╮"
    local title_content="$(_awen_pad_right "$(_awen_source_icon failure) command failed" "$content_width")"
    local hint_content="$(_awen_pad_right "$display_hint" "$content_width")"
    local fix_content="$(_awen_pad_right "> ${fix_cmd}" "$content_width")"
    local foot_content="$(_awen_pad_right "$(_awen_footer_line "$content_width" "↵ apply fix   ⇥ ignore")" "$content_width")"
    local bottom_line="  ╰${rule}╯"
    local line

    line="$top_line"
    pd+=$'\n'"${line}"
    region_highlight+=("$(( offset + 1 )) $(( offset + 1 + ${#line} )) $_AWEN_STYLE_FIX")
    (( offset += 1 + ${#line} ))

    line="  │ ${title_content} │"
    pd+=$'\n'"${line}"
    region_highlight+=("$(( offset + 1 )) $(( offset + 1 + ${#line} )) $_AWEN_STYLE_FIX")
    region_highlight+=("$(( offset + 4 )) $(( offset + 4 + ${#title_content} )) ${_AWEN_STYLE_FIX},bold")
    (( offset += 1 + ${#line} ))

    line="  │ ${hint_content} │"
    pd+=$'\n'"${line}"
    region_highlight+=("$(( offset + 1 )) $(( offset + 1 + ${#line} )) $_AWEN_STYLE_FIX")
    region_highlight+=("$(( offset + 4 )) $(( offset + 4 + ${#hint_content} )) $_AWEN_STYLE_TEXT")
    (( offset += 1 + ${#line} ))

    line="  │ ${fix_content} │"
    pd+=$'\n'"${line}"
    region_highlight+=("$(( offset + 1 )) $(( offset + 1 + ${#line} )) $_AWEN_STYLE_FIX")
    region_highlight+=("$(( offset + 4 )) $(( offset + 4 + ${#fix_content} )) ${_AWEN_STYLE_FIX},bold")
    (( offset += 1 + ${#line} ))

    if (( _AWEN_AI_LOADING )); then
        local ai_loading_text="$(_awen_source_icon ai) thinking..."
        local ai_loading_content="$(_awen_pad_right "$ai_loading_text" "$content_width")"
        line="  │ ${ai_loading_content} │"
        pd+=$'\n'"${line}"
        region_highlight+=("$(( offset + 1 )) $(( offset + 1 + ${#line} )) $_AWEN_STYLE_FIX")
        region_highlight+=("$(( offset + 4 )) $(( offset + 4 + ${#ai_loading_text} )) $_AWEN_STYLE_AI")
        (( offset += 1 + ${#line} ))
    fi

    line="  ├${rule}┤"
    pd+=$'\n'"${line}"
    region_highlight+=("$(( offset + 1 )) $(( offset + 1 + ${#line} )) $_AWEN_STYLE_FIX")
    (( offset += 1 + ${#line} ))

    line="  │ ${foot_content} │"
    pd+=$'\n'"${line}"
    region_highlight+=("$(( offset + 1 )) $(( offset + 1 + ${#line} )) $_AWEN_STYLE_FIX")
    region_highlight+=("$(( offset + 4 )) $(( offset + 4 + content_width + 1 )) $_AWEN_STYLE_DIM")
    region_highlight+=("$(( offset + 4 + content_width - 4 )) $(( offset + 4 + content_width + 1 )) $_AWEN_STYLE_MUTED")
    (( offset += 1 + ${#line} ))

    pd+=$'\n'"${bottom_line}"
    region_highlight+=("$(( offset + 1 )) $(( offset + 1 + ${#bottom_line} )) $_AWEN_STYLE_FIX")

    POSTDISPLAY="$pd"
    _AWEN_SUGGESTION="$full_cmd"
    _AWEN_MENU_ACTIVE=1
    _AWEN_MENU_INDEX=$failure_idx
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
        last_stderr=$(head -c 500 "$_AWEN_LAST_STDERR_FILE" 2>/dev/null | tr '\n' ' ' | tr '"' "'" )
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

# Build JSON request for suggest (completion mode)
# Args: $1=input $2=cursor $3=skip_ai ("true"/"false")
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

# Build JSON request for NL generation
# Args: $1=query (without "# " prefix)
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

# Parse suggest response and apply to display
# Args: $1=response JSON
_awen_apply_response() {
    local response="$1"

    if [[ -z "$response" ]]; then
        _awen_menu_reset
        _awen_remove_ghost_highlight
        POSTDISPLAY=""
        _AWEN_SUGGESTION=""
        return
    fi

    local hint_text="" warning_text=""

    # Parse hint/warning
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

    # Parse need_ai signal from daemon
    if [[ "$_AWEN_HAS_JQ" == "1" ]]; then
        _AWEN_NEED_AI=$(printf '%s\n' "$response" | jq -r '.need_ai // "false"' 2>/dev/null)
    else
        if [[ "$response" == *'"need_ai":true'* ]]; then
            _AWEN_NEED_AI="true"
        else
            _AWEN_NEED_AI="false"
        fi
    fi

    # Parse all suggestions
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

    # Reconstruct full commands
    _AWEN_MENU_FULL_CMDS=()
    local input="$BUFFER"
    local i
    for (( i=1; i<=count; i++ )); do
        _AWEN_MENU_FULL_CMDS+=("$(_awen_reconstruct_full_cmd "$input" "${_AWEN_MENU_TEXTS[$i]}" "${_AWEN_MENU_SOURCES[$i]}")")
    done

    # Detect failure suggestion for dedicated panel
    local failure_idx=0
    if [[ -n "$_AWEN_HINT" ]]; then
        for (( i=1; i<=count; i++ )); do
            if [[ "${_AWEN_MENU_SOURCES[$i]}" == "failure" ]]; then
                failure_idx=$i
                break
            fi
        done
    fi

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
        local single_text="${_AWEN_MENU_TEXTS[1]}"
        local single_source="${_AWEN_MENU_SOURCES[1]}"
        _awen_menu_reset
        _awen_render_ghost "$single_text" "$single_source"
    else
        _awen_menu_reset
        _awen_remove_ghost_highlight
        POSTDISPLAY=""
        _AWEN_SUGGESTION=""
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

    _awen_apply_response "$response"

    [[ "$_AWEN_NEED_AI" != "false" ]] && _awen_schedule_ai
}

# Phase 1: Synchronous local-only suggest (<20ms)
_awen_suggest_local() {
    if [[ -z "$BUFFER" || ! -S "$_AWEN_SOCKET" ]]; then
        _awen_remove_ghost_highlight
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

    # Throttle: skip if called too soon after previous local request
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

    # Only schedule AI Phase 2 if daemon signals it would be useful
    [[ "$_AWEN_NEED_AI" != "false" ]] && _awen_schedule_ai
}

# Cancel any pending async AI request (pipe stays open permanently)
_awen_cancel_pending_ai() {
    if [[ -n "$_AWEN_AI_PID" ]]; then
        kill "$_AWEN_AI_PID" 2>/dev/null
        _AWEN_AI_PID=""
    fi
    _AWEN_AI_LOADING=0
}

# Phase 2: Schedule async AI request after AWEN_AI_DELAY
# Pipe is pre-allocated at init time (outside ZLE) to avoid exec in widget context.
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

# Accept full ghost text suggestion
_awen_accept() {
    if (( _AWEN_MENU_ACTIVE )); then
        _awen_cancel_pending_ai
        BUFFER="${_AWEN_MENU_FULL_CMDS[$_AWEN_MENU_INDEX]}"
        CURSOR=${#BUFFER}
        _awen_remove_ghost_highlight
        _awen_menu_reset
        _AWEN_SUGGESTION=""
        _AWEN_NL_MODE=0
        POSTDISPLAY=""
        _awen_clear_hint
        zle -R
    elif [[ -n "$_AWEN_SUGGESTION" ]]; then
        _awen_cancel_pending_ai
        _awen_remove_ghost_highlight
        BUFFER="$_AWEN_SUGGESTION"
        CURSOR=${#BUFFER}
        _AWEN_SUGGESTION=""
        _AWEN_NL_MODE=0
        POSTDISPLAY=""
        _awen_clear_hint
        zle -R
    else
        zle forward-char
    fi
}

# Accept next word from ghost text
_awen_accept_word() {
    if (( _AWEN_MENU_ACTIVE )); then
        _awen_cancel_pending_ai
        local selected="${_AWEN_MENU_FULL_CMDS[$_AWEN_MENU_INDEX]}"
        _awen_remove_ghost_highlight
        _awen_menu_reset
        local input="$BUFFER"
        local remaining
        if [[ "$selected" == "$input"* ]]; then
            remaining="${selected#$input}"
        else
            remaining="$selected"
        fi
        local next_word="${remaining%% *}"
        if [[ "$next_word" == "$remaining" ]]; then
            BUFFER="$selected"
            _AWEN_SUGGESTION=""
            POSTDISPLAY=""
        else
            local accepted="${selected%$remaining}"
            BUFFER="${accepted}${next_word} "
            _AWEN_SUGGESTION="$selected"
            _awen_render_ghost "$selected"
        fi
        CURSOR=${#BUFFER}
        zle -R
    elif [[ -n "$_AWEN_SUGGESTION" ]]; then
        local input="$BUFFER"
        local remaining
        if [[ "$_AWEN_SUGGESTION" == "$input"* ]]; then
            remaining="${_AWEN_SUGGESTION#$input}"
        else
            remaining="$_AWEN_SUGGESTION"
        fi
        local next_word="${remaining%% *}"
        if [[ "$next_word" == "$remaining" ]]; then
            _awen_cancel_pending_ai
            _awen_remove_ghost_highlight
            BUFFER="$_AWEN_SUGGESTION"
            _AWEN_SUGGESTION=""
            POSTDISPLAY=""
        else
            local accepted="${_AWEN_SUGGESTION%$remaining}"
            BUFFER="${accepted}${next_word} "
            _awen_render_ghost "$_AWEN_SUGGESTION"
        fi
        CURSOR=${#BUFFER}
        zle -R
    else
        zle forward-word
    fi
}

# Dismiss suggestion
_awen_dismiss() {
    _awen_cancel_pending_ai
    if (( _AWEN_MENU_ACTIVE )) || [[ -n "$_AWEN_SUGGESTION" || -n "$POSTDISPLAY" || -n "$_AWEN_HINT" || -n "$_AWEN_WARNING" ]]; then
        _awen_remove_ghost_highlight
        _awen_menu_reset
        _AWEN_SUGGESTION=""
        POSTDISPLAY=""
        _awen_clear_hint
        zle -R
    fi
}

_awen_tab() {
    if (( _AWEN_MENU_ACTIVE )); then
        _awen_cancel_pending_ai
        BUFFER="${_AWEN_MENU_FULL_CMDS[$_AWEN_MENU_INDEX]}"
        CURSOR=${#BUFFER}
        _awen_remove_ghost_highlight
        _awen_menu_reset
        _AWEN_SUGGESTION=""
        _AWEN_NL_MODE=0
        POSTDISPLAY=""
        _awen_clear_hint
        zle -R
    elif [[ -n "$_AWEN_SUGGESTION" ]]; then
        _awen_cancel_pending_ai
        _awen_remove_ghost_highlight
        BUFFER="$_AWEN_SUGGESTION"
        CURSOR=${#BUFFER}
        _AWEN_SUGGESTION=""
        _AWEN_NL_MODE=0
        POSTDISPLAY=""
        _awen_clear_hint
        zle -R
    elif [[ -n "$BUFFER" ]]; then
        _awen_suggest_local
    else
        zle expand-or-complete
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
    _AWEN_FAILURE_SHOWN=0
    : > "$_AWEN_LAST_STDERR_FILE"
    if [[ "${AWEN_CAPTURE_STDERR:-1}" == "1" ]]; then
        exec {_AWEN_STDERR_BACKUP}>&2
        exec 2> >(tee "$_AWEN_LAST_STDERR_FILE" >&${_AWEN_STDERR_BACKUP})
    fi
}

# Self-insert wrapper: trigger suggest after each keystroke
_awen_self_insert() {
    if (( _AWEN_MENU_ACTIVE )); then
        _awen_remove_ghost_highlight
        _awen_menu_reset
        POSTDISPLAY=""
    fi
    zle .self-insert
    _awen_suggest_local
}

_awen_backward_delete_char() {
    if (( _AWEN_MENU_ACTIVE )); then
        _awen_remove_ghost_highlight
        _awen_menu_reset
        POSTDISPLAY=""
    fi
    zle .backward-delete-char
    _awen_suggest_local
}

# Menu navigation widgets
_awen_menu_up() {
    if (( _AWEN_MENU_ACTIVE )); then
        if (( _AWEN_MENU_INDEX > 1 )); then
            (( _AWEN_MENU_INDEX-- ))
        else
            _AWEN_MENU_INDEX=$_AWEN_MENU_COUNT
        fi
        _AWEN_SUGGESTION="${_AWEN_MENU_FULL_CMDS[$_AWEN_MENU_INDEX]}"
        _awen_render_menu
        zle -R
    else
        _awen_remove_ghost_highlight
        _awen_menu_reset
        POSTDISPLAY=""
        _AWEN_SUGGESTION=""
        _awen_clear_hint
        _awen_cancel_pending_ai
        zle up-line-or-history
    fi
}

_awen_menu_down() {
    if (( _AWEN_MENU_ACTIVE )); then
        if (( _AWEN_MENU_INDEX < _AWEN_MENU_COUNT )); then
            (( _AWEN_MENU_INDEX++ ))
        else
            _AWEN_MENU_INDEX=1
        fi
        _AWEN_SUGGESTION="${_AWEN_MENU_FULL_CMDS[$_AWEN_MENU_INDEX]}"
        _awen_render_menu
        zle -R
    else
        _awen_remove_ghost_highlight
        _awen_menu_reset
        POSTDISPLAY=""
        _AWEN_SUGGESTION=""
        _awen_clear_hint
        _awen_cancel_pending_ai
        zle down-line-or-history
    fi
}

_awen_menu_accept() {
    if (( _AWEN_MENU_ACTIVE )); then
        _awen_cancel_pending_ai
        BUFFER="${_AWEN_MENU_FULL_CMDS[$_AWEN_MENU_INDEX]}"
        CURSOR=${#BUFFER}
        _awen_remove_ghost_highlight
        _awen_menu_reset
        _AWEN_SUGGESTION=""
        POSTDISPLAY=""
        _awen_clear_hint
        zle -R
    else
        _awen_cancel_pending_ai
        _awen_remove_ghost_highlight
        _awen_menu_reset
        _AWEN_SUGGESTION=""
        POSTDISPLAY=""
        _awen_clear_hint
        zle accept-line
    fi
}

# Initialize Awen
_awen_on_ai_signal() {
    _awen_check_ai_result
}

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

    # Detect jq for robust JSON parsing
    if command -v jq &>/dev/null; then
        typeset -g _AWEN_HAS_JQ=1
    else
        typeset -g _AWEN_HAS_JQ=0
    fi

    # Detect zsh/datetime for fast timestamp
    if zmodload zsh/datetime 2>/dev/null; then
        typeset -g _AWEN_HAS_ZDATE=1
    else
        typeset -g _AWEN_HAS_ZDATE=0
    fi

    typeset -g _AWEN_AI_RESULT_FILE="${TMPDIR:-/tmp}/.awen-ai-result-$$"
    : > "$_AWEN_AI_RESULT_FILE"

    # Cleanup on shell exit
    trap '
        [[ -n "$_AWEN_AI_PID" ]] && kill "$_AWEN_AI_PID" 2>/dev/null
        rm -f "$_AWEN_LAST_STDERR_FILE" "${TMPDIR:-/tmp}/.awen-ai-token-$$" "$_AWEN_AI_RESULT_FILE" 2>/dev/null
    ' EXIT

    _awen_ensure_daemon

    # Register ZLE widgets
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


    # Keybinding setup (disable with AWEN_ENABLE_KEYBIND_OVERRIDE=0)
    if [[ "${AWEN_ENABLE_KEYBIND_OVERRIDE:-1}" == "1" ]]; then
        bindkey -M main '\e[C' _awen_accept          # Right arrow
        bindkey -M main '\eOC' _awen_accept          # Right arrow (application mode)
        bindkey -M main '\e[1;5C' _awen_accept_word  # Ctrl+Right
        bindkey -M main '\e[27;5;67~' _awen_accept_word  # Ctrl+Right (alternate)
        bindkey -M main '\e\e[C' _awen_accept_word   # Alt+Right (fallback)
        bindkey -M main '\e[Z' _awen_dismiss          # Shift+Tab dismiss

        # Menu navigation (fallthrough to defaults when menu inactive)
        bindkey -M main '\e[A' _awen_menu_up           # Up arrow
        bindkey -M main '\eOA' _awen_menu_up           # Up arrow (application mode)
        bindkey -M main '\e[B' _awen_menu_down         # Down arrow
        bindkey -M main '\eOB' _awen_menu_down         # Down arrow (application mode)
        bindkey -M main '^M' _awen_menu_accept         # Enter

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

        bindkey -M main '\t' _awen_tab                    # Tab accept / fallback
    fi

    # Register hooks
    autoload -Uz add-zsh-hook
    add-zsh-hook precmd _awen_precmd
    add-zsh-hook preexec _awen_preexec

    _awen_line_init() {
        if (( ! _AWEN_FAILURE_SHOWN )) \
            && [[ -n "$_AWEN_LAST_EXIT_CODE" && "$_AWEN_LAST_EXIT_CODE" -ne 0 ]] \
            && [[ -s "$_AWEN_LAST_STDERR_FILE" ]] \
            && [[ -z "$BUFFER" ]]; then
            _AWEN_FAILURE_SHOWN=1
            _awen_suggest_next
        fi
    }
    zle -N zle-line-init _awen_line_init
}

# Auto-initialize
awen_init
