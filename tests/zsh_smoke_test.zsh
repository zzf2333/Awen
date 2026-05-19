#!/usr/bin/env zsh
# Awen zsh plugin smoke tests — runs without a terminal or daemon

set -uo pipefail

PASS=0
FAIL=0
PLUGIN_DIR="${0:A:h}/../plugin"
PLUGIN_FILE="$PLUGIN_DIR/awen.zsh"

assert_eq() {
    local label="$1" expected="$2" actual="$3"
    if [[ "$expected" == "$actual" ]]; then
        PASS=$((PASS + 1))
    else
        FAIL=$((FAIL + 1))
        print -u2 "FAIL: $label"
        print -u2 "  expected: $(printf '%q' "$expected")"
        print -u2 "  actual:   $(printf '%q' "$actual")"
    fi
}

assert_contains() {
    local label="$1" haystack="$2" needle="$3"
    if [[ "$haystack" == *"$needle"* ]]; then
        PASS=$((PASS + 1))
    else
        FAIL=$((FAIL + 1))
        print -u2 "FAIL: $label"
        print -u2 "  expected to contain: $needle"
        print -u2 "  got: $haystack"
    fi
}

# ============================================================
# Load only function definitions (skip auto-init on last line)
# ============================================================

load_plugin_functions() {
    zle()          { : }
    bindkey()      { : }
    autoload()     { : }
    add-zsh-hook() { : }
    eval "$(sed '/^awen_init$/d' "$PLUGIN_FILE")"
}

load_plugin_functions

# ============================================================
# Test: _awen_json_escape
# ============================================================

assert_eq "json_escape backslash" \
    'hello\\world' \
    "$(_awen_json_escape 'hello\world')"

assert_eq "json_escape double quote" \
    'say \"hello\"' \
    "$(_awen_json_escape 'say "hello"')"

assert_eq "json_escape newline" \
    'line1\nline2' \
    "$(_awen_json_escape $'line1\nline2')"

assert_eq "json_escape tab" \
    'col1\tcol2' \
    "$(_awen_json_escape $'col1\tcol2')"

assert_eq "json_escape carriage return" \
    'cr\r' \
    "$(_awen_json_escape $'cr\r')"

assert_eq "json_escape combined" \
    'a\\b\"c\nd' \
    "$(_awen_json_escape $'a\\b"c\nd')"

assert_eq "json_escape empty string" \
    '' \
    "$(_awen_json_escape '')"

# ============================================================
# Test: _awen_extract_json_value (fallback parser)
# ============================================================

assert_eq "extract simple value" \
    'hello world' \
    "$(_awen_extract_json_value 'hello world"}')"

assert_eq "extract with escaped quotes" \
    'say \"hi\"' \
    "$(_awen_extract_json_value 'say \"hi\""}')"

assert_eq "extract empty value" \
    '' \
    "$(_awen_extract_json_value '"rest')"

assert_eq "extract with special chars" \
    'git checkout -b feat/new' \
    "$(_awen_extract_json_value 'git checkout -b feat/new"}')"

# ============================================================
# Test: jq fallback JSON construction
# ============================================================

_AWEN_HAS_JQ=0
_test_esc_input=$(_awen_json_escape 'git "commit')
_test_esc_cwd=$(_awen_json_escape "/tmp/test dir")
_test_json=$(printf '{"type":"suggest","input":"%s","cursor_pos":%d,"context":{"cwd":"%s","last_command":%s,"last_exit_code":%s,"last_stderr":%s,"git_branch":%s,"git_status":null,"session_commands":[],"env_hints":[]}}' \
    "$_test_esc_input" 11 "$_test_esc_cwd" "null" "0" "null" "null")

assert_contains "fallback json has type" "$_test_json" '"type":"suggest"'
assert_contains "fallback json has escaped input" "$_test_json" 'git \"commit'
assert_contains "fallback json has cwd" "$_test_json" '/tmp/test dir'
assert_contains "fallback json has null last_command" "$_test_json" '"last_command":null'

if command -v jq &>/dev/null; then
    _parsed=$(echo "$_test_json" | jq -r '.input' 2>/dev/null)
    assert_eq "fallback json valid for jq" 'git "commit' "$_parsed"
fi

# ============================================================
# Test: _awen_reconstruct_full_cmd
# ============================================================

assert_eq "reconstruct: full prefix match" \
    "git checkout main" \
    "$(_awen_reconstruct_full_cmd "git ch" "git checkout main")"

assert_eq "reconstruct: word completion" \
    "git checkout" \
    "$(_awen_reconstruct_full_cmd "git ch" "checkout")"

assert_eq "reconstruct: input ends with space" \
    "kubectl get pods" \
    "$(_awen_reconstruct_full_cmd "kubectl get " "pods")"

assert_eq "reconstruct: append with space" \
    "kubectl get pods" \
    "$(_awen_reconstruct_full_cmd "kubectl get" "pods")"

assert_eq "reconstruct: exact match" \
    "ls" \
    "$(_awen_reconstruct_full_cmd "ls" "ls")"

# ============================================================
# Test: _awen_menu_reset
# ============================================================

_AWEN_MENU_ACTIVE=1
_AWEN_MENU_INDEX=3
_AWEN_MENU_COUNT=5
_AWEN_MENU_TEXTS=("a" "b" "c")
_AWEN_MENU_SOURCES=("history" "specs" "ai")
_AWEN_MENU_FULL_CMDS=("git a" "git b" "git c")
_awen_menu_reset

assert_eq "menu_reset: active" "0" "$_AWEN_MENU_ACTIVE"
assert_eq "menu_reset: index" "1" "$_AWEN_MENU_INDEX"
assert_eq "menu_reset: count" "0" "$_AWEN_MENU_COUNT"
assert_eq "menu_reset: texts empty" "0" "${#_AWEN_MENU_TEXTS[@]}"
assert_eq "menu_reset: sources empty" "0" "${#_AWEN_MENU_SOURCES[@]}"
assert_eq "menu_reset: full_cmds empty" "0" "${#_AWEN_MENU_FULL_CMDS[@]}"

# ============================================================
# Test: fallback multi-suggestion parsing (no jq)
# ============================================================

_AWEN_HAS_JQ=0
_AWEN_MENU_TEXTS=()
_AWEN_MENU_SOURCES=()

_fb_response='{"type":"suggest","suggestions":[{"text":"cd /tmp","source":"history","confidence":1.0},{"text":"cd ~","source":"specs","confidence":0.8},{"text":"clone","source":"ai","confidence":0.5}]}'
_remaining="${_fb_response#*\"suggestions\":\[}"
while [[ "$_remaining" == *'"text":"'* ]]; do
    _after_text="${_remaining#*\"text\":\"}"
    _s_text=$(_awen_extract_json_value "$_after_text")
    _s_source="history"
    if [[ "$_remaining" == *'"source":"'* ]]; then
        _after_src="${_remaining#*\"source\":\"}"
        _s_source=$(_awen_extract_json_value "$_after_src")
    fi
    if [[ -n "$_s_text" ]]; then
        _AWEN_MENU_TEXTS+=("$_s_text")
        _AWEN_MENU_SOURCES+=("${_s_source:-history}")
    fi
    _remaining="${_after_text#*\}}"
done

assert_eq "fallback multi: count" "3" "${#_AWEN_MENU_TEXTS[@]}"
assert_eq "fallback multi: text 1" "cd /tmp" "${_AWEN_MENU_TEXTS[1]}"
assert_eq "fallback multi: text 2" "cd ~" "${_AWEN_MENU_TEXTS[2]}"
assert_eq "fallback multi: text 3" "clone" "${_AWEN_MENU_TEXTS[3]}"
assert_eq "fallback multi: source 1" "history" "${_AWEN_MENU_SOURCES[1]}"
assert_eq "fallback multi: source 2" "specs" "${_AWEN_MENU_SOURCES[2]}"
assert_eq "fallback multi: source 3" "ai" "${_AWEN_MENU_SOURCES[3]}"

# ============================================================
# Test: AWEN_CAPTURE_STDERR defaults off
# ============================================================

assert_eq "AWEN_CAPTURE_STDERR default" \
    "0" \
    "${AWEN_CAPTURE_STDERR:-0}"

# ============================================================
# Test: AWEN_ENABLE_KEYBIND_OVERRIDE=0 skips bindkey
# ============================================================

# Re-load with keybind override disabled
_KEYBIND_CALLS_DISABLED=()
AWEN_ENABLE_KEYBIND_OVERRIDE=0

zle()          { : }
bindkey()      { _KEYBIND_CALLS_DISABLED+=("$*") }
autoload()     { : }
add-zsh-hook() { : }

eval "$(sed '/^awen_init$/d' "$PLUGIN_FILE")"
_awen_find_binary() { _AWEN_BIN="/bin/true" }
_awen_ensure_daemon() { : }
awen_init 2>/dev/null

assert_eq "keybind override disabled — no bindkey calls" \
    "0" \
    "${#_KEYBIND_CALLS_DISABLED[@]}"

# Re-load with keybind override enabled
_KEYBIND_CALLS_ENABLED=()
AWEN_ENABLE_KEYBIND_OVERRIDE=1

zle()          { : }
bindkey()      { _KEYBIND_CALLS_ENABLED+=("$*") }
autoload()     { : }
add-zsh-hook() { : }

eval "$(sed '/^awen_init$/d' "$PLUGIN_FILE")"
_awen_find_binary() { _AWEN_BIN="/bin/true" }
_awen_ensure_daemon() { : }
awen_init 2>/dev/null

if [[ ${#_KEYBIND_CALLS_ENABLED[@]} -gt 0 ]]; then
    PASS=$((PASS + 1))
else
    FAIL=$((FAIL + 1))
    print -u2 "FAIL: keybind override enabled — expected bindkey calls"
fi

# ============================================================
# Summary
# ============================================================

echo ""
echo "=== Awen zsh smoke tests ==="
echo "Passed: $PASS"
echo "Failed: $FAIL"
echo ""

if [[ $FAIL -gt 0 ]]; then
    exit 1
fi
exit 0
