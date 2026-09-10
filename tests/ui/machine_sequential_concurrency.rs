// Concurrency bounds exist only for concurrent state machines: a sequential
// machine has no `min_concurrency` / `max_concurrency` builder methods.

struct Counter {
    value: i64,
}

#[hegel::state_machine]
impl Counter {
    #[rule]
    fn increment(&mut self, _: hegel::TestCase) {
        self.value += 1;
    }
}

#[hegel::test]
fn test_counter(tc: hegel::TestCase) {
    hegel::stateful::machine(Counter { value: 0 })
        .max_concurrency(3)
        .run(tc);
}

fn main() {}
