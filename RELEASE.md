RELEASE_TYPE: minor

The native engine (`--features native`) gains a urandom backend, selectable with `Settings::backend(Backend::Urandom)`, the `#[hegel::test(backend = Backend::Urandom)]` attribute, or the `--backend urandom` CLI flag.

In this mode every random choice is drawn from a fresh, unbuffered read of `/dev/urandom` instead of expanding a single seeded PRNG. This exists for running under [Antithesis](https://antithesis.com/), whose fuzzer controls the bytes `/dev/urandom` returns — sourcing every choice from the OS random device hands the fuzzer control over the whole test case (not just the PRNG seed) so it can steer and reproduce generation directly. The generation algorithm is otherwise unchanged; only the source of randomness differs, mirroring Hypothesis's `backend="hypothesis-urandom"`.

When running inside Antithesis the urandom backend is selected automatically unless you pin one explicitly. On platforms without `/dev/urandom` (Windows) it falls back to an OS-seeded PRNG. If you are not running under Antithesis you almost certainly want the default backend.

Relatedly, a weighted boolean draw now spends exactly one byte of entropy (matching Hypothesis's bytestring provider) rather than a full 64-bit float. This keeps a single-bit decision from consuming eight fuzzer-controlled bytes under the urandom backend.
