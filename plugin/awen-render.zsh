#!/usr/bin/env zsh
# Awen — Render functions (ghost text, menu, panels)

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

_awen_render_ghost() {
    local suggestion="$1" source="${2:-}"

    _awen_hl_clear

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
            local prompt_width=${#${(%%)PROMPT}}
            local available=$(( COLUMNS - prompt_width - ${#BUFFER} - 1 ))
            if (( available > 2 && ${#ghost_part} > available )); then
                ghost_part="${ghost_part[1,$((available - 1))]}…"
            elif (( available <= 2 )); then
                ghost_part=""
            fi

            if [[ -n "$ghost_part" ]]; then
                POSTDISPLAY="$ghost_part"
                _awen_hl_set_ghost "$#BUFFER $(( $#BUFFER + $#ghost_part )) $_AWEN_GHOST_STYLE"
            else
                POSTDISPLAY=""
            fi
        else
            POSTDISPLAY=""
        fi
    else
        POSTDISPLAY=""
    fi
}

_awen_render_nl_suggestion() {
    local command="$1"
    _awen_hl_clear
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
    _awen_hl_add "$(( offset + 1 )) $(( offset + 4 )) fg=240"
    _awen_hl_add "$(( offset + 4 )) $(( offset + ${#line} )) fg=82,bold"
    zle -R
}

_awen_render_menu() {
    _awen_hl_clear

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
        _awen_hl_add "$offset $(( offset + ${#ghost_part} )) $_AWEN_GHOST_STYLE"
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
    _awen_hl_add "$(( offset + 1 )) $(( offset + 1 + ${#top_line} )) $_AWEN_STYLE_PANEL"
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
            _awen_hl_add "${line_start} $(( line_start + ${#title_line} )) $_AWEN_STYLE_PANEL"
            _awen_hl_add "$(( line_start + 4 )) $(( line_start + 4 + ${#title_icon} )) $(_awen_source_style "$item_source")"
            _awen_hl_add "$(( line_start + 5 + ${#title_icon} )) $(( line_start + 4 + ${#title_text} + 1 )) $_AWEN_STYLE_MUTED"
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
        _awen_hl_add "${base} $(( base + entry_len )) $_AWEN_STYLE_PANEL"
        if (( i == _AWEN_MENU_INDEX )); then
            _awen_hl_add "$(( base + 4 )) $(( base + 4 + content_width )) $_AWEN_STYLE_SELECTED"
            _awen_hl_add "$(( base + 4 + content_width - tag_width )) $(( base + 4 + content_width )) ${tag_style},bold,bg=236"
        else
            _awen_hl_add "$(( base + 4 )) $(( base + 6 + cmd_width )) $_AWEN_STYLE_TEXT"
            _awen_hl_add "$(( base + 7 + cmd_width )) $(( base + 7 + cmd_width + desc_width )) $_AWEN_STYLE_DIM"
            _awen_hl_add "$(( base + 4 + content_width - tag_width )) $(( base + 4 + content_width )) ${tag_style}"
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
        _awen_hl_add "${lt_base} $(( lt_base + ${#loading_title_line} )) $_AWEN_STYLE_PANEL"
        _awen_hl_add "$(( lt_base + 4 )) $(( lt_base + 4 + ${#ai_icon} )) $_AWEN_STYLE_AI"
        _awen_hl_add "$(( lt_base + 5 + ${#ai_icon} )) $(( lt_base + 4 + ${#loading_title} + 1 )) $_AWEN_STYLE_MUTED"
        (( offset += 1 + ${#loading_title_line} ))

        local loading_text="  thinking..."
        local loading_content="$(_awen_pad_right "$loading_text" "$content_width")"
        local loading_line="  │ ${loading_content} │"
        pd+=$'\n'"${loading_line}"
        local ll_base=$(( offset + 1 ))
        _awen_hl_add "${ll_base} $(( ll_base + ${#loading_line} )) $_AWEN_STYLE_PANEL"
        _awen_hl_add "$(( ll_base + 4 )) $(( ll_base + 4 + ${#loading_text} )) $_AWEN_STYLE_DIM"
        (( offset += 1 + ${#loading_line} ))
    fi

    local mid_line="  ├${rule}┤"
    pd+=$'\n'"${mid_line}"
    _awen_hl_add "$(( offset + 1 )) $(( offset + 1 + ${#mid_line} )) $_AWEN_STYLE_PANEL"
    (( offset += 1 + ${#mid_line} ))

    local foot_content="$(_awen_pad_right "$(_awen_keycap_line "$content_width")" "$content_width")"
    local foot_line="  │ ${foot_content} │"
    pd+=$'\n'"${foot_line}"
    _awen_hl_add "$(( offset + 1 )) $(( offset + 1 + ${#foot_line} )) $_AWEN_STYLE_PANEL"
    _awen_hl_add "$(( offset + 4 )) $(( offset + 4 + content_width + 1 )) $_AWEN_STYLE_DIM"
    _awen_hl_add "$(( offset + 4 + content_width - ${#_AWEN_LOGO} )) $(( offset + 4 + content_width - ${#_AWEN_LOGO} + 4 )) $_AWEN_STYLE_MUTED"
    _awen_hl_add "$(( offset + 4 + content_width - ${#_AWEN_LOGO} + 4 )) $(( offset + 4 + content_width + 1 )) $_AWEN_STYLE_DIM"
    (( offset += 1 + ${#foot_line} ))

    local bottom_line="  ╰${rule}╯"
    pd+=$'\n'"${bottom_line}"
    _awen_hl_add "$(( offset + 1 )) $(( offset + 1 + ${#bottom_line} )) $_AWEN_STYLE_PANEL"

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
    _awen_hl_clear
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
        _awen_hl_add "$(( offset + 1 )) $(( offset + 1 + ${#line} )) $_AWEN_STYLE_RISK"
        (( offset += 1 + ${#line} ))
    done

    line="  │ ${title_content} │"
    pd+=$'\n'"${line}"
    _awen_hl_add "$(( offset + 1 )) $(( offset + 1 + ${#line} )) $_AWEN_STYLE_RISK"
    _awen_hl_add "$(( offset + 4 )) $(( offset + 4 + ${#title_content} )) ${_AWEN_STYLE_RISK},bold"
    (( offset += 1 + ${#line} ))

    line="  │ ${text_content} │"
    pd+=$'\n'"${line}"
    _awen_hl_add "$(( offset + 1 )) $(( offset + 1 + ${#line} )) $_AWEN_STYLE_RISK"
    _awen_hl_add "$(( offset + 4 )) $(( offset + 4 + ${#text_content} )) $_AWEN_STYLE_TEXT"
    (( offset += 1 + ${#line} ))

    line="  ├${rule}┤"
    pd+=$'\n'"${line}"
    _awen_hl_add "$(( offset + 1 )) $(( offset + 1 + ${#line} )) $_AWEN_STYLE_RISK"
    (( offset += 1 + ${#line} ))

    line="  │ ${foot_content} │"
    pd+=$'\n'"${line}"
    _awen_hl_add "$(( offset + 1 )) $(( offset + 1 + ${#line} )) $_AWEN_STYLE_RISK"
    _awen_hl_add "$(( offset + 4 )) $(( offset + 4 + content_width + 1 )) $_AWEN_STYLE_RISK"
    (( offset += 1 + ${#line} ))

    pd+=$'\n'"${bottom_line}"
    _awen_hl_add "$(( offset + 1 )) $(( offset + 1 + ${#bottom_line} )) $_AWEN_STYLE_RISK"

    POSTDISPLAY="$pd"
}

_awen_render_ai_loading_panel() {
    _awen_hl_clear
    _awen_menu_reset

    local offset=$#BUFFER
    local pd=""
    local content_width=$(( COLUMNS - 8 ))
    (( content_width > 86 )) && content_width=86
    (( content_width < 36 )) && content_width=36

    local rule="$(_awen_repeat "─" $(( content_width + 2 )))"
    local ai_icon="$(_awen_source_icon ai)"
    local title_text="${ai_icon} $(_awen_source_title ai)"
    local loading_text="  thinking..."
    local hint_text="$_AWEN_HINT"

    local title_content="$(_awen_pad_right "$title_text" "$content_width")"
    local loading_content="$(_awen_pad_right "$loading_text" "$content_width")"
    local foot_content="$(_awen_pad_right "$(_awen_footer_line "$content_width" "esc dismiss")" "$content_width")"
    local line

    line="  ╭${rule}╮"
    pd+=$'\n'"${line}"
    _awen_hl_add "$(( offset + 1 )) $(( offset + 1 + ${#line} )) $_AWEN_STYLE_PANEL"
    (( offset += 1 + ${#line} ))

    line="  │ ${title_content} │"
    pd+=$'\n'"${line}"
    local base=$(( offset + 1 ))
    _awen_hl_add "${base} $(( base + ${#line} )) $_AWEN_STYLE_PANEL"
    _awen_hl_add "$(( base + 4 )) $(( base + 4 + ${#ai_icon} )) $_AWEN_STYLE_AI"
    _awen_hl_add "$(( base + 5 + ${#ai_icon} )) $(( base + 4 + ${#title_text} + 1 )) $_AWEN_STYLE_MUTED"
    (( offset += 1 + ${#line} ))

    if [[ -n "$hint_text" ]]; then
        local hint_content="$(_awen_pad_right "$hint_text" "$content_width")"
        line="  │ ${hint_content} │"
        pd+=$'\n'"${line}"
        base=$(( offset + 1 ))
        _awen_hl_add "${base} $(( base + ${#line} )) $_AWEN_STYLE_PANEL"
        _awen_hl_add "$(( base + 4 )) $(( base + 4 + ${#hint_content} )) $_AWEN_STYLE_TEXT"
        (( offset += 1 + ${#line} ))
    fi

    line="  │ ${loading_content} │"
    pd+=$'\n'"${line}"
    base=$(( offset + 1 ))
    _awen_hl_add "${base} $(( base + ${#line} )) $_AWEN_STYLE_PANEL"
    _awen_hl_add "$(( base + 4 )) $(( base + 4 + ${#loading_text} )) $_AWEN_STYLE_DIM"
    (( offset += 1 + ${#line} ))

    line="  ├${rule}┤"
    pd+=$'\n'"${line}"
    _awen_hl_add "$(( offset + 1 )) $(( offset + 1 + ${#line} )) $_AWEN_STYLE_PANEL"
    (( offset += 1 + ${#line} ))

    line="  │ ${foot_content} │"
    pd+=$'\n'"${line}"
    _awen_hl_add "$(( offset + 1 )) $(( offset + 1 + ${#line} )) $_AWEN_STYLE_PANEL"
    _awen_hl_add "$(( offset + 4 )) $(( offset + 4 + content_width + 1 )) $_AWEN_STYLE_DIM"
    _awen_hl_add "$(( offset + 4 + content_width - ${#_AWEN_LOGO} )) $(( offset + 4 + content_width - ${#_AWEN_LOGO} + 4 )) $_AWEN_STYLE_MUTED"
    _awen_hl_add "$(( offset + 4 + content_width - ${#_AWEN_LOGO} + 4 )) $(( offset + 4 + content_width + 1 )) $_AWEN_STYLE_DIM"
    (( offset += 1 + ${#line} ))

    line="  ╰${rule}╯"
    pd+=$'\n'"${line}"
    _awen_hl_add "$(( offset + 1 )) $(( offset + 1 + ${#line} )) $_AWEN_STYLE_PANEL"

    POSTDISPLAY="$pd"
    _AWEN_SUGGESTION=""
}

_awen_render_failure_panel() {
    local failure_idx="$1"
    local hint_text="$_AWEN_HINT"
    local fix_cmd="${_AWEN_MENU_TEXTS[$failure_idx]}"
    local fix_desc="${_AWEN_MENU_DESCS[$failure_idx]}"
    local full_cmd="${_AWEN_MENU_FULL_CMDS[$failure_idx]}"

    _awen_hl_clear

    local ghost_part=""
    [[ "$full_cmd" == "$BUFFER"* ]] && ghost_part="${full_cmd#$BUFFER}"
    local offset=$#BUFFER
    local pd=""

    if [[ -n "$ghost_part" ]]; then
        pd="$ghost_part"
        _awen_hl_add "$offset $(( offset + ${#ghost_part} )) $_AWEN_GHOST_STYLE"
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
    _awen_hl_add "$(( offset + 1 )) $(( offset + 1 + ${#line} )) $_AWEN_STYLE_FIX"
    (( offset += 1 + ${#line} ))

    line="  │ ${title_content} │"
    pd+=$'\n'"${line}"
    _awen_hl_add "$(( offset + 1 )) $(( offset + 1 + ${#line} )) $_AWEN_STYLE_FIX"
    _awen_hl_add "$(( offset + 4 )) $(( offset + 4 + ${#title_content} )) ${_AWEN_STYLE_FIX},bold"
    (( offset += 1 + ${#line} ))

    line="  │ ${hint_content} │"
    pd+=$'\n'"${line}"
    _awen_hl_add "$(( offset + 1 )) $(( offset + 1 + ${#line} )) $_AWEN_STYLE_FIX"
    _awen_hl_add "$(( offset + 4 )) $(( offset + 4 + ${#hint_content} )) $_AWEN_STYLE_TEXT"
    (( offset += 1 + ${#line} ))

    line="  │ ${fix_content} │"
    pd+=$'\n'"${line}"
    _awen_hl_add "$(( offset + 1 )) $(( offset + 1 + ${#line} )) $_AWEN_STYLE_FIX"
    _awen_hl_add "$(( offset + 4 )) $(( offset + 4 + ${#fix_content} )) ${_AWEN_STYLE_FIX},bold"
    (( offset += 1 + ${#line} ))

    if (( _AWEN_AI_LOADING )); then
        local ai_loading_text="$(_awen_source_icon ai) thinking..."
        local ai_loading_content="$(_awen_pad_right "$ai_loading_text" "$content_width")"
        line="  │ ${ai_loading_content} │"
        pd+=$'\n'"${line}"
        _awen_hl_add "$(( offset + 1 )) $(( offset + 1 + ${#line} )) $_AWEN_STYLE_FIX"
        _awen_hl_add "$(( offset + 4 )) $(( offset + 4 + ${#ai_loading_text} )) $_AWEN_STYLE_AI"
        (( offset += 1 + ${#line} ))
    fi

    line="  ├${rule}┤"
    pd+=$'\n'"${line}"
    _awen_hl_add "$(( offset + 1 )) $(( offset + 1 + ${#line} )) $_AWEN_STYLE_FIX"
    (( offset += 1 + ${#line} ))

    line="  │ ${foot_content} │"
    pd+=$'\n'"${line}"
    _awen_hl_add "$(( offset + 1 )) $(( offset + 1 + ${#line} )) $_AWEN_STYLE_FIX"
    _awen_hl_add "$(( offset + 4 )) $(( offset + 4 + content_width + 1 )) $_AWEN_STYLE_DIM"
    _awen_hl_add "$(( offset + 4 + content_width - ${#_AWEN_LOGO} )) $(( offset + 4 + content_width - ${#_AWEN_LOGO} + 4 )) $_AWEN_STYLE_MUTED"
    _awen_hl_add "$(( offset + 4 + content_width - ${#_AWEN_LOGO} + 4 )) $(( offset + 4 + content_width + 1 )) $_AWEN_STYLE_DIM"
    (( offset += 1 + ${#line} ))

    pd+=$'\n'"${bottom_line}"
    _awen_hl_add "$(( offset + 1 )) $(( offset + 1 + ${#bottom_line} )) $_AWEN_STYLE_FIX"

    POSTDISPLAY="$pd"
    _AWEN_SUGGESTION="$full_cmd"
    _AWEN_MENU_ACTIVE=1
    _AWEN_MENU_INDEX=$failure_idx
}
