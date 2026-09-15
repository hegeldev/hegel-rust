RELEASE_TYPE: patch

This patch improves shrinking in four situations found by running Hegel against real-world bugs:

- A draw that decides whether later draws happen can now be raised when raising it makes the test case shorter, so a failing example no longer keeps an extra draw around just because the draw controlling it would have to grow.
- The randomised final shrink pass now hands each improvement it finds straight back to the deterministic passes instead of continuing to walk the value one step at a time, so failures with many irrelevant draws no longer exhaust the shrink budget before reaching the minimal example.
- A bounded float draw whose failing values exclude zero now shrinks to the simplest non-zero value in its range instead of stopping at an arbitrary tiny value.
- Integer shrinking now also tries dividing the value by 3, 5, 7 and 10, and tries raising one draw to its maximum while lowering another, so failures that only occur at multiples of a round number, or that trade one draw off against another, shrink further.
