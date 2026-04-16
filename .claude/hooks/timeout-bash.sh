#!/usr/bin/env bash
# PreToolUse hook for Bash, active only while the native-loop state file exists.
#
# - Refuses background execution for cargo/just/test commands (a hang would
#   otherwise become a zombie).
# - Forces a 5-minute timeout onto cargo/just/test commands that don't have
#   a smaller one.

STATE_FILE=".claude/native-loop.local.md"

if [ ! -f "$STATE_FILE" ]; then
    exit 0
fi

INPUT=$(cat)

eval "$(printf '%s' "$INPUT" | python3 - <<'PY'
import sys, json, shlex
d = json.load(sys.stdin)
cmd = d.get("tool_input", {}).get("command", "") or ""
bg = d.get("tool_input", {}).get("run_in_background", False)
timeout_ms = d.get("tool_input", {}).get("timeout", 0) or 0
# Escape command for shell eval.
print(f'COMMAND={shlex.quote(cmd)}')
print(f'RUN_IN_BACKGROUND={"true" if bg else "false"}')
print(f'CURRENT_TIMEOUT={int(timeout_ms)}')
PY
)"

# Does the command run any tests / cargo / just?
matches_testing() {
    echo "$1" | grep -qE '\b(cargo|just|pytest|uv)\b|\btest\b'
}

if matches_testing "$COMMAND"; then
    if [ "$RUN_IN_BACKGROUND" = "true" ]; then
        python3 - <<'PY'
import json
print(json.dumps({
    "decision": "block",
    "reason": (
        "Background execution is disabled while the native-loop Stop hook is armed. "
        "Run this command in the foreground so the hook can observe its result — a "
        "hung background command would otherwise become a zombie that never reports "
        "back. If the command genuinely needs to run in parallel, split it into "
        "pieces that each complete quickly."
    )
}))
PY
        exit 0
    fi

    MAX_TIMEOUT=300000
    if [ "$CURRENT_TIMEOUT" -le 0 ] 2>/dev/null || [ "$CURRENT_TIMEOUT" -gt "$MAX_TIMEOUT" ] 2>/dev/null; then
        python3 - <<PY
import json
print(json.dumps({
    "hookEventName": "PreToolUse",
    "updatedInput": {"timeout": $MAX_TIMEOUT}
}))
PY
    fi
fi

exit 0
