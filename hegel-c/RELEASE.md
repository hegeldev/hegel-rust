RELEASE_TYPE: patch

This patch adds `hegel_run_result_database_path`, which reports the database
directory a failing run saved its counterexamples to (NULL when nothing was
saved), so frontends can point at the saved example in their failure output.
