//! The derive macro's generated code must compile without the user importing
//! the Generator trait. Previously, `new()` called `.boxed()` (a Generator
//! trait method) without importing the trait, so it only compiled when users
//! happened to `use hegel::DefaultGenerator` (which brings both the derive
//! macro AND the trait into scope).

#[derive(Debug, hegel::DefaultGenerator)]
struct Person {
    name: String,
    age: i32,
}

fn main() {
    let _p: Option<Person> = None;
}
