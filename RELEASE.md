RELEASE_TYPE: patch

This patch improves shrinking: finding the minimal counterexample for a failing test typically takes around 10x fewer test executions, and inputs whose true minimum sits in another `one_of`-style branch reach it more often than before. 
