---
name: porting-stateful
description: "Port Python stateful (rule-based) tests from `hypothesis.stateful` to hegel-rust's `hegel::stateful`. Use when the upstream file defines a `RuleBasedStateMachine` (or a test function that calls `run_state_machine_as_test`)."
---

# Porting stateful tests to hegel-rust

The general porting workflow lives in
`.claude/skills/porting-tests/SKILL.md` — file layout, `main.rs` wiring,
naming conventions, the skip-vs-port policy, and the verification step all
apply here too. This skill covers only what is specific to stateful tests.

## What "stateful" means

An upstream file is a stateful port if the tests use any of:

- `from hypothesis.stateful import RuleBasedStateMachine, rule, ...`
- Subclasses of `RuleBasedStateMachine`
- `@rule`, `@invariant`, `@initialize`, `@precondition`, `@consumes`
- `Bundle(...)`, `multiple(...)`, `VarReference`
- `run_state_machine_as_test(...)`

Files mixing stateful and non-stateful tests are ported as one file; just
use both skills as needed.

## Core API mapping

| Hypothesis                                       | hegel-rust                                                                 |
| ------------------------------------------------ | -------------------------------------------------------------------------- |
| `class Foo(RuleBasedStateMachine):`              | `struct Foo { … }` + `#[hegel::state_machine] impl Foo { … }`              |
| `@rule()`                                        | `#[rule] fn …(&mut self, tc: TestCase) { … }`                              |
| `@rule(x=strategy)`                              | `#[rule] fn …(&mut self, tc: TestCase) { let x = tc.draw(strategy); … }`   |
| `@invariant()`                                   | `#[invariant] fn …(&mut self, _tc: TestCase) { … }`                        |
| `@precondition(lambda self: cond)`               | `tc.assume(cond);` at the top of the rule body                             |
| `@initialize()`                                  | Run the initialization code in the hegel test body, before `run(m, tc)`    |
| `Bundle("name")` + `@rule(target=b)`             | `b: Variables<T>` struct field; inside the rule, `self.b.add(value);`      |
| `@rule(x=bundle)`                                | `let x = self.bundle.draw();` (returns `&T`)                               |
| `@rule(x=consumes(bundle))`                      | `let x = self.bundle.consume();` (returns `T`)                             |
| `multiple(*args)`                                | Loop `for v in args { self.bundle.add(v); }`                               |
| `teardown(self)`                                 | Run teardown code in the hegel test body, after `run(m, tc)`               |
| `run_state_machine_as_test(Foo)`                 | `let m = Foo::new(); hegel::stateful::run(m, tc);` inside `#[hegel::test]` |
| `len(self.bundle("b"))`                          | `self.b.len()`                                                             |
| `TestCase` for `@given`-like draws inside a rule | `tc.draw(strategy)` — hegel rules always receive `tc: TestCase`            |

### Imports

Every ported stateful file should start with (adjust to what it uses):

```rust
use hegel::TestCase;
use hegel::generators::{self as gs, Generator};
use hegel::stateful::{Variables, variables};
```

### What `#[hegel::state_machine]` accepts

Only `#[rule]` and `#[invariant]` are recognised attributes inside the impl
block. There is currently no `#[precondition]`, no `#[initialize]`, and no
way to declare that a rule's return value feeds a bundle — those all map to
imperative code in the rule body or the test body (see the table above).

### Rule / invariant signatures

- Rule: `fn name(&mut self, tc: TestCase) { … }`
- Invariant: `fn name(&mut self, _tc: TestCase) { … }` — invariants take
  `tc` by convention even though they rarely need it; use `_tc` when unused.

Rules return `()`. A Hypothesis rule that uses `return value` to push into a
bundle becomes `self.bundle.add(value);` followed by an implicit return.

### Running the machine

The `#[hegel::state_machine]` macro implements `hegel::stateful::StateMachine`
for the struct. The entry point is:

```rust
#[hegel::test]
fn test_foo(tc: TestCase) {
    // (1) @initialize body goes here, if any
    let mut m = Foo { /* fields */ };
    m.some_init();

    // (2) pre-populate Variables bundles here, if any need seeding
    // (Bundles can instead be populated by rules during the run.)

    hegel::stateful::run(m, tc);

    // (3) teardown() body goes here, if any
}
```

## Worked mappings

### Basic `@rule` + `@invariant`

Upstream:

```python
class Counter(RuleBasedStateMachine):
    def __init__(self):
        super().__init__()
        self.n = 0

    @rule()
    def inc(self):
        self.n += 1

    @invariant()
    def non_negative(self):
        assert self.n >= 0
```

Port:

```rust
struct Counter { n: i64 }

#[hegel::state_machine]
impl Counter {
    #[rule]
    fn inc(&mut self, _tc: TestCase) { self.n += 1; }

    #[invariant]
    fn non_negative(&mut self, _tc: TestCase) { assert!(self.n >= 0); }
}

#[hegel::test]
fn test_counter(tc: TestCase) {
    hegel::stateful::run(Counter { n: 0 }, tc);
}
```

### `@rule(x=strategy)`

Upstream:

```python
@rule(x=integers())
def add(self, x):
    self.total += x
```

Port:

```rust
#[rule]
fn add(&mut self, tc: TestCase) {
    let x = tc.draw(gs::integers::<i64>());
    self.total += x;
}
```

### `@precondition`

Upstream:

```python
@precondition(lambda self: self.n > 0)
@rule()
def dec(self):
    self.n -= 1
```

Port (hegel's `tc.assume` has engine-level support — Hypothesis's
precondition is more efficient, but the two are semantically equivalent
for porting purposes):

```rust
#[rule]
fn dec(&mut self, tc: TestCase) {
    tc.assume(self.n > 0);
    self.n -= 1;
}
```

### `@initialize`

hegel-rust has no `#[initialize]`. Run the setup in the test body before
calling `run`:

```python
@initialize()
def setup(self):
    self.open_conn()
```

```rust
#[hegel::test]
fn test_thing(tc: TestCase) {
    let mut m = Thing::default();
    m.open_conn();
    hegel::stateful::run(m, tc);
}
```

If the initialization draws values from strategies (`@initialize(x=integers())`),
do the draw in the test body with `tc.draw(...)` before building `m`.

### Bundles — `target=` with `return`

Upstream:

```python
class M(RuleBasedStateMachine):
    nodes = Bundle("nodes")

    @rule(target=nodes)
    def make(self):
        return new_node()

    @rule(n=nodes)
    def use(self, n):
        n.frob()
```

Port:

```rust
struct M { nodes: Variables<Node> }

#[hegel::state_machine]
impl M {
    #[rule]
    fn make(&mut self, _tc: TestCase) {
        self.nodes.add(new_node());
    }

    #[rule]
    fn use_node(&mut self, _tc: TestCase) {
        // `draw()` calls `tc.assume(!empty)` itself, so rules that need a
        // bundle value can just call it unconditionally.
        let n = self.nodes.draw();
        n.frob();
    }
}

#[hegel::test]
fn test_m(tc: TestCase) {
    let m = M { nodes: variables(&tc) };
    hegel::stateful::run(m, tc);
}
```

`variables(&tc)` creates an empty pool tied to the test case. Every bundle
becomes its own struct field constructed this way.

### `consumes`

Upstream:

```python
@rule(target=b2, x=consumes(b1))
def move_(self, x):
    return x
```

Port:

```rust
#[rule]
fn move_(&mut self, _tc: TestCase) {
    let x = self.b1.consume();
    self.b2.add(x);
}
```

### `multiple(*args)`

Upstream:

```python
@rule(target=b, items=lists(integers(), max_size=10))
def populate(self, items):
    return multiple(*items)
```

Port:

```rust
#[rule]
fn populate(&mut self, tc: TestCase) {
    let items = tc.draw(gs::vecs(gs::integers::<i64>()).max_size(10));
    for v in items { self.b.add(v); }
}
```

`multiple()` with no args is a no-op — simply don't add anything to the
bundle.

### `teardown()`

Upstream:

```python
def teardown(self):
    self.conn.close()
```

Port — run inline in the test body after `run`:

```rust
#[hegel::test]
fn test_thing(tc: TestCase) {
    let mut m = Thing::default();
    hegel::stateful::run(m, tc);
    m.conn.close();
}
```

(Note: `hegel::stateful::run` moves `m`, so if you need `m` after the run,
build it before and pass by value to a wrapper, or restructure so cleanup
happens via `Drop`. In most cases teardown is doing something the `Drop`
impl would do anyway — prefer that.)

## What cannot be ported today

Two specific constructs have no hegel-rust counterpart. A test whose point
is to exercise one of these goes in `SKIPPED.md` with the reason.

1. **Strategies that wrap bundles**, e.g.
   `@rule(xs=lists(consumes(b1), max_size=3))`. hegel's `Variables<T>` is
   not a strategy and cannot be composed with `gs::vecs` / `gs::sets` etc.
   Workaround when feasible: draw a count with `tc.draw(gs::integers…)` and
   call `self.b1.consume()` in a loop. If the test's point is the
   *strategy composition itself*, skip.

2. **Name-based bundle introspection**, i.e. `self.bundle("some_name")`.
   hegel bundles are typed struct fields, not named strings. For tests that
   only need length, `self.some_name.len()` works; for tests that walk the
   values by name, skip.

## Skip-vs-port decisions for the stateful test file

Stateful upstream files contain three kinds of tests. Sort each test
individually:

### Port, adapting as needed

- **Flaky detection** tests (`test_flaky_raises_flaky`,
  `test_ratchetting_raises_flaky`, etc.). hegel-rust is expected to have
  equivalent flaky-detection behaviour. If the behaviour isn't there yet,
  port the test, let it fail at runtime, and the fixer loop will pick it up.
- **Database save-on-failure** tests
  (`test_saves_failing_example_in_database`). hegel-rust has a real
  database. Port and native-gate as for any database test; see
  `tests/test_database_key.rs`.
- **Falsifying-example print format** tests. The exact printed string
  differs between hypothesis and hegel-rust — update the expected string
  to the hegel-rust output. Don't pre-assume what the output will be; run
  the test and copy the actual failing-example output into the assertion.
- **`run_state_machine_as_test(Foo)`** — becomes `hegel::stateful::run(Foo::new(), tc)`
  inside a `#[hegel::test]`.

### Port, but with judgement

- Settings-manipulation tests (`Settings(stateful_step_count=5)` /
  `max_examples=…`). If there's a reasonable hegel-rust equivalent
  (`Settings::new().test_cases(…)`, the built-in 50-step cap in `run`),
  port. Otherwise, skip this specific test inside a ported file and
  leave a one-line `// TODO(port): …` in the module — the file is still
  ported, just one test is omitted with a visible reason.

### Skip

- Internal-contract tests that can't be expressed in hegel-rust:
  `check_during_init=True`, `test_empty_machine_is_invalid`,
  `test_stateful_double_rule_is_forbidden`, `test_no_double_invariant`,
  `FlakyPreconditionMachine` (no `@precondition` in hegel),
  `test_get_state_machine_test_is_importable` (not a public hegel API).
  Add these to `SKIPPED.md` on a per-test basis only if *every* test in
  the file is of this flavour; if only some are, port the file and drop
  those tests with a `// TODO(port):` note explaining why.

If the entire file is meta-tests on hypothesis internals with no public-API
content to port, add it to `SKIPPED.md` with the rationale.

## Destination

Follows the same rules as `porting-tests/SKILL.md`. Stateful test files
land under `tests/hypothesis/` as single modules — usually named after the
upstream file minus the `test_` prefix:

- `resources/hypothesis/hypothesis-python/tests/cover/test_stateful.py`
  → `tests/hypothesis/stateful.rs`
- `resources/hypothesis/hypothesis-python/tests/nocover/test_stateful.py`
  → `tests/hypothesis/nocover_stateful.rs`

Add `mod <name>;` to `tests/hypothesis/main.rs` in alphabetical order.

## Verification

In addition to the standard verification from `porting-tests/SKILL.md`
(server-mode compile, server-mode run, native-mode compile), for stateful
ports specifically confirm:

- Every state machine has at least one `#[rule]` (a rule-free machine
  panics at runtime).
- Bundle struct fields have type `Variables<T>` with the matching element
  type of the upstream `Bundle`.
- Every `#[hegel::test]` that uses bundles constructs them via
  `variables(&tc)` — a `Variables` created with a different `TestCase` will
  not work.

## Keep this skill current

If you find a stateful pattern in the upstream that isn't covered here,
add a row to the mapping table or a small subsection before moving on.
Keep entries terse and back them with a code sketch; don't restate rules
that already exist in `porting-tests/SKILL.md`.
