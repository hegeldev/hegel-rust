use hegel::PrettyPrintable;

#[derive(PrettyPrintable)]
struct Config {
    #[pretty(display)]
    name: String,
}

fn main() {}
