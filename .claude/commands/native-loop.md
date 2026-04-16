---
description: "Start the native-backend implementation loop. The Stop hook will gate session exit on a chain of checks, driving work to completion."
argument-hint: "[--max-iterations N]"
allowed-tools: ["Bash(.claude/scripts/setup-native-loop.sh:*)"]
---

# Start the native-backend implementation loop

Run the setup script to arm the loop:

```!
bash .claude/scripts/setup-native-loop.sh $ARGUMENTS
```

You are now inside the loop. Every time you try to stop, the Stop hook
(`.claude/hooks/stop-hook.sh`) runs a gate chain that picks exactly one
focused task and tells you to do it. The loop ends only when you output
the completion promise specified in `.claude/native-loop.local.md`, and
only when that promise is genuinely true.

Read `.claude/skills/porting-tests/SKILL.md` before porting any test,
and `.claude/skills/native-review/SKILL.md` before starting a review-phase
TODO. Commit after every focused change. Never `--amend`, never
`--no-verify`.

To abort, run `/native-loop-cancel`.
