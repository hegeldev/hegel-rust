//! Ported from nocover/test_completion.py

use hegel::Hegel;

#[test]
fn test_never_draw_anything() {
    Hegel::new(|_tc| {}).run();
}
