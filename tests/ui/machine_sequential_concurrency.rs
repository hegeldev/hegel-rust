// Concurrency bounds exist only for concurrent state machines: a sequential
// machine has no `min_concurrency` / `max_concurrency` builder methods.

use hegel::TestCase;
use hegel::stateful::machine;

fn main() {}

#[hegel::state_machine]
impl Counter {
    #[rule]
    fn increment(&mut self, _: TestCase) {
        self.value += 1;
    }
}

struct Counter {
    value: i64,
}

#[hegel::test]
fn test_counter(tc: TestCase) {
    machine(Counter { value: 0 }).max_concurrency(3).run(tc);
}
