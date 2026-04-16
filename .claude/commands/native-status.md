---
description: "Snapshot the native-loop state without running tests — counts, current block reason, iteration."
allowed-tools: ["Bash(.claude/scripts/count-canaries.py:*)", "Bash(.claude/scripts/list-unported.py:*)", "Bash(grep:*)", "Bash(sed:*)", "Bash(cat:*)", "Bash(wc:*)"]
---

# Native-loop status snapshot

```!
STATE=".claude/native-loop.local.md"

if [ ! -f "$STATE" ]; then
    echo "ℹ️  Native loop is NOT active (no state file)."
    echo "    Run /native-loop to arm it."
    exit 0
fi

field() {
    sed -n "s/^$1: *//p" "$STATE" | head -1 | sed 's/^"\(.*\)"$/\1/'
}

TODO_PENDING=$(grep -cE '^\s*- \[ \]' TODO.md 2>/dev/null || echo 0)
TODO_DONE=$(grep -cE '^\s*- \[x\]' TODO.md 2>/dev/null || echo 0)
CANARIES=$(python3 .claude/scripts/count-canaries.py 2>/dev/null || echo "?")
PBT=$(python3 .claude/scripts/list-unported.py --kind pbtkit 2>/dev/null | wc -l | tr -d ' ')
HYP=$(python3 .claude/scripts/list-unported.py --kind hypothesis 2>/dev/null | wc -l | tr -d ' ')

echo "🔁 Native loop active"
echo "   iteration:          $(field iteration)"
echo "   max iterations:     $(field max_iterations)"
echo "   review phase armed: $(field review_phase_armed)"
echo "   last block reason:  $(field last_block_reason)"
echo "   stall count:        $(field stall_count)"
echo
echo "State snapshot:"
echo "   TODO pending:          $TODO_PENDING"
echo "   TODO completed:        $TODO_DONE"
echo "   CANARY panics left:    $CANARIES"
echo "   pbtkit tests unported: $PBT"
echo "   hypothesis unported:   $HYP"
```
