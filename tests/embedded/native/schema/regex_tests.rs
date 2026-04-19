use super::*;

#[test]
fn translate_no_backslash() {
    assert_eq!(translate_python_escapes("abc"), "abc");
}

#[test]
fn translate_z_anchor() {
    assert_eq!(translate_python_escapes(r"\Z"), r"\z");
}

#[test]
fn translate_z_in_pattern() {
    assert_eq!(translate_python_escapes(r"\A.\Z"), r"\A.\z");
}

#[test]
fn translate_escaped_backslash_before_z() {
    // \\Z is a literal backslash in regex followed by 'Z', not an anchor.
    assert_eq!(translate_python_escapes(r"\\Z"), r"\\Z");
}

#[test]
fn translate_other_escapes_unchanged() {
    assert_eq!(translate_python_escapes(r"\A\d\w"), r"\A\d\w");
}

#[test]
fn translate_trailing_backslash() {
    assert_eq!(translate_python_escapes("a\\"), "a\\");
}
