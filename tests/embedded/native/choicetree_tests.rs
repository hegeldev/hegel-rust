use super::*;

#[test]
fn default_same_as_new() {
    // ChoiceTree::default() must behave identically to ChoiceTree::new().
    let tree1 = ChoiceTree::default();
    let tree2 = ChoiceTree::new();
    // Both should be non-exhausted initially.
    assert!(!tree1.exhausted());
    assert!(!tree2.exhausted());
}
