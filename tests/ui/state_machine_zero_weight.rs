// A #[rule] weight must be finite and positive.

struct Machine;

#[hegel::state_machine]
impl Machine {
    #[rule(weight = 0.0)]
    fn act(&mut self, _: hegel::TestCase) {}
}

fn main() {}
