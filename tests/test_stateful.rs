mod common;

use common::project::TempRustProject;
use hegel::TestCase;
use hegel::generators as gs;
use hegel::stateful::{Variables, variables};

#[test]
fn test_state_machine_failure() {
    let code = r#"
use hegel::TestCase;

struct Linear {
    state: i32,
}

#[hegel::state_machine]
impl Linear {
    #[rule]
    fn zero(&mut self, tc: TestCase) {
        tc.assume(self.state == 0);
        self.state += 1;
    }

    #[rule]
    fn one(&mut self, tc: TestCase) {
        tc.assume(self.state == 1);
        self.state += 1;
    }

    #[rule]
    fn two(&mut self, tc: TestCase) {
        tc.assume(self.state == 2);
        self.state += 1;
    }

    #[rule]
    fn three(&mut self, tc: TestCase) {
        tc.assume(self.state == 3);
        self.state += 1;
    }

    #[invariant]
    fn upper_bound(&mut self, _tc: TestCase) {
        assert!(self.state < 4);
    }
}

#[hegel::test]
fn test_upper_bound(tc: TestCase) {
    let m = Linear { state: 0 };
    hegel::stateful::run(m, tc);
}

fn main() {}
"#;

    TempRustProject::new()
        .main_file(code)
        .expect_failure("assertion failed: self.state < 4")
        .cargo_test(&[]);
}

// Consuming an element from a set should mean subsequent draws never yield the element.
struct TestConsumeMachine {
    numbers: Variables<i32>,
    consumed: i32,
}

#[hegel::state_machine]
impl TestConsumeMachine {
    #[rule]
    fn draw(&mut self, _tc: TestCase) {
        let x = self.numbers.draw();
        assert!(*x != self.consumed);
    }
}

#[hegel::test]
fn test_consume(tc: TestCase) {
    let ints = gs::integers::<i32>;
    let elements = tc.draw(gs::vecs(ints()).unique(true));
    tc.assume(!elements.is_empty());
    let mut bundle = variables(&tc);
    for element in elements.clone() {
        bundle.add(element);
    }
    let consumed = bundle.consume();
    let m = TestConsumeMachine {
        numbers: bundle,
        consumed,
    };
    hegel::stateful::run(m, tc);
}

// That `#[hegel::state_machine]` correctly propagates `#[cfg(...)]` attributes
// to the items it synthesises (so an inactive cfg strips them before
// compile_error! can fire) is asserted by
// tests/compile/pass/stateful_cfg_attributes_are_copied_to_rules.rs.

struct TestLifetimeMachine<'a> {
    data: &'a [i32],
}

#[hegel::state_machine]
impl<'a> TestLifetimeMachine<'a> {
    #[rule]
    fn f(&mut self, _tc: TestCase) {
        assert!(!self.data.is_empty());
    }
}

#[hegel::test]
fn test_state_machine_with_lifetime(tc: TestCase) {
    let data = vec![1, 2, 3];
    let m = TestLifetimeMachine { data: &data };
    hegel::stateful::run(m, tc);
}

struct GenericMachine<T> {
    values: Vec<T>,
}

#[hegel::state_machine]
impl<T: std::fmt::Debug> GenericMachine<T> {
    #[rule]
    fn check(&mut self, _tc: TestCase) {
        let _ = self.values.len();
    }
}

#[hegel::test]
fn test_state_machine_with_type_parameter(tc: TestCase) {
    let m = GenericMachine {
        values: vec![1, 2, 3],
    };
    hegel::stateful::run(m, tc);
}

// Drawing an element from a bundle should always yield an element that was previously added.
struct TestDrawDomainMachine {
    domain: Vec<i32>,
    variables: Variables<i32>,
}

#[hegel::state_machine]
impl TestDrawDomainMachine {
    #[rule]
    fn draw(&mut self, _tc: TestCase) {
        let x = self.variables.draw();
        assert!(self.domain.contains(x));
    }
}

#[hegel::test]
fn test_draw_domain(tc: TestCase) {
    let ints = gs::integers::<i32>;
    let elements = tc.draw(gs::vecs(ints()));
    tc.assume(!elements.is_empty());
    let mut bundle = variables(&tc);
    for element in elements.clone() {
        bundle.add(element);
    }
    let m = TestDrawDomainMachine {
        domain: elements,
        variables: bundle,
    };
    hegel::stateful::run(m, tc);
}
