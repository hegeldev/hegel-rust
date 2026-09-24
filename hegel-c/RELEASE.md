RELEASE_TYPE: patch

This patch improves shrinking in four situations.

Values that must stay equal to each other, such as an element's opening and closing tag, are lowered as a group. The shrinker previously gave up on the whole group when an unrelated draw happened to hold the same number, leaving the pair at `1` or `2` where `0` would do. It now retries the group split by the draws' constraints, and with each member left out in turn.

A list element whose deletion has to be paid for by a draw *after* the list — an index into the list, its declared length, a parity flag, a string with one character per element — is now deleted. The shrinker previously only paired a deletion with a change to the draw before it, so such examples kept dead elements in front of the one that mattered, and different runs of the same test settled on different numbers of them.

Two numbers bound by their product — a duration and a multiplier whose product must overflow, two floats whose difference must overflow — now shrink together. The shrinker raises the later draw to the end of its range, or by a factor of two or ten, while scaling the earlier draw down to keep the product, and then finishes the earlier draw on its own. It previously left such pairs wherever the first draw happened to stall, and could spend its whole improvement budget walking one of them down a step at a time.

An integer whose failing values are sparse multiples, such as every thousandth value, now shrinks to the first multiple. The shrinker's divisions stalled on a prime factor (`61_000` for multiples of `1000`); it now also drops every digit but the trailing zeros, in decimal and in binary.
