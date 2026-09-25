// A sequential state machine has no concurrency groups, so `group = "..."`
// is not a valid #[rule] argument there.

struct Machine;

#[hegel::state_machine]
impl Machine {
    #[rule(group = "rw")]
    fn act(&mut self, _: hegel::TestCase) {}
}

fn main() {}
