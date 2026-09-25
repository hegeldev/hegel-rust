#![allow(dead_code)]

use hegel::TestCase;
use hegel::generators as gs;

struct IntegerStack {
    stack: Vec<i32>,
}

#[hegel::state_machine]
impl IntegerStack {
    #[rule]
    fn push(&mut self, tc: TestCase) {
        let integers = gs::integers::<i32>;
        let element = tc.draw(integers());
        self.stack.push(element);
    }

    #[rule]
    fn pop(&mut self, _: TestCase) {
        self.stack.pop();
    }

    #[rule]
    fn pop_push(&mut self, tc: TestCase) {
        let integers = gs::integers::<i32>;
        let element = tc.draw(integers());
        let initial = self.stack.clone();
        self.stack.push(element);
        let popped = self.stack.pop().unwrap();
        assert_eq!(popped, element);
        assert_eq!(self.stack, initial);
    }

    #[rule]
    fn push_pop(&mut self, tc: TestCase) {
        let initial = self.stack.clone();
        let element = self.stack.pop();
        tc.assume(element.is_some());
        let element = element.unwrap();
        self.stack.push(element);
        assert_eq!(self.stack, initial);
    }
}

#[hegel::test]
fn test_integer_stack(tc: TestCase) {
    let stack = IntegerStack { stack: Vec::new() };
    hegel::stateful::machine(stack).run(tc);
}

fn main() {}
