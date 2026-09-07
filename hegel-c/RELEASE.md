RELEASE_TYPE: patch

This patch fixes a missed flakiness detection during targeting. A test whose data generation first changed shape during the targeted-search phase was silently ignored and the run carried on. It now fails the run with the usual non-determinism diagnostic, matching every other phase.
