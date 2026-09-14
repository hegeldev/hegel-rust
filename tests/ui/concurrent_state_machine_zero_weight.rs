// A #[rule] weight must be finite and positive, in a concurrent machine too.

struct Machine;

#[hegel::concurrent_state_machine]
impl Machine {
    #[rule(group = "rw", weight = 0)]
    fn act(&self, _: hegel::TestCase) {}
}

fn main() {}
