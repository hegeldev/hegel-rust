RELEASE_TYPE: patch

This patch improves shrinking in two situations where the shrinker previously stopped well short of the minimal example.

Two integers that a test pins together (for example a pair that must differ by at most one) shrank one step at a time, alternating between the two, until the shrinker's improvement budget ran out. They are now lowered together, so such a pair reaches its minimum in a handful of steps regardless of how far away it started.

A shorter failing example that requires making one choice *less* simple — switching a `one_of` to a later, shorter alternative whose value must stay non-trivial, or flipping a boolean that replaces a collection with a single draw — was previously unreachable: the pass meant to find it proposed candidates the shrinker rejected before running them. The shrinker now runs those candidates and, when raising a choice changes the shape of the test case, also tries dropping each of the following draws to complete the switch.
