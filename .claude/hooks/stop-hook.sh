#!/usr/bin/env bash
# Stop hook for the native-backend implementation loop.
#
# Gate chain (first-blocking wins):
#   ①  stop_hook_active flag on the Stop input → approve (recursion guard)
#   ②  session_id mismatch → approve (another session owns the loop)
#   ③  completion promise in last assistant text → approve + delete state
#   ④  iteration ≥ max_iterations > 0 → approve + delete state
#   ⑤  stall-detection (≥ 5 blocks in a row on same reason) → escalation
#   ⑥  cargo fmt --check / just check-format → block
#   ⑦  cargo test (server mode) → block on ONE failing test
#   ⑧  cargo test --features native → block on ONE failing test
#   ⑨  cargo clippy --all-features --tests -- -D warnings → block
#   ⑩  uncommitted work → block: commit-before-next-step
#   ⑪  TODO.md has `- [ ]` items → block on the first one
#   ⑫  unported pbtkit tests → block on the smallest
#   ⑬  unported hypothesis cover tests → block on the smallest
#   ⑭  native coverage below 100% → block on first uncovered line
#   ⑮  arm review phase (once): populate TODOs for every src/native/*.rs
#   ⑯  final-polish audit: missing docs, FIXME, unreviewed #[allow], etc.
#   ⑰  all clear → approve
#
# No set -e / -u — we always emit valid JSON, even on error.

STATE_FILE=".claude/native-loop.local.md"
OUTPUT_PRODUCED=false
GATE_TIMEOUT=300
COVERAGE_TIMEOUT=600

# ---------- hook output primitives ----------

approve() {
    if [ "$OUTPUT_PRODUCED" = "false" ]; then
        OUTPUT_PRODUCED=true
        echo '{"decision": "approve"}'
    fi
    exit 0
}

# block "reason" ["reason_tag"]
#
# Updates the state file with the new last_block_reason and a stall_count
# that tracks how many iterations have blocked on the same reason in a row.
block() {
    local reason="$1"
    local reason_tag="${2:-}"
    if [ "$OUTPUT_PRODUCED" = "false" ]; then
        OUTPUT_PRODUCED=true
        update_state_on_block "$reason_tag"
        python3 - <<'PY' "$reason"
import json, sys
reason = sys.argv[1]
print(json.dumps({"decision": "block", "reason": reason}))
PY
    fi
    exit 0
}

trap 'if [ "$OUTPUT_PRODUCED" = "false" ]; then echo "{\"decision\": \"approve\"}"; fi' EXIT

# ---------- read hook input ----------

INPUT=$(cat)
HOOK_SESSION_ID=$(printf '%s' "$INPUT" | python3 -c '
import sys, json
try:
    d = json.load(sys.stdin)
    print(d.get("session_id", ""))
except Exception:
    print("")
' 2>/dev/null)
STOP_HOOK_ACTIVE=$(printf '%s' "$INPUT" | python3 -c '
import sys, json
try:
    d = json.load(sys.stdin)
    print(str(d.get("stop_hook_active", False)).lower())
except Exception:
    print("false")
' 2>/dev/null)
TRANSCRIPT_PATH=$(printf '%s' "$INPUT" | python3 -c '
import sys, json
try:
    d = json.load(sys.stdin)
    print(d.get("transcript_path", ""))
except Exception:
    print("")
' 2>/dev/null)

# ① recursion guard
if [ "$STOP_HOOK_ACTIVE" = "true" ]; then
    approve
fi

# --- no state file → no loop ---
if [ ! -f "$STATE_FILE" ]; then
    approve
fi

# ---------- state helpers ----------

read_state_field() {
    local field="$1"
    sed -n "s/^${field}: *//p" "$STATE_FILE" | head -1 | sed 's/^"\(.*\)"$/\1/'
}

write_state_field() {
    # write_state_field field_name new_value
    # Rewrites an existing top-level field in the YAML frontmatter.
    local field="$1"
    local value="$2"
    local tmp
    tmp=$(mktemp)
    python3 - "$STATE_FILE" "$field" "$value" "$tmp" <<'PY'
import sys, re, pathlib
path, field, value, tmp = sys.argv[1:]
text = pathlib.Path(path).read_text()
# Only rewrite inside the frontmatter block (between the first two ---).
lines = text.splitlines(keepends=True)
out = []
in_front = False
seen_first = False
replaced = False
pat = re.compile(rf"^{re.escape(field)}:\s*.*$")
# Escape the value if it contains special chars.
need_quote = any(c in value for c in [":", "#", "\"", "'", "\n"])
val_out = value
if need_quote:
    val_out = '"' + value.replace('"', '\\"') + '"'
for line in lines:
    stripped = line.strip()
    if stripped == "---":
        if not seen_first:
            seen_first = True
            in_front = True
            out.append(line)
            continue
        elif in_front:
            if not replaced:
                out.append(f"{field}: {val_out}\n")
                replaced = True
            in_front = False
            out.append(line)
            continue
    if in_front and pat.match(line):
        out.append(f"{field}: {val_out}\n")
        replaced = True
    else:
        out.append(line)
pathlib.Path(tmp).write_text("".join(out))
PY
    mv "$tmp" "$STATE_FILE"
}

update_state_on_block() {
    local reason_tag="$1"
    local prev_reason current_stall
    prev_reason=$(read_state_field "last_block_reason")
    current_stall=$(read_state_field "stall_count")
    current_stall="${current_stall:-0}"

    if [ -n "$reason_tag" ] && [ "$reason_tag" = "$prev_reason" ]; then
        current_stall=$((current_stall + 1))
    else
        current_stall=1
    fi

    local iter
    iter=$(read_state_field "iteration")
    iter="${iter:-1}"
    iter=$((iter + 1))
    write_state_field "iteration" "$iter"
    write_state_field "last_block_reason" "$reason_tag"
    write_state_field "stall_count" "$current_stall"
}

# ② session isolation
STATE_SESSION_ID=$(read_state_field "session_id")
if [ -n "$STATE_SESSION_ID" ] && [ -n "$HOOK_SESSION_ID" ] && [ "$STATE_SESSION_ID" != "$HOOK_SESSION_ID" ]; then
    approve
fi

# ③ completion promise detection
COMPLETION_PROMISE=$(read_state_field "completion_promise")
if [ -n "$COMPLETION_PROMISE" ] && [ -n "$TRANSCRIPT_PATH" ] && [ -f "$TRANSCRIPT_PATH" ]; then
    PROMISE_FOUND=$(python3 - "$TRANSCRIPT_PATH" "$COMPLETION_PROMISE" <<'PY'
import sys, json, re
transcript_path, promise = sys.argv[1:]
tag = f"<promise>{promise}</promise>"
last_text = ""
try:
    with open(transcript_path) as f:
        for line in f:
            try:
                rec = json.loads(line)
            except Exception:
                continue
            # Claude Code transcript: role=assistant, content is list of blocks.
            if rec.get("message", {}).get("role") != "assistant":
                continue
            for block in rec.get("message", {}).get("content", []) or []:
                if isinstance(block, dict) and block.get("type") == "text":
                    last_text = block.get("text", "")
except Exception:
    print("false")
    sys.exit()
# Normalize whitespace and check for the tag.
normalized = " ".join(last_text.split())
print("true" if tag in last_text or tag in normalized else "false")
PY
)
    if [ "$PROMISE_FOUND" = "true" ]; then
        rm -f "$STATE_FILE"
        approve
    fi
fi

# ④ max iterations
MAX_ITER=$(read_state_field "max_iterations")
MAX_ITER="${MAX_ITER:-0}"
ITER=$(read_state_field "iteration")
ITER="${ITER:-1}"
if [ "$MAX_ITER" -gt 0 ] 2>/dev/null && [ "$ITER" -ge "$MAX_ITER" ] 2>/dev/null; then
    rm -f "$STATE_FILE"
    approve
fi

# ⑤ stall-detection prefix: any block below uses block() which updates stall.
# We handle the escalation inline by reading stall_count and adding a prefix
# when it's been blocking on the same reason repeatedly.
stall_prefix() {
    local tag="$1"
    local prev_reason stall
    prev_reason=$(read_state_field "last_block_reason")
    stall=$(read_state_field "stall_count")
    stall="${stall:-0}"
    if [ "$tag" = "$prev_reason" ] && [ "$stall" -ge 5 ] 2>/dev/null; then
        printf '\n⚠️  STALL DETECTED: this gate has blocked on the same reason for %s iterations in a row.\n\nStep back before re-attempting. Open TODO.md and add a new Pending item that captures:\n- What this blocker actually is\n- What you have already tried\n- What new approach you will try next\n\nThen commit the TODO update and proceed with that new approach. Do NOT retry the same fix a sixth time.\n\n---\n\n' "$stall"
    fi
}

# ---------- gate ⑥: format ----------

FMT_OUT=$(timeout "$GATE_TIMEOUT" cargo fmt --check 2>&1) || {
    PREFIX=$(stall_prefix "format")
    block "${PREFIX}cargo fmt --check is failing. Run \`just format\` (or \`cargo fmt\`) to apply formatting, review the diff, and commit with a message like 'Apply cargo fmt'. Output (last 20 lines):\n\n${FMT_OUT: -2000}" "format"
}

# ---------- gate ⑦: server-mode tests ----------

SERVER_TEST_OUT=$(timeout "$GATE_TIMEOUT" cargo test --no-fail-fast 2>&1)
SERVER_TEST_EXIT=$?
if [ "$SERVER_TEST_EXIT" -ne 0 ]; then
    # Pick one failing test.
    FAIL_LINE=$(printf '%s' "$SERVER_TEST_OUT" | grep -E '^test .* FAILED' | head -1 | sed -E 's/^test ([^ ]+) .*/\1/')
    PREFIX=$(stall_prefix "server_test:${FAIL_LINE}")
    if [ -z "$FAIL_LINE" ]; then
        MSG="Server-mode tests failed (exit $SERVER_TEST_EXIT) but no test name could be extracted. This may mean a compile error. Last 40 lines of output:\n\n$(printf '%s' "$SERVER_TEST_OUT" | tail -40)"
    else
        DETAIL=$(printf '%s' "$SERVER_TEST_OUT" | awk "/^---- $FAIL_LINE stdout ----/,/^failures:/" | head -60)
        MSG="Server-mode tests are failing. Fix this one test FIRST, then commit. Do not fix others in the same commit.\n\nFailing test: ${FAIL_LINE}\n\nFailure output:\n${DETAIL}\n\nRun \`cargo test ${FAIL_LINE}\` to iterate faster."
    fi
    block "${PREFIX}${MSG}" "server_test:${FAIL_LINE:-unknown}"
fi

# ---------- gate ⑧: native-mode tests ----------

NATIVE_TEST_OUT=$(timeout "$GATE_TIMEOUT" cargo test --features native --no-fail-fast 2>&1)
NATIVE_TEST_EXIT=$?
if [ "$NATIVE_TEST_EXIT" -ne 0 ]; then
    FAIL_LINE=$(printf '%s' "$NATIVE_TEST_OUT" | grep -E '^test .* FAILED' | head -1 | sed -E 's/^test ([^ ]+) .*/\1/')
    PREFIX=$(stall_prefix "native_test:${FAIL_LINE}")
    if [ -z "$FAIL_LINE" ]; then
        MSG="Native-mode tests failed (exit $NATIVE_TEST_EXIT) but no test name could be extracted. This may mean a compile error. Last 40 lines of output:\n\n$(printf '%s' "$NATIVE_TEST_OUT" | tail -40)"
    else
        DETAIL=$(printf '%s' "$NATIVE_TEST_OUT" | awk "/^---- $FAIL_LINE stdout ----/,/^failures:/" | head -80)
        CANARY_NUDGE=""
        if printf '%s' "$DETAIL" | grep -q 'CANARY:'; then
            CANARY_NUDGE=$(
                cat <<'EOF'

This is a canary panic. Canaries mark code paths that were previously
unreachable. This test now reaches the canary, which means the fix is to
DELETE the `panic!("CANARY:...")` line so the real code underneath runs.
The same commit should include the test that reaches it (if it's new).

After deleting, re-run `cargo test --features native` to see whether
the real code behaves correctly. If it now fails, that's the next bug
to fix — either the test is wrong, or the previously-unreachable code
has a bug that was masked by the canary.
EOF
            )
        fi
        MSG="Native-mode tests are failing. Fix this one test FIRST, then commit.\n\nFailing test: ${FAIL_LINE}\n\nFailure output:\n${DETAIL}${CANARY_NUDGE}\n\nRun \`cargo test --features native ${FAIL_LINE}\` to iterate faster."
    fi
    block "${PREFIX}${MSG}" "native_test:${FAIL_LINE:-unknown}"
fi

# ---------- gate ⑨: clippy ----------

CLIPPY_OUT=$(timeout "$GATE_TIMEOUT" cargo clippy --all-features --tests -- -D warnings 2>&1) || {
    PREFIX=$(stall_prefix "clippy")
    FIRST_WARN=$(printf '%s' "$CLIPPY_OUT" | grep -m1 -E '^(error|warning): ' || true)
    block "${PREFIX}cargo clippy is failing. Fix the warnings and commit.\n\nFirst issue:\n${FIRST_WARN}\n\nFull output (last 30 lines):\n$(printf '%s' "$CLIPPY_OUT" | tail -30)" "clippy"
}

# ---------- gate ⑩: uncommitted work ----------

GIT_STATUS=$(git status --porcelain 2>/dev/null || true)
if [ -n "$GIT_STATUS" ]; then
    PREFIX=$(stall_prefix "uncommitted")
    block "${PREFIX}All tests and lints pass, but you have uncommitted work. Make a focused commit describing the single change in this batch, then continue.\n\nUncommitted changes:\n${GIT_STATUS}\n\nReminder: never use \`--amend\` or \`--no-verify\`. Each commit should describe one focused change." "uncommitted"
fi

# ---------- gate ⑪: TODO.md items ----------

if [ -f TODO.md ]; then
    FIRST_TODO=$(python3 - <<'PY'
import re, pathlib
text = pathlib.Path("TODO.md").read_text()
# Grab the ## In progress section first, then ## Pending.
def section(name):
    m = re.search(rf"^##\s+{re.escape(name)}\s*$(.*?)(?=^##\s|\Z)", text, re.MULTILINE | re.DOTALL)
    return m.group(1) if m else ""

for name in ("In progress", "Pending"):
    body = section(name)
    for line in body.splitlines():
        stripped = line.rstrip()
        if re.match(r"^\s*- \[ \]", stripped):
            # Find following indented lines (acceptance-check, notes, etc).
            idx = body.splitlines().index(stripped)
            out = [stripped]
            for follow in body.splitlines()[idx + 1:]:
                if re.match(r"^\s*- \[[ x]\]", follow):
                    break
                if follow.strip() == "":
                    break
                out.append(follow.rstrip())
            print("\n".join(out))
            raise SystemExit
PY
)
    if [ -n "$FIRST_TODO" ]; then
        PREFIX=$(stall_prefix "todo")
        block "${PREFIX}Work on this TODO item (the first pending item in TODO.md). When done, mark it \`- [x]\` and move it into ## Completed. Follow any acceptance-check line the item includes.\n\n${FIRST_TODO}" "todo"
    fi
fi

# ---------- gate ⑫: unported pbtkit ----------

PBT=$(python3 .claude/scripts/list-unported.py --kind pbtkit --smallest 1 2>/dev/null || true)
if [ -n "$PBT" ]; then
    PREFIX=$(stall_prefix "unported_pbtkit")
    block "${PREFIX}Port this pbtkit test file next. It's the smallest file unported and not listed in SKIPPED.md.\n\nFile: ${PBT}\n\nRead the \`porting-tests\` skill before starting. Commit the port as a single focused commit. If the port surfaces a missing feature in the native backend, stub the affected test cases with \`todo!()\` and add a new TODO describing the missing feature — don't hand-patch the test around it." "unported_pbtkit:${PBT}"
fi

# ---------- gate ⑬: unported hypothesis ----------

HYP=$(python3 .claude/scripts/list-unported.py --kind hypothesis --smallest 1 2>/dev/null || true)
if [ -n "$HYP" ]; then
    PREFIX=$(stall_prefix "unported_hypothesis")
    block "${PREFIX}Port this Hypothesis test file next. It's the smallest unported file in tests/cover/ and not listed in SKIPPED.md.\n\nFile: ${HYP}\n\nRead the \`porting-tests\` skill before starting. Commit the port as a single focused commit." "unported_hypothesis:${HYP}"
fi

# ---------- gate ⑭: native coverage ----------

if [ -x scripts/check-coverage.py ]; then
    COV_OUT=$(timeout "$COVERAGE_TIMEOUT" python3 .claude/scripts/first-uncovered.py 2>&1)
    COV_EXIT=$?
    if [ "$COV_EXIT" -ne 0 ]; then
        PREFIX=$(stall_prefix "native_coverage")
        block "${PREFIX}Native-mode coverage is below 100%. Address this one line — either add a test that reaches it, or delete the code if it is truly unreachable. If the line is a CANARY panic, a test should already be reaching it (delete the panic); if no test reaches it, write one.\n\n${COV_OUT}" "native_coverage:${COV_OUT%%$'\n'*}"
    fi
fi

# ---------- gate ⑮: arm review phase ----------

REVIEW_ARMED=$(read_state_field "review_phase_armed")
if [ "$REVIEW_ARMED" != "true" ]; then
    # Everything else is green. Arm the review phase by appending one
    # Pending TODO per src/native/*.rs file and flipping the flag.
    python3 - <<'PY'
import pathlib, re
todo_path = pathlib.Path("TODO.md")
text = todo_path.read_text()

native_files = sorted(
    p.as_posix()
    for p in pathlib.Path("src/native").rglob("*.rs")
    if p.is_file()
)

new_entries = []
for f in native_files:
    new_entries.append(
        f"- [ ] Review & simplify {f}\n"
        f"    1. Read {f} end-to-end.\n"
        f"    2. Read the pbtkit counterpart under /tmp/pbtkit/src/pbtkit/ (best match).\n"
        f"    3. Read the Hypothesis counterpart under /tmp/hypothesis/hypothesis-python/src/hypothesis/internal/ (best match).\n"
        f"    4. Invoke the `simplify` skill scoped to this one file.\n"
        f"    5. Look for: dead code, duplication, unidiomatic Rust, unnecessary allocations/clones,\n"
        f"       misnamed items, over/under-abstraction, missing or stale doc comments on public items,\n"
        f"       `#[allow(...)]` attributes with no justification.\n"
        f"    6. Either (a) make focused improvements and commit (touching only this file),\n"
        f"       or (b) if a finding is too large for one commit, append a new Pending TODO for it\n"
        f"       and commit the smaller fixes you can make now.\n"
        f"    (verify: `just lint` and `cargo test --features native` still pass;\n"
        f"     commit touches only {f} unless a rename or cross-file refactor demands otherwise)\n"
    )

pending_header = "## Pending"
if pending_header in text:
    idx = text.index(pending_header) + len(pending_header)
    # Insert immediately after the header, before the next section or end.
    tail_start = text.find("\n##", idx)
    if tail_start == -1:
        tail_start = len(text)
    insertion = "\n\n" + "\n".join(new_entries) + "\n"
    text = text[:idx] + insertion + text[idx:tail_start].lstrip("\n") + text[tail_start:]
else:
    text += "\n## Pending\n\n" + "\n".join(new_entries) + "\n"

todo_path.write_text(text)
PY

    write_state_field "review_phase_armed" "true"
    block "Review phase armed. Every file under src/native/ has just been added to TODO.md as a review-and-simplify task. Pick the first one and work it.\n\nBefore starting each file, read .claude/skills/native-review/SKILL.md." "review_armed"
fi

# ---------- gate ⑯: final-polish audit ----------

# Runs only once everything else is clean AND review_phase_armed is already
# true. We do cheap grep-based checks in order, blocking on the first finding.

# 1. Missing docs on public items under src/native/.
MISSING_DOCS=$(RUSTDOCFLAGS="" timeout 120 cargo rustc --features native -- -W missing-docs 2>&1 | grep -E '^(warning|error): missing documentation' | head -1 || true)
if [ -n "$MISSING_DOCS" ]; then
    PREFIX=$(stall_prefix "polish_docs")
    block "${PREFIX}Final-polish audit: a public item under src/native/ is missing documentation.\n\n${MISSING_DOCS}\n\nAdd a brief doc comment (\`/// ...\`) explaining the item's purpose. Commit." "polish_docs"
fi

# 2. Any lingering // TODO / FIXME / XXX markers inside src/native/.
LEFTOVER_MARKER=$(grep -rEn '//\s*(TODO|FIXME|XXX)' src/native/ 2>/dev/null | head -1 || true)
if [ -n "$LEFTOVER_MARKER" ]; then
    PREFIX=$(stall_prefix "polish_marker")
    block "${PREFIX}Final-polish audit: a stale inline TODO/FIXME/XXX marker remains in src/native/.\n\n${LEFTOVER_MARKER}\n\nEither resolve the comment and remove the marker, or convert it to a TODO.md entry and remove the inline comment. Commit." "polish_marker"
fi

# 3. Unreviewed #[allow(...)] attributes.
LEFTOVER_ALLOW=$(grep -rEn '^\s*#\[allow\(' src/native/ 2>/dev/null | head -1 || true)
if [ -n "$LEFTOVER_ALLOW" ]; then
    PREFIX=$(stall_prefix "polish_allow")
    block "${PREFIX}Final-polish audit: a #[allow(...)] attribute appears in src/native/ without an accompanying justification comment.\n\n${LEFTOVER_ALLOW}\n\nAdd a comment above the attribute explaining WHY the lint is suppressed, or remove the suppression if the underlying issue can be fixed. Commit." "polish_allow"
fi

# 4. Surviving CANARY panics.
LEFTOVER_CANARY=$(grep -rn 'panic!("CANARY:' src/native/ 2>/dev/null | head -1 || true)
if [ -n "$LEFTOVER_CANARY" ]; then
    PREFIX=$(stall_prefix "polish_canary")
    block "${PREFIX}Final-polish audit: a CANARY panic is still present in src/native/.\n\n${LEFTOVER_CANARY}\n\nEvery canary should be either (a) reached by a test and deleted, or (b) proven truly unreachable and replaced with \`unreachable!()\`. Resolve this one and commit." "polish_canary"
fi

# ---------- gate ⑰: all clear ----------

block "🎉 All gates pass, TODO empty, everything ported, coverage 100%, review complete, final polish clean.\n\nBefore emitting the completion promise, re-check: is every requirement in the canonical prompt body (.claude/native-loop.local.md) genuinely met? If so, emit the completion promise:\n\n<promise>${COMPLETION_PROMISE}</promise>\n\nIf you spot anything off, add a new TODO.md item and keep working." "all_clear"
