# Native backend implementation TODO

Tracks outstanding work on the `DRMacIver/native` branch's native backend.
Driven by the Stop hook in `.claude/hooks/stop-hook.sh` — see
`.claude/skills/porting-tests/SKILL.md` for the loop's workflow.

Every auto-generated item includes an acceptance check (a command or
condition that verifies completion). Check items off with `- [x]` as you
finish them and move them to the Completed section.

## In progress

## Pending

- [ ] **E2E orchestrator marker test**: Create a file `E2E_MARKER.md` at the repo root containing the single line `e2e merge test marker`.

  Acceptance: `test -f E2E_MARKER.md && grep -qx 'e2e merge test marker' E2E_MARKER.md`

## Completed
