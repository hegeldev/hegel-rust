use crate::TestCase;
use crate::cbor_utils::cbor_map;
use crate::generators::{integers, sampled_from};
use crate::test_case::{ASSUME_FAIL_STRING, STOP_TEST_STRING};
use ciborium::Value;
use std::cmp::min;
use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};

pub type Rule = Box<dyn FnMut()>;

pub struct Variables<T> {
    pool_id: i128,
    tc: TestCase,
    values: HashMap<i128, T>,
}

impl<T> Variables<T> {
    fn pool_generate(&self, consume: bool) -> i128 {
        match self.tc.send_request(
            "pool_generate",
            &cbor_map! {
                "pool_id" => self.pool_id,
                "consume" => consume,
            },
        ) {
            Ok(Value::Integer(i)) => i.into(),
            Err(_) => {
                panic!("{}", STOP_TEST_STRING);
            }
            Ok(other) => panic!("Expected integer response for variable id, got {:?}", other),
        }
    }

    pub fn empty(&self) -> bool {
        self.values.is_empty()
    }

    pub fn add(&mut self, v: T) {
        let variable_id: i128 = match self
            .tc
            .send_request("pool_add", &cbor_map! {"pool_id" => self.pool_id})
        {
            Ok(Value::Integer(i)) => i.into(),
            Err(_) => {
                panic!("{}", STOP_TEST_STRING);
            }
            Ok(other) => panic!("Expected integer response for variable id, got {:?}", other),
        };
        if self.values.contains_key(&variable_id) {
            panic!("unexpected variable id in map");
        }
        self.values.insert(variable_id, v);
    }

    pub fn draw(&self) -> &T {
        self.tc.assume(!self.empty());
        let variable_id = self.pool_generate(false);
        self.values.get(&variable_id).unwrap()
    }

    pub fn consume(&mut self) -> T {
        self.tc.assume(!self.empty());
        let variable_id = self.pool_generate(true);
        self.values.remove(&variable_id).unwrap()
    }
}

pub fn variables<T>(tc: &TestCase) -> Variables<T> {
    let pool_id = match tc.send_request("new_pool", &cbor_map! {}) {
        Ok(Value::Integer(i)) => i.into(),
        Err(_) => {
            panic!("{}", STOP_TEST_STRING);
        }
        Ok(other) => panic!("Expected integer response for pool id, got {:?}", other),
    };
    Variables {
        pool_id,
        tc: tc.clone(),
        values: HashMap::new(),
    }
}

pub trait StateMachine {
    fn rules(&self) -> Vec<fn(&mut Self, &TestCase)>;
    fn invariants(&self) -> Vec<fn(&Self, &TestCase)>;
}

// TODO: factor out (shared with runner.rs)
fn panic_message(payload: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        s.to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "Unknown panic".to_string()
    }
}

fn check_invariants(m: &impl StateMachine, tc: &TestCase) {
    let invariants = m.invariants();
    for invariant in invariants {
        invariant(m, tc);
    }
}

pub fn run(mut m: impl StateMachine, tc: TestCase) {
    let rules = m.rules();
    if rules.is_empty() {
        panic!("Cannot run a machine with no rules.");
    }

    let rules = &sampled_from(rules);

    tc.note("Initial invariant check.");
    check_invariants(&m, &tc);

    // We generate an unbounded integer as the step cap that hypothesis actually sees. This means
    // we almost always run the maximum amount of steps, but allows us the possibility of shrinking
    // to a smaller number of steps.
    let max_steps = 50;
    let unbounded_step_cap = tc.draw(integers::<i64>().min_value(1));
    let step_cap = min(unbounded_step_cap, max_steps);

    let mut steps_run_successfully = 0;
    let mut steps_attempted = 0;

    // TODO: compare with the condition in the reference SDK
    while steps_run_successfully < step_cap && steps_attempted < 10 * step_cap {
        let rule = tc.draw(rules);

        // We only need this because AssertUnwindSafe expects a closure.
        let thunk = || rule(&mut m, &tc);
        let result = catch_unwind(AssertUnwindSafe(thunk));

        steps_attempted += 1;
        match result {
            Ok(()) => {
                steps_run_successfully += 1;
                check_invariants(&m, &tc);
            }
            Err(e) => {
                let msg = panic_message(&e);
                if msg == STOP_TEST_STRING {
                    // Server ran out of data — this test case is done.
                    break;
                } else if msg != ASSUME_FAIL_STRING {
                    resume_unwind(e);
                }
            }
        };
    }
}
