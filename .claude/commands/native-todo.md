---
description: "Print the current TODO.md and SKIPPED.md contents."
allowed-tools: ["Bash(cat:*)"]
---

# Show the native-loop TODO and skip lists

```!
echo "=================== TODO.md ==================="
cat TODO.md 2>/dev/null || echo "(TODO.md not present)"
echo
echo "================= SKIPPED.md ==================="
cat SKIPPED.md 2>/dev/null || echo "(SKIPPED.md not present)"
```
