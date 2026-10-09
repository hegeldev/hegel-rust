//! Embedded tests for `src/llvm_coverage.rs`.

use super::*;

#[test]
fn buckets_follow_afl() {
    assert_eq!(bucket(0), 0);
    assert_eq!(bucket(3), 3);
    assert_eq!(bucket(4), 4);
    assert_eq!(bucket(7), 4);
    assert_eq!(bucket(8), 5);
    assert_eq!(bucket(31), 6);
    assert_eq!(bucket(127), 7);
    assert_eq!(bucket(128), 8);
    assert_eq!(bucket(u64::MAX), 8);
}

#[cfg(hegel_coverage)]
#[test]
fn a_sink_writes_one_bucket_per_counter_of_what_ran_since_begin() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("map");
    let mut sink = Sink::new(path.clone());
    sink.begin();
    for n in 0..200u64 {
        std::hint::black_box(bucket(n));
    }
    sink.write();
    let map = std::fs::read(&path).unwrap();
    assert!(map.iter().all(|&b| b <= 8), "{map:?}");
    assert!(map.contains(&8), "a counter run 200 times buckets to 8");
    sink.begin();
    sink.write();
    assert_eq!(std::fs::read(&path).unwrap().len(), map.len());
}

#[cfg(not(hegel_coverage))]
#[test]
fn without_instrumentation_there_is_no_sink() {
    assert!(Sink::from_env().is_none());
}

#[cfg(hegel_coverage)]
#[test]
#[should_panic(expected = "HEGEL_FUZZ_COVERAGE: cannot write")]
fn an_unwritable_map_is_an_error() {
    let dir = tempfile::TempDir::new().unwrap();
    let mut sink = Sink::new(dir.path().join("missing").join("map"));
    sink.begin();
    sink.write();
}
