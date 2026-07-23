// A parenthesized #[rule(...)] on a concurrent state machine accepts only
// `group = "..."`.

struct Machine;

#[hegel::concurrent_state_machine]
impl Machine {
    #[rule(grp = "rw")]
    fn act(&self, _: hegel::TestCase) {}
}

fn main() {}
