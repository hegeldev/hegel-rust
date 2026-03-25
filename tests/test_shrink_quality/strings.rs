use crate::common::utils::minimal;
use hegel::generators;

#[test]
fn test_minimize_string_to_empty() {
    assert_eq!(minimal(generators::text(), |_| true), "");
}

#[test]
fn test_minimize_longer_string() {
    // Note: use chars().count() not len(), since len() counts bytes in Rust.
    let result = minimal(generators::text(), |x: &String| x.chars().count() >= 10);
    assert_eq!(result, "0".repeat(10));
}

#[test]
fn test_minimize_longer_list_of_strings() {
    assert_eq!(
        minimal(generators::vecs(generators::text()), |x: &Vec<String>| {
            x.len() >= 10
        }),
        vec![""; 10]
    );
}
