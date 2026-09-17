RELEASE_TYPE: patch

This patch improves shrinking in two situations.

Values that must stay equal to each other, such as an element's opening and closing tag, are lowered as a group. The shrinker previously gave up on the whole group when an unrelated draw happened to hold the same number, leaving the pair at `1` or `2` where `0` would do. It now retries the group split by the draws' constraints, and with each member left out in turn.

A list element whose deletion has to be paid for by a draw *after* the list — an index into the list, its declared length, a parity flag, a string with one character per element — is now deleted. The shrinker previously only paired a deletion with a change to the draw before it, so such examples kept dead elements in front of the one that mattered, and different runs of the same test settled on different numbers of them.
