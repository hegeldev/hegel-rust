---
description: "Cancel the native-backend implementation loop. Removes the state file so the Stop hook stops gating."
allowed-tools: ["Bash(rm:*)"]
---

# Cancel the native-backend loop

```!
if [ -f .claude/native-loop.local.md ]; then
    rm .claude/native-loop.local.md
    echo "✅ .claude/native-loop.local.md removed. The Stop hook will no longer block session exit."
else
    echo "ℹ️  No active native-loop state file. Nothing to cancel."
fi
```
