RELEASE_TYPE: patch

The native backend (`--features native`) now iterates targeting labels, shrink origins, and changed-node indices in a deterministic order, so a seeded run with multiple targets or failure origins is reproducible run-to-run.
