// A parenthesized #[rule()] with no arguments is rejected: write
// #[rule] instead.

struct Machine;

#[hegel::state_machine]
impl Machine {
    #[rule()]
    fn act(&mut self, _: hegel::TestCase) {}
}

fn main() {}
