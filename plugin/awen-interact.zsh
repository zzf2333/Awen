#!/usr/bin/env zsh
# Awen — ZLE widget handlers (accept, dismiss, navigate, input)

_awen_accept() {
    if (( _AWEN_MENU_ACTIVE )); then
        _awen_cancel_pending_ai
        BUFFER="${_AWEN_MENU_FULL_CMDS[$_AWEN_MENU_INDEX]}"
        CURSOR=${#BUFFER}
        _awen_hl_clear
        _awen_menu_reset
        _AWEN_SUGGESTION=""
        _AWEN_NL_MODE=0
        POSTDISPLAY=""
        _awen_clear_hint
        zle -R
        _awen_suggest_local
    elif [[ -n "$_AWEN_SUGGESTION" ]]; then
        _awen_cancel_pending_ai
        _awen_hl_clear
        BUFFER="$_AWEN_SUGGESTION"
        CURSOR=${#BUFFER}
        _AWEN_SUGGESTION=""
        _AWEN_NL_MODE=0
        POSTDISPLAY=""
        _awen_clear_hint
        zle -R
        _awen_suggest_local
    else
        zle forward-char
    fi
}

_awen_accept_word() {
    if (( _AWEN_MENU_ACTIVE )); then
        _awen_cancel_pending_ai
        local selected="${_AWEN_MENU_FULL_CMDS[$_AWEN_MENU_INDEX]}"
        _awen_hl_clear
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
            _awen_hl_clear
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

_awen_dismiss() {
    _awen_cancel_pending_ai
    if (( _AWEN_MENU_ACTIVE )) || [[ -n "$_AWEN_SUGGESTION" || -n "$POSTDISPLAY" || -n "$_AWEN_HINT" || -n "$_AWEN_WARNING" ]]; then
        _awen_hl_clear
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
        _awen_hl_clear
        _awen_menu_reset
        _AWEN_SUGGESTION=""
        _AWEN_NL_MODE=0
        POSTDISPLAY=""
        _awen_clear_hint
        zle -R
    elif [[ -n "$_AWEN_SUGGESTION" ]]; then
        _awen_cancel_pending_ai
        _awen_hl_clear
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

_awen_self_insert() {
    if (( _AWEN_MENU_ACTIVE )); then
        _awen_hl_clear
        _awen_menu_reset
        POSTDISPLAY=""
    fi
    zle .self-insert
    _awen_suggest_local
}

_awen_bracketed_paste() {
    if (( _AWEN_MENU_ACTIVE )); then
        _awen_hl_clear
        _awen_menu_reset
        POSTDISPLAY=""
    fi
    _AWEN_SUGGESTION=""
    local _pre_cursor=$CURSOR
    zle .bracketed-paste
    if (( _pre_cursor > 0 && CURSOR > _pre_cursor )); then
        local _char_before="${BUFFER[$_pre_cursor]}"
        local _first_pasted="${BUFFER[$((  _pre_cursor + 1 ))]}"
        if [[ "$_char_before" == [[:alnum:]] && "$_first_pasted" == [/~.\'] ]]; then
            BUFFER="${BUFFER[1,$_pre_cursor]} ${BUFFER[$((  _pre_cursor + 1 )),$#BUFFER]}"
            (( CURSOR++ ))
        fi
    fi
    _awen_suggest_local
}

_awen_backward_delete_char() {
    if (( _AWEN_MENU_ACTIVE )); then
        _awen_menu_reset
    fi
    _awen_hl_clear
    POSTDISPLAY=""
    _AWEN_SUGGESTION=""

    zle .backward-delete-char

    if [[ -z "$BUFFER" ]]; then
        _awen_clear_hint
        _awen_cancel_pending_ai
        _awen_cancel_delete_debounce
        return
    fi

    _awen_schedule_delete_debounce
}

_awen_menu_up() {
    if (( _AWEN_MENU_ACTIVE )); then
        _AWEN_MENU_USER_SELECTED=1
        if (( _AWEN_MENU_INDEX > 1 )); then
            (( _AWEN_MENU_INDEX-- ))
        else
            _AWEN_MENU_INDEX=$_AWEN_MENU_COUNT
        fi
        _AWEN_SUGGESTION="${_AWEN_MENU_FULL_CMDS[$_AWEN_MENU_INDEX]}"
        _awen_render_menu
        zle -R
    else
        _awen_hl_clear
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
        _AWEN_MENU_USER_SELECTED=1
        if (( _AWEN_MENU_INDEX < _AWEN_MENU_COUNT )); then
            (( _AWEN_MENU_INDEX++ ))
        else
            _AWEN_MENU_INDEX=1
        fi
        _AWEN_SUGGESTION="${_AWEN_MENU_FULL_CMDS[$_AWEN_MENU_INDEX]}"
        _awen_render_menu
        zle -R
    else
        _awen_hl_clear
        _awen_menu_reset
        POSTDISPLAY=""
        _AWEN_SUGGESTION=""
        _awen_clear_hint
        _awen_cancel_pending_ai
        zle down-line-or-history
    fi
}

_awen_menu_accept() {
    _awen_cancel_pending_ai
    if (( _AWEN_MENU_ACTIVE && _AWEN_MENU_USER_SELECTED )); then
        BUFFER="${_AWEN_MENU_FULL_CMDS[$_AWEN_MENU_INDEX]}"
        CURSOR=${#BUFFER}
    fi
    _awen_hl_clear
    _awen_menu_reset
    _AWEN_SUGGESTION=""
    _AWEN_NL_MODE=0
    POSTDISPLAY=""
    _awen_clear_hint
    zle accept-line
}
