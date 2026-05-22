RELEASE_TYPE: patch

The native backend (`--features native`) now persists failing examples to the
database as soon as they are discovered and after every shrink improvement,
matching Hypothesis's `save_choices`/`downgrade_choices` behaviour. Previously
the save happened only at the end of `run_main`, so killing the runner during
shrinking (Ctrl-C, SIGTERM) would lose the failure even though it had already
been found.
