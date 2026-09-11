// A concurrent state machine's rules run against a model shared by
// reference across worker threads, so `&mut self` receivers are rejected.

struct Machine {
    value: i64,
}

#[hegel::concurrent_state_machine]
impl Machine {
    #[rule]
    fn mutate(&mut self, _: hegel::TestCase) {
        self.value += 1;
    }
}

fn main() {}
