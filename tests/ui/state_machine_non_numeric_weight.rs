// A #[rule] weight must be a number literal.

struct Machine;

#[hegel::state_machine]
impl Machine {
    #[rule(weight = "heavy")]
    fn act(&mut self, _: hegel::TestCase) {}
}

fn main() {}
