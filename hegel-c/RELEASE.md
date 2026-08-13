RELEASE_TYPE: patch

This patch makes shrinking much cheaper and better at escaping locally-minimal branches: finding the minimal counterexample for a failing test typically takes around 10x fewer test executions, and inputs whose true minimum sits in another `one_of`-style branch reach it more often than before. Individual runs are still randomized, so a particular seed may shrink differently than it used to, but every deterministic benchmark result is unchanged and the branch-escape rate is higher across the board.
