RELEASE_TYPE: minor

The native engine now bounds the shrinking phase by wall-clock time. If shrinking a failing example runs for more than five minutes it stops, reports the smallest counterexample found so far, and prints a warning, instead of potentially running for a very long time on tests whose body is slow to execute. Re-running resumes shrinking from the saved example. This mirrors Hypothesis's `MAX_SHRINKING_SECONDS` safety valve.
