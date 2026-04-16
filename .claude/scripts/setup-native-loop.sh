#!/usr/bin/env bash
# Setup script for /native-loop.
#
# 1. Creates TODO.md and SKIPPED.md at the repo root if they are missing.
#    (Templates are in this repo already; this is belt-and-braces.)
# 2. Clones /tmp/pbtkit and /tmp/hypothesis (shallow) if missing.
# 3. Writes .claude/native-loop.local.md with session state + prompt body.
# 4. Prints a summary for the user.

set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
cd "$REPO_ROOT"

MAX_ITERATIONS=0
while [[ $# -gt 0 ]]; do
    case "$1" in
    --max-iterations)
        MAX_ITERATIONS="${2:-0}"
        shift 2
        ;;
    --help | -h)
        cat <<'EOF'
/native-loop — drive native-backend implementation to completion.

USAGE:
  /native-loop [--max-iterations N]

What it does:
  - Creates TODO.md + SKIPPED.md at the repo root if missing.
  - Clones /tmp/pbtkit and /tmp/hypothesis if missing.
  - Writes .claude/native-loop.local.md so the Stop hook activates.

The Stop hook will then gate every session exit on the gate chain
described in .claude/hooks/stop-hook.sh, picking one focused task per
iteration. You can only stop by emitting:
  <promise>I have fully completed the native backend implementation.</promise>
and only when it's genuinely true.

To abort, run /native-loop-cancel.
EOF
        exit 0
        ;;
    *)
        echo "unknown argument: $1" >&2
        exit 2
        ;;
    esac
done

# --- Ensure root state files exist ---
if [[ ! -f TODO.md ]]; then
    cat >TODO.md <<'EOF'
# Native backend implementation TODO

## In progress

## Pending

## Completed
EOF
fi

if [[ ! -f SKIPPED.md ]]; then
    cat >SKIPPED.md <<'EOF'
# Skipped upstream test files

## pbtkit (`/tmp/pbtkit/tests/`)

## hypothesis (`/tmp/hypothesis/hypothesis-python/tests/cover/`)
EOF
fi

# --- Clone upstream repos if missing ---
if [[ ! -d /tmp/pbtkit ]]; then
    echo "Cloning pbtkit into /tmp/pbtkit ..."
    git clone --depth 1 https://github.com/DRMacIver/pbtkit.git /tmp/pbtkit
fi

if [[ ! -d /tmp/hypothesis ]]; then
    echo "Cloning hypothesis into /tmp/hypothesis (LFS disabled) ..."
    GIT_LFS_SKIP_SMUDGE=1 git clone --depth 1 https://github.com/HypothesisWorks/hypothesis.git /tmp/hypothesis
fi

# --- Write state file ---
mkdir -p .claude
SESSION_ID="${CLAUDE_CODE_SESSION_ID:-$(date -u +%s)-$$}"
NOW="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
COMPLETION_PROMISE="I have fully completed the native backend implementation."

cat >.claude/native-loop.local.md <<EOF
---
active: true
iteration: 1
session_id: $SESSION_ID
max_iterations: $MAX_ITERATIONS
completion_promise: "$COMPLETION_PROMISE"
review_phase_armed: false
last_block_reason: ""
stall_count: 0
started_at: "$NOW"
---

You are implementing the native backend of hegel-rust. A persistent Stop
hook will keep driving you forward until the implementation is fully
complete, including quality polish. You can only stop by emitting
<promise>$COMPLETION_PROMISE</promise>
and only when that is genuinely true.

Each iteration, the Stop hook tells you the single next thing to do.
The guiding principle is: small incremental chunks of empirically
verifiable work, one focused commit per chunk.

Typical gate messages and the right response:

1. Format / lint / clippy failure → fix it, commit, move on.
2. Server-mode test failing → fix the one named test, commit.
3. Native-mode test failing → fix the one named test. If it's a CANARY
   panic, delete that panic line (the code path is now reached) and
   ensure the same commit has the test that reaches it.
4. Uncommitted work → make a focused commit describing this single
   change. Do not amend.
5. TODO item pending → work it; its acceptance-check line tells you how
   to verify it's done.
6. Unported pbtkit/hypothesis test → read the \`porting-tests\` skill.
   Port the single file named. If it surfaces a missing feature, stub
   the affected cases with \`todo!()\` and add a new TODO; commit the
   partial port.
7. Uncovered line in src/native → add a test that reaches it, or delete
   dead code. If the line is a canary, see #3.
8. Review-phase TODO → read the \`native-review\` skill, invoke the
   \`simplify\` skill for the file, commit focused improvements.
9. Final-polish finding → address the single finding, commit.
10. All clear → emit the completion promise.

Always: follow CLAUDE.md (100% coverage for new code, no new // nocov
without permission, tests under tests/ not inline). \`just lint\` and
\`just test\` must pass before every commit. Never \`--amend\`. Never
disable hooks with \`--no-verify\`.
EOF

# --- Summary ---
TODO_PENDING=$(grep -cE '^\s*- \[ \]' TODO.md || true)
CANARIES=$(python3 .claude/scripts/count-canaries.py 2>/dev/null || echo "?")
PBTKIT_UNPORTED=$(python3 .claude/scripts/list-unported.py --kind pbtkit 2>/dev/null | wc -l | tr -d ' ')
HYPOTHESIS_UNPORTED=$(python3 .claude/scripts/list-unported.py --kind hypothesis 2>/dev/null | wc -l | tr -d ' ')

cat <<EOF

🔁 Native implementation loop armed.
   state: .claude/native-loop.local.md
   max iterations: $([ "$MAX_ITERATIONS" -gt 0 ] && echo "$MAX_ITERATIONS" || echo "unbounded")

Current snapshot:
   TODO pending:          $TODO_PENDING
   CANARY panics left:    $CANARIES
   pbtkit tests unported: $PBTKIT_UNPORTED
   hypothesis unported:   $HYPOTHESIS_UNPORTED

The Stop hook (.claude/hooks/stop-hook.sh) will now gate every session
exit, picking one focused task per iteration. To abort, run /native-loop-cancel.

Completion promise: <promise>$COMPLETION_PROMISE</promise>
   (output ONLY when it is genuinely true)
EOF
