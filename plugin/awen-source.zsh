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
    local entry
    for entry in "${_AWEN_HL_ENTRIES[@]}"; do
        region_highlight=("${(@)region_highlight:#$entry}")
    done
    _AWEN_HL_ENTRIES=()
    _AWEN_GHOST_HIGHLIGHT=""
}

_awen_hl_add() {
    region_highlight+=("$1")
    _AWEN_HL_ENTRIES+=("$1")
}

_awen_hl_set_ghost() {
    _AWEN_GHOST_HIGHLIGHT="$1"
    region_highlight+=("$1")
    _AWEN_HL_ENTRIES+=("$1")
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
