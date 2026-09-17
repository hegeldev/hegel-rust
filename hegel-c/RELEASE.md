RELEASE_TYPE: patch

This patch improves shrinking of values that must stay equal to each other, such as an element's opening and closing tag. The shrinker lowers such values as a group, and previously gave up on the whole group when an unrelated draw happened to hold the same number, leaving the pair at `1` or `2` where `0` would do. It now retries the group split by the draws' constraints, and with each member left out in turn.
