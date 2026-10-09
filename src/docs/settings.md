Every Hegel run starts from a [`Settings`](crate::Settings) value. This
page covers what the settings are, the places each one can be set, how
those places combine, and how the *profile* a run starts from is chosen.

# The settings

| Setting | Values | Base value | Effect |
|---|---|---|---|
| `test_cases` | integer ≥ 1 | `100` | How many test cases to run. |
| `verbosity` | `quiet`, `normal`, `verbose`, `debug` | `normal` | How much Hegel prints ([`Verbosity`](crate::Verbosity)). |
| `seed` | integer, or none | none | A fixed seed for reproducibility; none means a fresh random seed per run. |
| `derandomize` | boolean | `false` | Use a fixed seed derived from the test name, so every run of a test is the same. A fixed `seed` takes precedence. |
| `database` | a path, `disabled`, or `default` | `default` | Where failing examples are stored for replay; `default` is `.hegel/examples` under the working directory. |
| `suppress_health_check` | list of `filter_too_much`, `too_slow`, `test_cases_too_large`, `large_initial_test_case`, or `all` | none | Health checks that should not fail the run ([`HealthCheck`](crate::HealthCheck)). Suppressing `test_cases_too_large` also removes the limit of 2^20 choices per test case. |
| `phases` | list of `explicit`, `reuse`, `generate`, `target`, `shrink` | all five | Which parts of the run happen ([`Phase`](crate::Phase)); leaving out `shrink`, say, reports the first counterexample found. |
| `report_multiple_failures` | boolean | `false` | Report every distinct failure a run finds rather than collapsing to one. |
| `show_statistics` | boolean | `false` | Print the end-of-run statistics report for events recorded with [`TestCase::event`](crate::TestCase::event) and [`TestCase::event_value`](crate::TestCase::event_value). |
| `print_blob` | boolean | `true` | On failure, print a copy-pasteable `#[hegel::reproduce_failure("…")]` line. |
| `backend` | `default`, `urandom` | `default` | The source of randomness ([`Backend`](crate::Backend)): a seeded PRNG, or fresh bytes from `/dev/urandom` on every draw for Antithesis's fuzzer to control. |
| `nondeterminism_strictness` | `quiet`, `warn`, `error` | `quiet` | How a run reacts when it detects nondeterministic test behavior ([`NondeterminismStrictness`](crate::NondeterminismStrictness)): switch to nondeterministic handling silently, switch with a one-line notice, or abort the run with a flaky-test error. |

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
3. **Environment variables**, applied by the engine over the resolved
   profile whenever a `Settings` value is created, so they win over the
   profile and `hegel.toml` for every test that does not set the same
   setting itself:

   | Variable | Effect |
   |---|---|
   | `HEGEL_TEST_CASES` | Sets `test_cases`. Must be a positive integer. |
   | `HEGEL_DATABASE` | Sets `database`: `disabled` turns it off, any other value is the path. |
   | `HEGEL_STATISTICS` | Anything but `0` or the empty string turns `show_statistics` on. |
   | `HEGEL_SEED` | Sets `seed`: an integer is the seed, `none` clears one the profile set. |
   | `HEGEL_DERANDOMIZE` | Sets `derandomize`: `true`, `1` or `yes`, or `false`, `0` or `no`. |
   | `HEGEL_PRINT_BLOB` | Sets `print_blob`, with the same vocabulary. |
   | `HEGEL_STATEFUL_STEPS` | Replaces every state machine's step count ([`stateful::Machine::steps`](crate::stateful::Machine::steps)). Must be a positive integer. |
   | `HEGEL_NONDETERMINISM_STRICTNESS` | Sets `nondeterminism_strictness`: `quiet`, `warn` or `error`. |

   An empty variable is ignored; any other malformed value is an error,
   raised when the `Settings` value is created. The variables do not
   change how the settings combine: a fixed `seed`, from wherever it
   came, still takes precedence over `derandomize`, so
   `HEGEL_DERANDOMIZE=true` has no effect on a profile with a seed unless
   `HEGEL_SEED=none` clears it.
4. **Settings compiled into the test**: builder-method calls on the
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
   `profile`. These are the settings the author of the test chose, so they
   take precedence over the environment variables: a variable adjusts the
   suite, not a test that pins the setting.
5. **Command-line flags**, for `#[hegel::main]` binaries only: `--seed`,
   `--verbosity`, `--derandomize`, `--database`,
   `--suppress-health-check`, and `--backend` apply on top of the
   compiled-in settings. `--profile <name>` is different: it sets the
   process's default profile (see [Choosing the default
   profile](#choosing-the-default-profile)) before the compiled-in
   settings are evaluated, so they still apply on top of the named
   profile. A `#[hegel::main]` binary always runs exactly one test case,
   and suppresses the `too_slow` and `test_cases_too_large` health checks,
   which judge how valid cases accumulate over a run.

`HEGEL_DEFAULT_PROFILE` and `HEGEL_CONFIG` also come from the environment
but act on layer 2, choosing the default profile and the config file; they
are described below.

For example, with this `hegel.toml`:

```toml
[profiles.ci]
test_cases = 1000
```

a test declared `#[hegel::test]` and run on a CI server with
`HEGEL_TEST_CASES=5000` resolves the `ci` profile (1000 test cases,
derandomized, database disabled) and the environment variable overrides
`test_cases` to 5000. A test declared `#[hegel::test(test_cases = 200)]`
in the same run keeps its 200: the attribute is compiled in, and wins over
the variable. Locally, without the variable, the first test runs 100 cases
and the second 200, with the `development` profile's settings for
everything else.

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
| `ci` | `derandomize = true`, `database = "disabled"`, `suppress_health_check = ["too_slow"]` | On a CI server, detected from `CI`, `GITHUB_ACTIONS`, `GITLAB_CI`, `BUILDKITE`, `CIRCLECI`, and the variables other common services set. |
| `workload` | `backend = "urandom"`, `database = "disabled"`, `suppress_health_check = ["all"]` | Inside [Antithesis](https://antithesis.com/), detected from `ANTITHESIS_OUTPUT_DIR`. Antithesis's fuzzer controls `/dev/urandom`, so the `urandom` backend hands it every choice; and Antithesis pauses threads, which would trip wall-clock health checks such as `too_slow` spuriously. |

`print_blob` is on in the base settings, so a failing test prints a
`#[hegel::reproduce_failure("…")]` line.

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
| `HEGEL_TEST_CASES` | profile resolution | Sets `test_cases` over the resolved profile. |
| `HEGEL_DATABASE` | profile resolution | Sets `database` over the resolved profile. |
| `HEGEL_STATISTICS` | profile resolution | Turns `show_statistics` on over the resolved profile. |
| `HEGEL_SEED` | profile resolution | Sets `seed` over the resolved profile. |
| `HEGEL_DERANDOMIZE` | profile resolution | Sets `derandomize` over the resolved profile. |
| `HEGEL_PRINT_BLOB` | profile resolution | Sets `print_blob` over the resolved profile. |
| `HEGEL_NONDETERMINISM_STRICTNESS` | profile resolution | Sets `nondeterminism_strictness` over the resolved profile. |
| `HEGEL_STATEFUL_STEPS` | `Machine::run` and `Machine::run_concurrent` | Replaces every state machine's step count for the run. |
| `ANTITHESIS_OUTPUT_DIR` | environment detection | Selects the `workload` profile, and each test's verdict is reported to the `sdk.jsonl` inside it. Must name an existing directory. |
| `HEGEL_FUZZ_OUTPUT` | run start | Fuzzer client: the run executes exactly one test case and writes a JSON record of it to this path. See [Driving a test from a fuzzer](#driving-a-test-from-a-fuzzer). |
| `HEGEL_FUZZ_PREFIX` | run start | Fuzzer client: a file holding the choice sequence the one test case replays before drawing randomly. |
| `HEGEL_FUZZ_MISFIT` | run start | Fuzzer client: `random` (the default) or `simplest`, what replaces a prefix value that no longer fits its draw. |
| `HEGEL_FUZZ_REPRODUCE` | run start | Fuzzer client: a prefix file to replay like a database entry, shrinking (when `phases` includes `shrink`) and persisting the failure it reproduces. |
| `HEGEL_FUZZ_TAIL` | run start | Fuzzer client: `random` (the default) or `none`, whether the one test case draws randomly past its prefix or overruns there, with a misfit punned as the shrinker puns it. |
| `HEGEL_FUZZ_TRACE` | run start | Fuzzer client: a file every choice of the one test case is appended to as it is drawn, for recovering the sequence of a case that kills the process. |
| `HEGEL_FUZZ_SERVER` | run start | Fuzzer client: two named pipes, `requests,replies`, that turn the process into a fuzz server running one test case per request, forked at the test's first draw. |
| `HEGEL_FUZZ_COVERAGE` | run start | Fuzzer client: a file each test case's addition to the program's LLVM coverage counters is written to, one AFL-bucketed byte per counter. Needs a `-C instrument-coverage` build. |
| `HEGEL_FUZZ_RECORD` | run start | Fuzzer client: `full` (the default) or `compact`, which leaves the `choices` and `spans` arrays out of the record for a fuzzer that reads them from `realized_base64`. |
| `HEGEL_FUZZ_TEST` | run start | Fuzzer client: the database key of the test the `HEGEL_FUZZ_*` variables are for; every other test runs no test case. |
| `CI`, `GITHUB_ACTIONS`, … | environment detection | Selects the `ci` profile. |

# Driving a test from a fuzzer

An external fuzzer — one that chooses each test case from what earlier
ones did, rather than generating them independently — drives a test
program one execution at a time through the `HEGEL_FUZZ_*` environment
variables. They are read by the engine at run start, so they work for any
entry point: a `#[hegel::main]` binary, a `#[hegel::test]` under
`cargo test`, or a `Hegel` driver.

When `HEGEL_FUZZ_OUTPUT` names a file, the run executes exactly one test
case and writes a JSON record of it there. Database replay, the retry of
`assume`-rejected cases, nondeterminism replays and shrinking are all
skipped: the fuzzer wants one execution of one input, and it decides what
to do next. A failing case is still the run's failure — the report is
printed and the process exits as any failing run does — so the fuzzer can
tell a failure from a pass by the exit status, and the record's `status`
and `origin` say which failure it was.

The record is one JSON object:

- `engine_version`: the libhegel version that wrote the record. The choice
  sequence encoding is only stable within one version.
- `test`: the test's database key, as `HEGEL_FUZZ_TEST` names it.
- `status`: `valid`, `invalid` (an `assume` rejected the case),
  `interesting` (the property failed) or `overrun` (the case ran out of
  its choice budget).
- `origin`: for an interesting case, the failure's origin — the panic
  location — and otherwise `null`.
- `choices`: every choice the case made, in order, each with its `kind`
  (`integer`, `boolean`, `float`, `bytes`, `string` or `clone`), its
  `value`, the constraint it was drawn under (`min`, `max` and
  `shrink_towards` for an integer, `p` for a boolean, `min`, `max`,
  `allow_nan`, `allow_infinity` and `smallest_nonzero_magnitude` for a
  float, `min_size` and `max_size` for bytes and strings), and `forced`.
  Integers are decimal strings, bytes are hex, a float that JSON cannot
  represent is the string `NaN`, `inf` or `-inf`, and a lone surrogate in
  a string prints as U+FFFD (the encoded sequence keeps the exact value). A `clone` carries the
  cloned stream's `children` and `spans`.
- `realized_base64`: the case's realized form, its choices with the
  constraints they were drawn under and its spans, in the encoding the
  `fuzz-driver` feature of `hegeltest-c` reads back for seeding its
  shrinker.
- `choices_base64`: the same choice sequence in the failure database's
  entry format, base64-encoded — what to write to a file and pass back as
  `HEGEL_FUZZ_PREFIX`.
- `spans`: the span tree over the choices, each with its `label`, `start`
  and `end` choice indices, `depth`, `parent` span index and whether it was
  `discarded`. A state machine's steps and every generator's draws are
  spans, so a fuzzer can mutate a case at the boundaries that matter.
- `prefix_length`, `prefix_consumed` and `misaligned_at`: how much of the
  prefix the case used, and the first prefix position whose stored value
  the case did not replay, or `null`.
- `events` and `targets`: the case's `event` and `target` observations.
- `elapsed_ms`: how long the case took.

`HEGEL_FUZZ_PREFIX` names a file holding a choice sequence in the entry
format. The case replays it as a prefix and draws randomly past its end,
seeded as usual, so `HEGEL_SEED` makes a run reproducible from the prefix.
A stored value that does not fit the draw made at its position — the
prefix came from a different path through the test — is replaced by a
fresh random value, or by the simplest value fitting the draw when
`HEGEL_FUZZ_MISFIT=simplest`, and the prefix continues at the next
position either way.

`HEGEL_FUZZ_REPRODUCE` names a prefix file to run the ordinary way
instead of in fuzz mode: the sequence is replayed like an entry of the
failure database, and the failure it reproduces is shrunk, reported and
saved to the database, so the test's normal runs replay it from then on.
Nothing is generated; a run with no failure means the entry no longer
fails.

`HEGEL_FUZZ_TRACE` names a file the case appends every choice to as it
is drawn, in the entry format without its leading count, so a case that
aborts or is killed before the record is written still leaves its choice
sequence behind; a fuzzer runs a crashing prefix and seed once more with
the variable set to learn what the case drew. A cloned stream is traced
as an empty clone when it is opened.

`HEGEL_FUZZ_SERVER` names two named pipes, `requests,replies`, and
turns the process into a fuzz server. The selected test starts as usual
and, when its test case makes the first draw, reads requests from the
first pipe instead of drawing, one per line: tab-separated `key=value`
fields `output` (the record file, as `HEGEL_FUZZ_OUTPUT`), `prefix` (a
choice-sequence file, as `HEGEL_FUZZ_PREFIX`), `seed` (the seed for the
draws past the prefix), `tail` and `misfit` (as `HEGEL_FUZZ_TAIL` and
`HEGEL_FUZZ_MISFIT`) and `stderr` (a file the case's standard error goes
to). For each request the process forks: the child continues from that
draw exactly as a `HEGEL_FUZZ_OUTPUT` run would, writes its record and
exits, while the parent writes `pid <n>` to the reply pipe, waits for the
child, writes `exit <code>` or `signal <n>`, and reads the next request.
A request it cannot read or serve gets `error <message>` and no child.
End of file on the request pipe ends the server. The child ends its
process as soon as its run does, with status 0 or 101, rather than
returning to whatever started the test: under `cargo test` that is
libtest, whose channels do not survive a fork. Whatever the test does
before its first draw — loading the program, opening a database — is
done once and shared by every case, so a fuzzer whose executions are
dominated by process start-up gets them back. A test that never draws
cannot serve, and the run is a usage error.

`HEGEL_FUZZ_COVERAGE` names a file the program's LLVM coverage counters
are written to after every test case, for a fuzzer that wants the
case's coverage without parsing the raw profile the process writes at
exit. The counters are noted before each case and, once it has run,
what the case added to each is written, one byte per counter holding
the count bucketed as AFL buckets it: 0, 1, 2 and 3 kept, 4–7, 8–15,
16–31 and 32–127 becoming 4 to 7, and anything larger 8. The counters
themselves are left alone, so the profile the process writes at exit is
unaffected. The program must be built with
`-C instrument-coverage`, so the counters exist; setting the variable
for a program built without it is an error. Under `HEGEL_FUZZ_SERVER`
every served case writes the map.

`HEGEL_FUZZ_RECORD=compact` leaves the `choices` and `spans` arrays out
of the record. They are its bulk for a long case, and a fuzzer that
reads the choice sequence from `choices_base64` and the spans from
`realized_base64` has no use for them. The default, `full`, keeps them.

`HEGEL_FUZZ_TEST` names the database key of the test the variables are
for (`module_path::function_name`, the `test` field of the record). Every
other test in the process runs no test case, so a test binary holding
several tests can be driven one test at a time. Without it every test the
process runs is driven, and each overwrites the record.
