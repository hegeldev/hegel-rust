RELEASE_TYPE: patch

This release only affects libhegel users and is otherwise a pure refactoring.

In libhegel, an invalid schema would abort the process improperly when the rust code panicked. Now the rust code uses Result everywhere internally and properly propagates these errors to callers.
