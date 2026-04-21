//! Ported from hypothesis-python/tests/cover/test_find.py
//!
//! Python's `find()` and `phases` setting have no public hegel-rust
//! counterparts. The original test pins down that `find(..., random=Random(13))`
//! is deterministic across runs — we express the same property by driving
//! `Hegel::new(...)` with `seed(Some(13))` and recording the first value
//! that matches the predicate.

use hegel::generators as gs;
use hegel::{Hegel, Settings};
use std::panic::AssertUnwindSafe;
use std::sync::{Arc, Mutex};

#[test]
fn test_find_uses_provided_seed() {
    let mut prev: Option<String> = None;

    for _ in 0..3 {
        let found: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        let found_clone = Arc::clone(&found);

        std::panic::catch_unwind(AssertUnwindSafe(|| {
            Hegel::new(move |tc| {
                let v: String = tc.draw(gs::text());
                if v.chars().count() > 5 {
                    let mut g = found_clone.lock().unwrap();
                    if g.is_none() {
                        *g = Some(v);
                    }
                    drop(g);
                    panic!("HEGEL_FOUND");
                }
            })
            .settings(
                Settings::new()
                    .test_cases(1000)
                    .database(None)
                    .seed(Some(13)),
            )
            .run();
        }))
        .ok();

        let value = found.lock().unwrap().take().unwrap();

        if let Some(ref p) = prev {
            assert_eq!(p, &value);
        } else {
            prev = Some(value);
        }
    }
}
