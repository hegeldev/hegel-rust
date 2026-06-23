RELEASE_TYPE: minor

This release is a major breaking change to the libhegel C ABI, but will have
no effect on hegel-rust users.

It contains three distinct changes:

* Every function other than `hegel_context_new` and `hegel_context_last_error
   now takes a `hegel_context_t*` as its first argument and returns a
  `hegel_result_t` code (`HEGEL_OK` is zero; negatives are errors)
  All return values are through out parameters at the end of the function.
  This should simplify having a uniform error reporting interface for consumers.
* The engine no longer has a concept of "final test case". It runs until completion,
  then the caller is responsible for handling running the final test case(s).
  This makes it easier to handle error reporting and debugger integration in
  calling code, as it gives more control of how errors are propagated.
* All settings methods for configuring a settings object now have `_set_` in
  their name to indicate they are setter rather than getter methods (getters
  will be added in a future release).
