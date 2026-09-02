# Nondeterminism project notes

Working notes for extending Hegel to handle nondeterministic tests (branch
`DRMacIver/nondeterminism`). This branch is an artefact: expect experiments, dead ends, and
heavy notes; the final implementation will be extracted from it. The things worth preserving
from each throwaway are knowledge, worked use cases, and tests — they live here.

- `design.md` — the current design. Kept up to date as experiments teach us things.
- `decisions.md` — append-only decision log with rationale.
- `research/map-*.md` — code maps of every determinism-dependent subsystem, with `path:line`
  refs (compiled 2026-09-02 at a0185a65).
- `research/sketch-v0.md` — the first mechanism sketch, kept because the reviews reference it.
- `research/critique-*.md` — adversarial reviews of sketch v0 that produced the current design.
- `experiments/` — one directory per experiment; `000-plan.md` is the sequence and status.

Experiment code lives in `/experiments` at the repo root (standalone crates, not workspace
members, so `just check` and the coverage ratchet are unaffected).
