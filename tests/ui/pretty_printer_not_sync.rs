fn requires_sync<T: Sync>() {}

fn main() {
    requires_sync::<hegel::PrettyPrinter>();
    requires_sync::<hegel::TestCase>();
}
