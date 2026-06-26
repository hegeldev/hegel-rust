#![allow(dead_code)]

use hegel::TestCase;
use hegel::generators as gs;
use std::cmp::Ord;
use std::marker::Copy;

struct MinStack<T> {
    stack: Vec<T>,
    minimums: Vec<T>,
}

impl<T: Copy + Ord> MinStack<T> {
    fn new() -> Self {
        MinStack {
            stack: Vec::new(),
            minimums: Vec::new(),
        }
    }

    fn push(&mut self, element: T) {
        self.stack.push(element);
        if self.minimums.is_empty() || self.minimum().is_some_and(|m| element < m) {
            self.minimums.push(element);
        }
    }

    fn pop(&mut self) -> Option<T> {
        let element = self.stack.pop();
        if element.is_some_and(|e| self.minimum().unwrap() == e) {
            self.minimums.pop();
        }
        element
    }

    fn minimum(&self) -> Option<T> {
        let length = self.minimums.len();
        if length > 0 {
            Some(self.minimums[length - 1])
        } else {
            None
        }
    }

    fn reference(&self) -> Option<T> {
        let mut minimum = None;
        for element in &self.stack {
            if minimum.is_none_or(|m| m > *element) {
                minimum = Some(*element);
            }
        }
        minimum
    }
}

struct MinStackTest {
    stack: MinStack<i32>,
}

#[hegel::state_machine]
impl MinStackTest {
    #[rule]
    fn push(&mut self, tc: TestCase) {
        let element = tc.draw(gs::integers::<i32>());
        self.stack.push(element);
    }

    #[rule]
    fn pop(&mut self, tc: TestCase) {
        let element = self.stack.pop();
        match element {
            Some(element) => {
                tc.note(&format!("pop {}", element));
            }
            _ => {
                tc.note("pop nothing");
            }
        }
    }

    #[invariant]
    fn minimums_agree(&mut self, _: TestCase) {
        assert_eq!(self.stack.minimum(), self.stack.reference());
    }
}

#[hegel::test]
fn test_min_stack(tc: TestCase) {
    let test = MinStackTest {
        stack: MinStack::new(),
    };
    hegel::stateful::run(test, tc);
}

fn main() {}
