# Nondeterminism project notes

Working notes for extending Hegel to handle nondeterministic tests (branch
`DRMacIver/nondeterminism`). This branch is an artefact: expect experiments, dead ends, and
heavy notes; the final implementation will be extracted from it. The things worth preserving
from each throwaway are knowledge, worked use cases, and tests — they live here.

- `design.md` — the as-built design, rewritten once the implementation landed.
- `decisions.md` — append-only decision log with rationale.
- `research/map-*.md` — code maps of every determinism-dependent subsystem, with `path:line`
  refs (compiled 2026-09-02 at a0185a65).
- `research/sketch-v0.md` — the first mechanism sketch, kept because the reviews reference it.
- `research/critique-*.md` — adversarial reviews: of sketch v0 (producing the current
  design), and of the as-built implementation (`critique-asbuilt.md`, 2026-09-03).
- `remediation-plan.md` — the fix plan for the as-built review's findings; continues
  `production-plan.md`'s phases, gates, and experiment series.
- `seam-plan.md` — the implementation plan for gate G20's resolution (tree removal, the
  first-interesting check, history and backtracking); continues the same series.
- `experiments/` — one directory per experiment; `000-plan.md` is the sequence and status.

Experiment code lives in `/experiments` at the repo root (standalone crates, not workspace
members, so `just check` and the coverage ratchet are unaffected).
