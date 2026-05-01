use super::*;

#[test]
fn test_protocol_debug_true_when_env_set() {
    // Set the env var BEFORE the LazyLock is first accessed in this binary.
    // No other test in the lib binary touches PROTOCOL_DEBUG, so this is the
    // first access and the closure evaluates with the env var present.
    // This exercises the "1" | "true" arm of the matches! macro.
    unsafe {
        std::env::set_var("HEGEL_PROTOCOL_DEBUG", "true");
    }
    assert!(*PROTOCOL_DEBUG);
    unsafe {
        std::env::remove_var("HEGEL_PROTOCOL_DEBUG");
    }
}
