Every Hegel run starts from a [`Settings`](crate::Settings) value. This
page covers what the settings are, the places each one can be set, how
those places combine, and how the *profile* a run starts from is chosen.

# The settings

| Setting | Values | Base value | Effect |
|---|---|---|---|
| `test_cases` | integer ≥ 1 | `100` | How many test cases to run. |
| `verbosity` | `quiet`, `normal`, `verbose`, `debug` | `normal` | How much Hegel prints ([`Verbosity`](crate::Verbosity)). |
| `seed` | integer, or none | none | A fixed seed for reproducibility; none means a fresh random seed per run. |
| `derandomize` | boolean | `false` | Use a fixed seed derived from the test name, so every run of a test is the same. |
| `database` | a path, `disabled`, or `default` | `default` | Where failing examples are stored for replay; `default` is `.hegel/examples` under the working directory. |
| `suppress_health_check` | list of `filter_too_much`, `too_slow`, `test_cases_too_large`, `large_initial_test_case`, or `all` | none | Health checks that should not fail the run ([`HealthCheck`](crate::HealthCheck)). |
| `phases` | list of `explicit`, `reuse`, `generate`, `target`, `shrink` | all five | Which parts of the run happen ([`Phase`](crate::Phase)); leaving out `shrink`, say, reports the first counterexample found. |
| `report_multiple_failures` | boolean | `false` | Report every distinct failure a run finds rather than collapsing to one. |
| `show_statistics` | boolean | `false` | Print the end-of-run statistics report for events recorded with [`TestCase::event`](crate::TestCase::event) and [`TestCase::event_value`](crate::TestCase::event_value). |
| `print_blob` | boolean | `false` | On failure, print a copy-pasteable `#[hegel::reproduce_failure("…")]` line. |
| `backend` | `default`, `urandom` | `default` | The source of randomness ([`Backend`](crate::Backend)): a seeded PRNG, or fresh bytes from `/dev/urandom` on every draw for Antithesis's fuzzer to control. |

The "Values" column is the vocabulary `hegel.toml` and the command-line
flags use. In Rust the same settings are the builder methods on
[`Settings`](crate::Settings), and the `#[hegel::test]` attribute arguments
are those methods by name.

# Where settings come from

A run's settings are built in layers. Each layer overrides what the
layers below it set and leaves the rest alone:

1. **The base settings**: the values in the table above. Immutable, and
   available by name as the reserved `base` profile.
2. **The profile**: a named set of overrides resolved by the engine, from
   the profiles shipped with Hegel, your `hegel.toml`, and programmatic
   registration. [`Settings::new`](crate::Settings::new) resolves the
   `default` profile, the one in effect for the current environment;
   [`Settings::from_profile`](crate::Settings::from_profile) resolves any
   profile by name. The rest of this page is mostly about this layer.
3. **Settings compiled into the test**: builder-method calls on the
   `Settings` value, or equivalently the arguments of `#[hegel::test]`,
   `#[hegel::main]`, and the other attribute macros:

   ```no_run
   use hegel::TestCase;
   use hegel::generators as gs;

   #[hegel::test(test_cases = 500, derandomize = true)]
   fn compiled_in(tc: TestCase) {
       tc.draw(gs::booleans());
   }
   ```

   Each `key = value` argument becomes a call to the builder method of the
   same name on the settings the profile produced. Two arguments are
   special: `profile = "<name>"` starts from
   [`Settings::from_profile`](crate::Settings::from_profile) instead of
   [`Settings::new`](crate::Settings::new), and a single positional
   expression (`#[hegel::test(my_settings())]`) supplies a complete
   `Settings` value to start from instead, which cannot be combined with
   `profile`.
4. **Command-line flags**, for `#[hegel::main]` binaries only: `--seed`,
   `--verbosity`, `--derandomize`, `--database`,
   `--suppress-health-check`, and `--backend` apply on top of the
   compiled-in settings. `--profile <name>` is different: it sets the
   process's default profile (see [Choosing the default
   profile](#choosing-the-default-profile)) before the compiled-in
   settings are evaluated, so they still apply on top of the named
   profile. A `#[hegel::main]` binary always runs exactly one test case,
   and suppresses the `too_slow` and `test_cases_too_large` health checks,
   which judge how valid cases accumulate over a run.
5. **Environment variables**, applied last, once per run, so they win over
   everything in source:

   | Variable | Effect |
   |---|---|
   | `HEGEL_TEST_CASES` | Overrides `test_cases`. Must be a positive integer. |
   | `HEGEL_DATABASE` | Overrides `database`: `disabled` turns it off, any other value is the path. |
   | `HEGEL_STATISTICS` | Anything but `0` or the empty string turns `show_statistics` on. |

   An empty variable is ignored.

`HEGEL_DEFAULT_PROFILE` and `HEGEL_CONFIG` also come from the environment
but act on layer 2, choosing the default profile and the config file; they
are described below.

For example, with this `hegel.toml`:

```toml
[profiles.ci]
test_cases = 1000
```

a test declared `#[hegel::test(test_cases = 200)]` and run on a CI server
with `HEGEL_TEST_CASES=5000` resolves the `ci` profile (1000 test cases,
derandomized, database disabled, `print_blob` on), the attribute overrides
`test_cases` to 200, and the environment variable overrides it again to
5000. Locally, without the variable, the same test runs 200 cases with the
`development` profile's settings for everything else.

# Profiles

A profile is a named delta over another profile: a set of settings it
overrides, plus the profile it extends. Resolving a profile walks that
chain down to the base settings and applies the deltas bottom-up.

## The two reserved names

- **`base`** is the immutable base settings from the table above. It
  terminates every chain, so a profile that extends `base` — or a test
  that selects it — gets exactly the base settings plus its own overrides,
  whatever the environment.
- **`default`** is the default profile: the one a run gets when nothing
  names a profile, and the one custom profiles extend when they don't say
  otherwise. It is an alias, resolved as described under [Choosing the
  default profile](#choosing-the-default-profile).

Neither can be modified or registered, and `default` cannot be named as
its own target (`default = "default"` in `hegel.toml`, or
`HEGEL_DEFAULT_PROFILE=default`) — that is an error.

## The shipped profiles

Three ordinary profiles ship with Hegel. All three extend `base` directly:
they are siblings, and none layers over another.

| Profile | Overrides | When it is the environment's profile |
|---|---|---|
| `development` | nothing | Locally: whenever neither of the others applies. |
| `ci` | `derandomize = true`, `database = "disabled"`, `suppress_health_check = ["too_slow"]`, `print_blob = true` | On a CI server, detected from `CI`, `GITHUB_ACTIONS`, `GITLAB_CI`, `BUILDKITE`, `CIRCLECI`, and the variables other common services set. |
| `workload` | `backend = "urandom"`, `database = "disabled"`, `suppress_health_check = ["all"]` | Inside [Antithesis](https://antithesis.com/), detected from `ANTITHESIS_OUTPUT_DIR`. Antithesis's fuzzer controls `/dev/urandom`, so the `urandom` backend hands it every choice; and Antithesis pauses threads, which would trip wall-clock health checks such as `too_slow` spuriously. |

The `ci` profile's `print_blob = true` is why a failing test on CI prints a
`#[hegel::reproduce_failure("…")]` line: with the database disabled, the
blob is the only way to reproduce that failure locally. Set
`print_blob = false` under `[profiles.ci]` to turn it off.

## Custom profiles and inheritance

Custom profiles are defined in `hegel.toml` (see below) or registered
programmatically. A `hegel.toml` profile chooses its parent with
`extends`; without one, a custom profile extends `default` and a shipped
profile extends `base`. So with

```toml
[profiles.nightly]
test_cases = 10000

[profiles.pinned]
extends = "base"
test_cases = 10000

[profiles.deep-ci]
extends = "ci"
test_cases = 100000
```

- `nightly` resolves as `nightly` → `ci` → `base` on a CI server and
  `nightly` → `development` → `base` locally: it gets the environment's
  behaviour plus more test cases, wherever it is selected from.
- `pinned` resolves as `pinned` → `base` everywhere: 10000 test cases, a
  fresh seed, the database on, no reproducer line, even on CI.
- `deep-ci` resolves as `deep-ci` → `ci` → `base` everywhere.

Because the shipped profiles are siblings, a delta meant for every
environment does not go in `development`; it goes in a profile of its own
that the others name with `extends`:

```toml
[profiles.common]
test_cases = 200

[profiles.development]
extends = "common"

[profiles.ci]
extends = "common"
```

A `hegel.toml` section for a shipped profile merges over the shipped delta
(`[profiles.ci]` with `print_blob = false` keeps derandomization and the
disabled database), and may set `extends` to change its parent, as above.

When the `default` alias appears in the middle of a chain — a custom
profile without `extends` — it skips any candidate already in the chain,
so a chain never revisits a profile. If `ci` is configured with
`extends = "common"` and `common` has no `extends`, resolving `ci` on a CI
server goes `ci` → `common` → `base`, not back to `ci`. When every
candidate has been visited the alias resolves to `base`.

An `extends` naming an unknown profile, or a chain that revisits a profile
through explicit `extends` links, is an error. Every profile in a
`hegel.toml` is checked whenever any profile is resolved, so a broken
profile fails loudly even before something selects it.

## Choosing the default profile

The `default` alias resolves to the strongest of these that is set; the
weaker ones are ignored entirely, not kept as fallbacks:

1. [`Settings::set_default_profile`](crate::Settings::set_default_profile),
   which the `--profile` flag of a `#[hegel::main]` binary calls.
2. The `HEGEL_DEFAULT_PROFILE` environment variable, when non-empty.
3. The top-level `default = "<profile>"` entry in `hegel.toml`.
4. The environment's profile: `workload` inside Antithesis, else `ci` on
   a CI server, else `development`.

Naming a profile that does not exist is an error, reported with the list
of known profiles when settings are resolved (an unknown `--profile` is a
usage error at startup). The environment's profile is still part of the
picture when one of the first three is set: it is what a custom profile
falls back to extending once the named default is already in its chain.

```bash
HEGEL_DEFAULT_PROFILE=nightly cargo test
```

## Selecting a profile for one test

`#[hegel::test(profile = "nightly")]`,
[`Settings::from_profile`](crate::Settings::from_profile), and
[`Settings::try_from_profile`](crate::Settings::try_from_profile) resolve a
profile by name for one test without changing what `default` is. The
selected profile still sits on its usual parent, so `nightly` selected
this way on CI still inherits from `ci`. Selecting `base` gives the plain
base settings.

# `hegel.toml`

## Discovery

The engine looks for `hegel.toml` in the test process's working directory
and then each ancestor up to the filesystem root; the first file found
wins outright, and configs never merge across files. Cargo runs tests from
the package directory, so a file at either the package or the workspace
root is found. The file is read once per process.

When the test process runs somewhere else — a Bazel sandbox, say — set
`HEGEL_CONFIG` to the file's path to load it directly, skipping the
search. A non-empty `HEGEL_CONFIG` that cannot be read is an error, not an
ignored config. Under `verbosity = "debug"` each run logs which config
file it loaded, or that it loaded none.

## Format

The file is TOML: an optional top-level `default = "<profile>"` entry and
`[profiles.<name>]` tables whose entries are the settings keys. The
vocabulary is strict: an unknown key, a value of the wrong type, a
misplaced `default`, or any other top-level key is an error carrying the
file and line number, as is malformed TOML. Profile names use ASCII
letters, digits, `-` and `_`; `base` and `default` cannot be sections.

```toml
default = "nightly"      # optional: the default profile for this project

[profiles.development]
test_cases = 200

[profiles.ci]            # merges onto the shipped ci profile
test_cases = 1000
print_blob = false

[profiles.nightly]
extends = "ci"
test_cases = 10000
seed = "none"
suppress_health_check = ["too_slow", "filter_too_much"]
phases = ["explicit", "reuse", "generate", "target", "shrink"]
database = "default"
backend = "default"
```

Every key from the settings table is accepted with the vocabulary shown
there, plus `extends`. Two values exist to undo a parent's setting:
`seed = "none"` clears an inherited seed, and `database = "default"`
restores the default database after a parent disabled it or set a path.

# Programmatic profiles

[`Settings::register_profile`](crate::Settings::register_profile) stores a
complete snapshot of a `Settings` value under a name, process-wide:

```no_run
use hegel::Settings;

Settings::register_profile("nightly", Settings::from_profile("ci").test_cases(10_000));
```

A registered profile terminates the chain, like `base`: it has no parent,
because the snapshot already holds every setting. Registering a shipped
profile's name replaces that profile wholesale. A `hegel.toml` section of
the same name still merges over the snapshot, but may not set `extends`
on it. Registering again under the same name replaces the earlier
snapshot, and the reserved names are rejected.

[`Settings::set_default_profile`](crate::Settings::set_default_profile)
sets the process's default profile, as the strongest candidate for the
`default` alias. The name does not have to exist yet.

Neither call is retroactive: settings values already created keep their
fields. Both must therefore run before the tests that should see them,
which a `#[hegel::main]` binary or an embedding controls but `cargo test`
does not. Under `cargo test`, `hegel.toml` is the reliable way to define
profiles and choose the default; the Rust API is for entry points that
run first.

# Environment variables

| Variable | Read by | Effect |
|---|---|---|
| `HEGEL_DEFAULT_PROFILE` | profile resolution | The default profile, unless `Settings::set_default_profile` or `--profile` set one. |
| `HEGEL_CONFIG` | config loading | Path of the `hegel.toml` to load instead of searching for one. |
| `HEGEL_TEST_CASES` | each run | Overrides `test_cases`, after every other layer. |
| `HEGEL_DATABASE` | each run | Overrides `database`, after every other layer. |
| `HEGEL_STATISTICS` | each run | Turns `show_statistics` on, after every other layer. |
| `ANTITHESIS_OUTPUT_DIR` | environment detection | Selects the `workload` profile. Must name an existing directory. |
| `CI`, `GITHUB_ACTIONS`, … | environment detection | Selects the `ci` profile. |
