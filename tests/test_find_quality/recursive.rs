//! Shapes inside recursive generators must turn up within an ordinary
//! search budget. A deferred JSON definition recurses through lists and
//! maps, so its cases nest same-label spans and the engine's span mutation
//! has every opportunity to crowd out fresh generation; the target shape
//! here needs a map, an empty key and a list value to coincide.

use hegel::generators::{self as gs, Generator};
use hegel::{HealthCheck, Hegel, Settings};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
#[allow(dead_code)]
enum Json {
    Null,
    Float(f64),
    Text(String),
    List(Vec<Json>),
    Map(BTreeMap<String, Json>),
}

fn json() -> impl Generator<Json> + Clone + 'static {
    let json = gs::deferred::<Json>();
    let g = json.generator();
    json.set(gs::one_of(vec![
        gs::just(Json::Null).boxed(),
        gs::floats::<f64>().map(Json::Float).boxed(),
        gs::text().map(Json::Text).boxed(),
        gs::vecs(g.clone()).map(Json::List).boxed(),
        gs::btree_maps(gs::text(), g.clone()).map(Json::Map).boxed(),
    ]));
    g
}

/// Whether a seeded search of `budget` cases over `json()` generates a map
/// whose `""` key holds a list.
fn finds_map_with_empty_key_holding_a_list(seed: u64, budget: u64) -> bool {
    let found = Arc::new(Mutex::new(false));
    let sink = Arc::clone(&found);
    let g = json();
    Hegel::new(move |tc| {
        let value = tc.draw_silent(&g);
        if matches!(&value, Json::Map(m) if matches!(m.get(""), Some(Json::List(_)))) {
            *sink.lock().unwrap() = true;
        }
    })
    .settings(
        Settings::new()
            .test_cases(budget)
            .seed(Some(seed))
            .database(None)
            .suppress_health_check(HealthCheck::all()),
    )
    .run();
    *found.lock().unwrap()
}

#[test]
fn test_deferred_json_reaches_a_map_with_an_empty_key_holding_a_list() {
    let found = (0..20u64)
        .filter(|&seed| finds_map_with_empty_key_holding_a_list(seed, 1000))
        .count();
    assert!(
        found >= 15,
        "found in {found} of 20 seeded searches of 1000 cases"
    );
}
