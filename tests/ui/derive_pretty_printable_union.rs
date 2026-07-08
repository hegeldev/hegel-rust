// `PrettyPrintable` cannot be derived for unions: there is no safe way to
// know which field is active.

#[derive(hegel::PrettyPrintable)]
union Bits {
    int: i32,
    float: f32,
}

fn main() {}
