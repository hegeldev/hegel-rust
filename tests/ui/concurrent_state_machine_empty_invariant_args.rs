// A parenthesized #[invariant()] with no arguments is rejected: either
// write a bare #[invariant] for a sampled invariant or pass `always_run`.

struct Machine;

#[hegel::concurrent_state_machine]
impl Machine {
    #[rule]
    fn act(&self, _: hegel::TestCase) {}

    #[invariant()]
    fn check(&self, _: hegel::TestCase) {}
}

fn main() {}
