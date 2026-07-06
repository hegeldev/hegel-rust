RELEASE_TYPE: minor

This release changes `hegel_test_case_clone` to hand out an *independent
stream* of the test case rather than a view onto the same choice sequence.
A clone still shares the test case's outcome — `hegel_mark_complete` on any
handle completes the whole family, and the choice budget is shared — but it
generates from its own choice sequence, so clones can be driven
concurrently from different threads without perturbing each other, and the
values every stream produces are deterministic under replay and shrink
correctly. Previously concurrent clone draws interleaved into one shared
sequence, which was explicitly non-deterministic.

Each cloned stream is recorded as a single choice in the stream it was
cloned from, so cloning now consumes one choice position on the source
handle, takes the source handle's lock like a draw (it can return
`HEGEL_E_CONCURRENT_USE` on contention), and fails with
`HEGEL_E_ALREADY_COMPLETE` once the test case has completed, where it
previously succeeded and returned a dead handle. Reproduce blobs now encode
the cloned streams' choices alongside their parent's, so blobs from tests
that clone are not readable by older libhegel versions.

Collections, variable pools, and state machines remain shared across the
family — ids from one handle work on any other — but concurrent use of one
such object from two streams makes the affected values scheduling-dependent.
