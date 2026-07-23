// A parenthesized #[rule()] with no arguments is rejected: either write a
// bare #[rule] for the anonymous group or name a group.

struct Machine;

#[hegel::concurrent_state_machine]
impl Machine {
    #[rule()]
    fn act(&self, _: hegel::TestCase) {}
}

fn main() {}
