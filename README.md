# This repository has moved

Hegel for Rust now lives in the Hegel monorepo at **https://github.com/hegeldev/hegel**.

| What you're looking for | Where it is now |
|---|---|
| The `hegeltest` crate (Rust frontend) and its proc macros | [`frontends/rust/`](https://github.com/hegeldev/hegel/tree/main/frontends/rust) |
| libhegel, the engine behind every Hegel frontend (`hegeltest-c`, formerly `hegel-c/` here) | [`libhegel/`](https://github.com/hegeldev/hegel/tree/main/libhegel) |
| Prebuilt libhegel binaries and the C header | [Releases](https://github.com/hegeldev/hegel/releases) on the new repository, tagged `libhegel/<version>` |
| Changelogs | [`frontends/rust/CHANGELOG.md`](https://github.com/hegeldev/hegel/blob/main/frontends/rust/CHANGELOG.md) and [`libhegel/CHANGELOG.md`](https://github.com/hegeldev/hegel/blob/main/libhegel/CHANGELOG.md) |
| Bug reports and feature requests | [hegeldev/hegel issues](https://github.com/hegeldev/hegel/issues) |

Nothing changes for users of the crates: `hegeltest`, `hegeltest-macros` and `hegeltest-c` keep their names on [crates.io](https://crates.io/crates/hegeltest), and the documentation is still at [docs.rs/hegeltest](https://docs.rs/hegeltest).

## Tags and history

The full history of this repository was merged into the monorepo unchanged, so every commit here exists there too. Releases are tagged by path and version in the new repository:

| Here | In hegeldev/hegel |
|---|---|
| `v0.50.1` (the `hegeltest` crate) | `frontends/rust/0.50.1` |
| `libhegel-v0.47.1` | `libhegel/0.47.1` |

Releases made before the move, and the libhegel binaries attached to them, remain available under this repository's [releases](https://github.com/hegeldev/hegel-rust/releases) so that existing pins keep working. New releases are published only to hegeldev/hegel.

For general information about Hegel, see https://hegel.dev.
