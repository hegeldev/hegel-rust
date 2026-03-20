use hegel::TestCase;
use hegel::generators::integers;
use hegel::stateful::{Variables, variables};
use std::cmp::min;

struct DieHard {
    small: i32,
    big: i32,
}

#[hegel::state_machine]
impl DieHard {
    #[rule]
    fn fill_small(&mut self, _tc: &TestCase) {
        self.small = 3;
    }

    #[rule]
    fn fill_big(&mut self, _tc: &TestCase) {
        self.big = 5;
    }

    #[rule]
    fn empty_small(&mut self, _tc: &TestCase) {
        self.small = 0;
    }

    #[rule]
    fn empty_big(&mut self, _tc: &TestCase) {
        self.big = 0;
    }

    #[rule]
    fn pour_small_into_big(&mut self, _tc: &TestCase) {
        let big = self.big;
        self.big = min(5, self.big + self.small);
        self.small -= self.big - big;
    }

    #[rule]
    fn pour_big_into_small(&mut self, _tc: &TestCase) {
        let small = self.small;
        self.small = min(3, self.small + self.big);
        self.big -= self.small - small;
    }

    #[invariant]
    fn physics_of_jugs(&self, _tc: &TestCase) {
        assert!(0 <= self.small && self.small <= 3);
        assert!(0 <= self.big && self.big <= 5);
    }

    #[invariant]
    fn die_hard_problem_not_solved(&self, tc: &TestCase) {
        tc.note(&format!("small / big = {0} / {1}", self.small, self.big));
        assert!(self.big != 4);
    }
}

#[hegel::test(test_cases = 1)]
fn test_die_hard(tc: TestCase) {
    let m = DieHard { small: 0, big: 0 };
    hegel::stateful::run(m, tc);
}

struct VariableMachine {
    numbers: Variables<i32>,
}

#[hegel::state_machine]
impl VariableMachine {
    #[rule]
    fn generate(&mut self, tc: &TestCase) {
        let i = tc.draw(integers::<i32>());
        self.numbers.add(i);
        assert!(!self.numbers.empty());
    }

    #[rule]
    fn add(&mut self, _tc: &TestCase) {
        let _ = self.numbers.draw();
    }
}

#[hegel::test(test_cases = 1000)]
fn test_variables(tc: TestCase) {
    let m = VariableMachine {
        numbers: variables(&tc),
    };
    hegel::stateful::run(m, tc);
}
