# Introduction

This book explains the `DRMacIver/nondeterminism` branch of hegel-rust: what it
built, and how it got there. The branch teaches Hegel's engine to handle tests
whose behaviour is not a deterministic function of their choice sequence —
tests with hidden state, real concurrency, or environmental dependence — and to
keep finding, shrinking, reporting, and reproducing bugs in them instead of
giving up.

The book has two parts.

**Part I: The branch as built** describes the implementation as it now stands,
mechanism by mechanism: how a run detects nondeterminism, how a failure origin
earns confirmation, how shrinking works when a single replay proves nothing,
what the final replay and the failure database do, and what changed at the C
ABI and in the frontend. It closes with where the branch sits in its plan.

**Part II: How it got here** is the history: the design sketch and the
critiques that reshaped it, the twelve experiments and what each one settled,
the production and remediation phases, the removal of the data tree, and the
false starts and lessons collected along the way.

The branch is an artefact, per `notes/README.md`: it was taken to production
grade in place, and the final implementation will be extracted from it later
(decision 26). This book is a synthesis for that extraction and for anyone who
needs to understand the branch whole. The primary sources remain authoritative:
`notes/design.md` for the as-built design, `notes/decisions.md` for the
append-only decision log, `notes/research/` for the maps and critiques,
`notes/experiments/` for the experiment write-ups, and the git history for the
sequence of events. Decisions are cited as (decision N) and experiments as
(experiment NNN) throughout.

The book was compiled by Claude from the branch's notes, code, and git
history, and fact-checked against those sources.
