// A parenthesized #[invariant(...)] accepts only `always_run`.

struct Machine;

#[hegel::state_machine]
impl Machine {
    #[rule]
    fn act(&mut self, _: hegel::TestCase) {}

    #[invariant(sometimes)]
    fn check(&self, _: hegel::TestCase) {}
}

fn main() {}
