#!/usr/bin/env zsh
# Awen — Source labels, styles, icons + highlight manager

_awen_source_label() {
    case "$1" in
        history)    printf '%s' "history" ;;
        specs)      printf '%s' "spec" ;;
        ai)         printf '%s' "ai" ;;
        failure)    printf '%s' "fix" ;;
        filesystem) printf '%s' "file" ;;
        *)          printf '%s' "$1" ;;
    esac
}

_awen_source_style() {
    case "$1" in
        history)    printf '%s' "$_AWEN_STYLE_HISTORY" ;;
        specs)      printf '%s' "$_AWEN_STYLE_SPEC" ;;
        ai)         printf '%s' "$_AWEN_STYLE_AI" ;;
        failure)    printf '%s' "$_AWEN_STYLE_FIX" ;;
        risk)       printf '%s' "$_AWEN_STYLE_RISK" ;;
        filesystem) printf '%s' "$_AWEN_STYLE_FILE" ;;
        *)          printf '%s' "$_AWEN_STYLE_DIM" ;;
    esac
}

_awen_source_title() {
    case "$1" in
        history)    printf '%s' "history" ;;
        specs)      printf '%s' "options" ;;
        ai)         printf '%s' "ai suggestions" ;;
        failure)    printf '%s' "fix" ;;
        filesystem) printf '%s' "files" ;;
        *)          printf '%s' "suggestions" ;;
    esac
}

_awen_source_icon() {
    case "$1" in
        history)    printf '%s' "↺" ;;
        specs)      printf '%s' "◇" ;;
        ai)         printf '%s' "✦" ;;
        failure)    printf '%s' "✓" ;;
        risk)       printf '%s' "!" ;;
        filesystem) printf '%s' "📁" ;;
        *)          printf '%s' "•" ;;
    esac
}

# --- Highlight Manager ---

_awen_hl_clear() {
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
    region_highlight=("${(@)region_highlight:#*fg=73*}")
    region_highlight=("${(@)region_highlight:#*fg=69*}")
    region_highlight=("${(@)region_highlight:#*fg=177*}")
    region_highlight=("${(@)region_highlight:#*fg=220*}")
    region_highlight=("${(@)region_highlight:#*fg=114*}")
    region_highlight=("${(@)region_highlight:#*fg=108*}")
    region_highlight=("${(@)region_highlight:#*fg=214*}")
    region_highlight=("${(@)region_highlight:#*fg=82*}")
    region_highlight=("${(@)region_highlight:#*fg=82,bold*}")
    region_highlight=("${(@)region_highlight:#*fg=238*}")
    _AWEN_GHOST_HIGHLIGHT=""
}

_awen_hl_add() {
    region_highlight+=("$1")
}

_awen_hl_set_ghost() {
    _AWEN_GHOST_HIGHLIGHT="$1"
    region_highlight+=("$1")
}

# --- State reset helpers ---

_awen_menu_reset() {
    _AWEN_MENU_ACTIVE=0
    _AWEN_MENU_INDEX=1
    _AWEN_MENU_COUNT=0
    _AWEN_MENU_TEXTS=()
    _AWEN_MENU_SOURCES=()
    _AWEN_MENU_DESCS=()
    _AWEN_MENU_FULL_CMDS=()
}

_awen_clear_ghost() {
    if [[ -n "$_AWEN_SUGGESTION" ]] || (( _AWEN_MENU_ACTIVE )); then
        _AWEN_SUGGESTION=""
        _awen_hl_clear
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
