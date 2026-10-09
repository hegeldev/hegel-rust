use super::*;
use crate::backend::{DataSource, TestCaseResult};
use crate::native::bignum::BigInt;
use crate::native::core::choices::{
    BooleanChoice, BytesChoice, FloatChoice, RealizedStream, StringChoice,
};
use crate::native::database::serialize_choices;
use crate::native::intervalsets::IntervalSet;
use crate::settings::{Database, Verbosity};
use alloc::sync::Arc;
use alloc::vec;
use std::sync::atomic::{AtomicUsize, Ordering};

fn env_of(vars: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> + use<> {
    let vars: Vec<(String, String)> = vars
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    move |key| vars.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone())
}

fn int(v: i64) -> ChoiceValue {
    ChoiceValue::Integer(BigInt::from(v))
}

fn write_entry(dir: &tempfile::TempDir, name: &str, choices: &[ChoiceValue]) -> String {
    let path = dir.path().join(name);
    std::fs::write(&path, serialize_choices(choices).unwrap()).unwrap();
    path.to_str().unwrap().to_string()
}

fn usage_message(err: RunError) -> String {
    match err {
        RunError::UsageError(message) => message,
        other => panic!("expected a usage error, got {other:?}"),
    }
}

#[test]
fn no_fuzz_variables_means_no_fuzz_mode() {
    assert!(from_env_with(|_| None, Some("k")).unwrap().is_none());
    let env = env_of(&[("HEGEL_FUZZ_OUTPUT", ""), ("HEGEL_FUZZ_REPRODUCE", "")]);
    assert!(from_env_with(env, Some("k")).unwrap().is_none());
}

#[test]
fn a_server_variable_names_the_two_pipes() {
    let env = env_of(&[("HEGEL_FUZZ_SERVER", "/req,/rep")]);
    match from_env_with(env, Some("k")).unwrap().unwrap() {
        FuzzMode::Server {
            requests,
            replies,
            compact,
        } => {
            assert_eq!(requests, "/req");
            assert_eq!(replies, "/rep");
            assert!(!compact);
        }
        _ => panic!("expected the server"),
    }
    let env = env_of(&[("HEGEL_FUZZ_SERVER", "/req")]);
    let message = usage_message(from_env_with(env, Some("k")).unwrap_err());
    assert!(message.contains("HEGEL_FUZZ_SERVER"), "{message}");
    let env = env_of(&[
        ("HEGEL_FUZZ_SERVER", "/req,/rep"),
        ("HEGEL_FUZZ_TEST", "other"),
    ]);
    assert!(matches!(
        from_env_with(env, Some("k")).unwrap(),
        Some(FuzzMode::Skip)
    ));
}

#[test]
fn an_output_path_selects_a_case_with_an_empty_prefix_and_random_misfits() {
    let env = env_of(&[("HEGEL_FUZZ_OUTPUT", "/some/record.json")]);
    match from_env_with(env, Some("k")).unwrap().unwrap() {
        FuzzMode::Case {
            prefix,
            output,
            random_misfits,
            exact,
            trace,
            compact,
        } => {
            assert!(prefix.is_empty());
            assert!(!compact);
            assert_eq!(output, "/some/record.json");
            assert!(random_misfits);
            assert!(!exact);
            assert_eq!(trace, None);
        }
        _ => panic!("expected a case"),
    }
}

#[test]
fn the_prefix_file_is_decoded_and_the_misfit_policy_parsed() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = write_entry(&dir, "prefix", &[int(3), ChoiceValue::Boolean(true)]);
    let env = env_of(&[
        ("HEGEL_FUZZ_OUTPUT", "out"),
        ("HEGEL_FUZZ_PREFIX", &path),
        ("HEGEL_FUZZ_MISFIT", "simplest"),
    ]);
    match from_env_with(env, Some("k")).unwrap().unwrap() {
        FuzzMode::Case {
            prefix,
            random_misfits,
            ..
        } => {
            assert_eq!(prefix, vec![int(3), ChoiceValue::Boolean(true)]);
            assert!(!random_misfits);
        }
        _ => panic!("expected a case"),
    }
    let env = env_of(&[
        ("HEGEL_FUZZ_OUTPUT", "out"),
        ("HEGEL_FUZZ_MISFIT", "random"),
    ]);
    assert!(matches!(
        from_env_with(env, Some("k")).unwrap(),
        Some(FuzzMode::Case {
            random_misfits: true,
            ..
        })
    ));
    let env = env_of(&[("HEGEL_FUZZ_OUTPUT", "out"), ("HEGEL_FUZZ_MISFIT", "weird")]);
    let message = usage_message(from_env_with(env, Some("k")).unwrap_err());
    assert!(message.contains("HEGEL_FUZZ_MISFIT"), "{message}");
    assert!(message.contains("weird"), "{message}");
}

#[test]
fn an_unreadable_or_corrupt_prefix_is_a_usage_error_naming_the_variable() {
    let dir = tempfile::TempDir::new().unwrap();
    let missing = dir.path().join("missing").to_str().unwrap().to_string();
    let env = env_of(&[
        ("HEGEL_FUZZ_OUTPUT", "out"),
        ("HEGEL_FUZZ_PREFIX", &missing),
    ]);
    let message = usage_message(from_env_with(env, Some("k")).unwrap_err());
    assert!(message.contains("HEGEL_FUZZ_PREFIX="), "{message}");
    assert!(message.contains("could not be read"), "{message}");

    let corrupt = dir.path().join("corrupt");
    std::fs::write(&corrupt, b"\xff\xff\xff\xff not an entry").unwrap();
    let corrupt = corrupt.to_str().unwrap().to_string();
    let env = env_of(&[("HEGEL_FUZZ_REPRODUCE", &corrupt)]);
    let message = usage_message(from_env_with(env, Some("k")).unwrap_err());
    assert!(message.contains("HEGEL_FUZZ_REPRODUCE="), "{message}");
    assert!(message.contains("not a choice sequence"), "{message}");
}

#[test]
fn a_test_selector_skips_every_other_test() {
    let env = env_of(&[
        ("HEGEL_FUZZ_OUTPUT", "out"),
        ("HEGEL_FUZZ_TEST", "crate::a"),
    ]);
    assert!(matches!(
        from_env_with(&env, Some("crate::b")).unwrap(),
        Some(FuzzMode::Skip)
    ));
    assert!(matches!(
        from_env_with(&env, None).unwrap(),
        Some(FuzzMode::Skip)
    ));
    assert!(matches!(
        from_env_with(&env, Some("crate::a")).unwrap(),
        Some(FuzzMode::Case { .. })
    ));
}

#[test]
fn reproduce_reads_the_entry_and_needs_a_database_key() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = write_entry(&dir, "entry", &[int(9)]);
    let env = env_of(&[("HEGEL_FUZZ_REPRODUCE", &path)]);
    match from_env_with(&env, Some("k")).unwrap().unwrap() {
        FuzzMode::Reproduce { entry } => {
            assert_eq!(entry, serialize_choices(&[int(9)]).unwrap());
        }
        _ => panic!("expected reproduce"),
    }
    let message = usage_message(from_env_with(&env, None).unwrap_err());
    assert!(message.contains("database key"), "{message}");
}

fn quiet_settings() -> Settings {
    let mut settings = Settings::new().verbosity(Verbosity::Quiet);
    settings.database = Database::Disabled;
    settings
}

fn drive_mode<F>(
    mode: FuzzMode,
    settings: &Settings,
    key: Option<&str>,
    mut body: F,
) -> Result<TestRunResult, RunError>
where
    F: FnMut(&dyn DataSource) -> TestCaseResult,
{
    let exchange = CaseExchange::new();
    crate::exchange::drive(&exchange, run(mode, settings, key, &exchange), |ds| {
        let result = body(&*ds);
        ds.mark_complete(&result);
    })
}

fn case(prefix: &[ChoiceValue], output: &std::path::Path) -> FuzzMode {
    FuzzMode::Case {
        prefix: prefix.to_vec(),
        output: output.to_str().unwrap().to_string(),
        random_misfits: true,
        exact: false,
        trace: None,
        compact: false,
    }
}

fn exact_case(prefix: &[ChoiceValue], output: &std::path::Path) -> FuzzMode {
    FuzzMode::Case {
        prefix: prefix.to_vec(),
        output: output.to_str().unwrap().to_string(),
        random_misfits: true,
        exact: true,
        trace: None,
        compact: false,
    }
}

fn traced_case(output: &std::path::Path, trace: &std::path::Path) -> FuzzMode {
    FuzzMode::Case {
        prefix: Vec::new(),
        output: output.to_str().unwrap().to_string(),
        random_misfits: true,
        exact: false,
        trace: Some(trace.to_str().unwrap().to_string()),
        compact: false,
    }
}

#[test]
fn the_tail_policy_is_parsed_and_defaults_to_random() {
    for (vars, expected) in [
        (vec![(OUTPUT_VAR, "/out")], false),
        (vec![(OUTPUT_VAR, "/out"), (TAIL_VAR, "random")], false),
        (vec![(OUTPUT_VAR, "/out"), (TAIL_VAR, "none")], true),
    ] {
        match from_env_with(env_of(&vars), None).unwrap() {
            Some(FuzzMode::Case { exact, .. }) => assert_eq!(exact, expected),
            other => panic!("{other:?}"),
        }
    }
    let err = from_env_with(env_of(&[(OUTPUT_VAR, "/out"), (TAIL_VAR, "some")]), None).unwrap_err();
    assert_eq!(
        usage_message(err),
        "HEGEL_FUZZ_TAIL must be random or none, got \"some\""
    );
}

#[test]
fn an_exact_case_overruns_past_its_prefix_and_puns_a_misfit() {
    let dir = tempfile::TempDir::new().unwrap();
    let out = dir.path().join("record.json");
    let seen = std::sync::Mutex::new(Vec::new());
    drive_mode(
        exact_case(&[int(7), int(500)], &out),
        &quiet_settings(),
        Some("k"),
        |ds| {
            let a = match draw_int(ds, 0, 100) {
                Ok(n) => n,
                Err(r) => return r,
            };
            let b = match draw_int(ds, 0, 100) {
                Ok(n) => n,
                Err(r) => return r,
            };
            seen.lock().unwrap().push((a, b));
            match draw_int(ds, 0, 100) {
                Ok(_) => TestCaseResult::Valid,
                Err(r) => r,
            }
        },
    )
    .unwrap();
    let record = read_record(&out);
    assert_eq!(record["status"], "overrun");
    assert_eq!(record["misaligned_at"], 1);
    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].0, 7);
    assert!((0..=100).contains(&seen[0].1));
}

#[test]
fn the_record_carries_the_realized_form_of_the_case() {
    let dir = tempfile::TempDir::new().unwrap();
    let out = dir.path().join("record.json");
    drive_mode(case(&[int(3)], &out), &quiet_settings(), Some("k"), |ds| {
        ds.start_span(5).unwrap();
        if let Err(r) = draw_int(ds, 0, 10) {
            return r;
        }
        ds.stop_span(false).unwrap();
        TestCaseResult::Valid
    })
    .unwrap();
    let record = read_record(&out);
    let bytes =
        crate::native::base64::base64_decode(record["realized_base64"].as_str().unwrap()).unwrap();
    let (nodes, spans) = crate::native::realized::deserialize_realized(&bytes).unwrap();
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0].value(), int(3));
    assert!(matches!(&nodes[0].data, ChoiceData::Integer(c, _) if c.max_value == BigInt::from(10)));
    assert_eq!(spans.len(), record["spans"].as_array().unwrap().len());
    assert!(spans.iter().any(|s| s.label == 5));
}

#[test]
fn a_trace_path_is_read_from_the_environment() {
    let mode = from_env_with(env_of(&[(OUTPUT_VAR, "/out"), (TRACE_VAR, "/trace")]), None)
        .unwrap()
        .unwrap();
    match mode {
        FuzzMode::Case { trace, .. } => assert_eq!(trace.as_deref(), Some("/trace")),
        other => panic!("{other:?}"),
    }
    let mode = from_env_with(env_of(&[(OUTPUT_VAR, "/out"), (TRACE_VAR, "")]), None)
        .unwrap()
        .unwrap();
    match mode {
        FuzzMode::Case { trace, .. } => assert_eq!(trace, None),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_trace_holds_every_choice_as_it_is_drawn_without_the_count() {
    let dir = tempfile::TempDir::new().unwrap();
    let out = dir.path().join("record.json");
    let trace = dir.path().join("trace");
    std::fs::write(&trace, b"stale").unwrap();
    let seen = std::sync::Mutex::new(Vec::new());
    drive_mode(traced_case(&out, &trace), &quiet_settings(), None, |ds| {
        let a = draw_int(ds, 0, 100).unwrap();
        seen.lock()
            .unwrap()
            .push(std::fs::read(&trace).unwrap().len());
        ds.generate_boolean(0.5, None).unwrap();
        seen.lock()
            .unwrap()
            .push(std::fs::read(&trace).unwrap().len());
        let child = ds.clone_stream().unwrap();
        draw_int(&*child, 0, 100).unwrap();
        let _ = a;
        TestCaseResult::Valid
    })
    .unwrap();
    let record = read_record(&out);
    assert_eq!(record["choices"][2]["kind"], "clone");
    let expected = serialize_choices(&[
        int(record["choices"][0]["value"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap()),
        ChoiceValue::Boolean(record["choices"][1]["value"].as_bool().unwrap()),
    ])
    .unwrap();
    let traced = std::fs::read(&trace).unwrap();
    assert_eq!(traced[..traced.len() - 5], expected[4..]);
    assert_eq!(&traced[traced.len() - 5..], &[5, 0, 0, 0, 0]);
    assert_eq!(
        seen.lock().unwrap().as_slice(),
        &[expected.len() - 4 - 2, expected.len() - 4]
    );
}

#[test]
fn an_unwritable_trace_path_is_a_usage_error() {
    let dir = tempfile::TempDir::new().unwrap();
    let out = dir.path().join("record.json");
    let trace = dir.path().join("missing").join("trace");
    let err = drive_mode(traced_case(&out, &trace), &quiet_settings(), None, |_| {
        TestCaseResult::Valid
    })
    .unwrap_err();
    let message = usage_message(err);
    assert!(message.contains("HEGEL_FUZZ_TRACE="), "{message}");
    assert!(message.contains("could not be written"), "{message}");
}

fn read_record(path: &std::path::Path) -> serde_json::Value {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

fn draw_int(ds: &dyn DataSource, min: i64, max: i64) -> Result<i64, TestCaseResult> {
    use crate::native::bignum::ToPrimitive;
    ds.generate_integer(&BigInt::from(min), &BigInt::from(max))
        .map(|v| v.to_i64().unwrap())
        .map_err(|_| TestCaseResult::Overrun)
}

#[test]
fn skip_runs_nothing() {
    let calls = AtomicUsize::new(0);
    let result = drive_mode(FuzzMode::Skip, &quiet_settings(), Some("k"), |_| {
        calls.fetch_add(1, Ordering::SeqCst);
        TestCaseResult::Valid
    })
    .unwrap();
    assert!(result.failures.is_empty());
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn a_case_runs_exactly_once_and_records_what_it_drew() {
    let dir = tempfile::TempDir::new().unwrap();
    let out = dir.path().join("record.json");
    let calls = AtomicUsize::new(0);
    let result = drive_mode(
        case(&[int(42)], &out),
        &quiet_settings(),
        Some("crate::test"),
        |ds| {
            calls.fetch_add(1, Ordering::SeqCst);
            ds.start_span(77).unwrap();
            let n = match draw_int(ds, 0, 100) {
                Ok(n) => n,
                Err(r) => return r,
            };
            ds.stop_span(false).unwrap();
            ds.generate_boolean(0.5, None).unwrap();
            ds.event_observation("hit", None).unwrap();
            ds.event_observation("size", Some(3.0)).unwrap();
            ds.target_observation(1.5, "score").unwrap();
            assert_eq!(n, 42);
            TestCaseResult::Valid
        },
    )
    .unwrap();
    assert!(result.failures.is_empty());
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    let record = read_record(&out);
    assert_eq!(record["engine_version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(record["test"], "crate::test");
    assert_eq!(record["status"], "valid");
    assert_eq!(record["origin"], serde_json::Value::Null);
    assert_eq!(record["prefix_length"], 1);
    assert_eq!(record["prefix_consumed"], 1);
    assert_eq!(record["misaligned_at"], serde_json::Value::Null);
    let choices = record["choices"].as_array().unwrap();
    assert_eq!(choices.len(), 2);
    assert_eq!(choices[0]["kind"], "integer");
    assert_eq!(choices[0]["value"], "42");
    assert_eq!(choices[0]["min"], "0");
    assert_eq!(choices[0]["max"], "100");
    assert_eq!(choices[0]["shrink_towards"], "0");
    assert_eq!(choices[0]["forced"], false);
    assert_eq!(choices[1]["kind"], "boolean");
    assert_eq!(choices[1]["p"], 0.5);
    let spans = record["spans"].as_array().unwrap();
    let outer = spans.iter().find(|s| s["label"] == "77").unwrap();
    assert_eq!(outer["start"], 0);
    assert_eq!(outer["end"], 1);
    assert_eq!(outer["discarded"], false);
    assert!(
        spans
            .iter()
            .any(|s| s["parent"] == outer["depth"].as_u64().unwrap()
                || s["depth"].as_u64().unwrap() > outer["depth"].as_u64().unwrap())
    );
    assert_eq!(
        record["events"],
        serde_json::json!([{"name": "hit", "value": null}, {"name": "size", "value": 3.0}])
    );
    assert_eq!(record["targets"], serde_json::json!({"score": 1.5}));
    assert!(record["elapsed_ms"].as_f64().unwrap() >= 0.0);

    let encoded = record["choices_base64"].as_str().unwrap();
    let expected = serialize_choices(&[
        int(42),
        ChoiceValue::Boolean(choices[1]["value"].as_bool().unwrap()),
    ])
    .unwrap();
    assert_eq!(encoded, base64_encode(&expected));
}

#[test]
fn a_misfitting_prefix_is_reported_and_the_case_continues() {
    let dir = tempfile::TempDir::new().unwrap();
    let out = dir.path().join("record.json");
    drive_mode(
        case(&[ChoiceValue::Boolean(true), int(5), int(6)], &out),
        &quiet_settings(),
        None,
        |ds| {
            for _ in 0..2 {
                if let Err(r) = draw_int(ds, 0, 100) {
                    return r;
                }
            }
            TestCaseResult::Valid
        },
    )
    .unwrap();
    let record = read_record(&out);
    assert_eq!(record["test"], serde_json::Value::Null);
    assert_eq!(record["prefix_length"], 3);
    assert_eq!(record["prefix_consumed"], 2);
    assert_eq!(record["misaligned_at"], 0);
    assert_eq!(record["choices"][1]["value"], "5");
}

#[test]
fn an_interesting_case_is_the_runs_failure_with_a_reproduce_blob() {
    let dir = tempfile::TempDir::new().unwrap();
    let out = dir.path().join("record.json");
    let result = drive_mode(
        case(&[int(7)], &out),
        &quiet_settings(),
        Some("k"),
        |ds| match draw_int(ds, 0, 100) {
            Ok(7) => TestCaseResult::Interesting(crate::backend::Failure {
                origin: "Panic at a.rs:1:1".to_string(),
                reproduce_blob: None,
                caveat: None,
            }),
            Ok(_) => TestCaseResult::Valid,
            Err(r) => r,
        },
    )
    .unwrap();
    assert_eq!(result.failures.len(), 1);
    assert_eq!(result.failures[0].origin, "Panic at a.rs:1:1");
    let blob = result.failures[0].reproduce_blob.as_deref().unwrap();
    match crate::native::blob::decode_blob(blob).unwrap() {
        crate::native::blob::DecodedBlob::Choices(choices) => assert_eq!(choices, vec![int(7)]),
        _ => panic!("a fuzz failure's blob is its choice sequence"),
    }
    let record = read_record(&out);
    assert_eq!(record["status"], "interesting");
    assert_eq!(record["origin"], "Panic at a.rs:1:1");
}

#[test]
fn invalid_and_overrun_cases_are_recorded_without_retry() {
    for (outcome, status) in [
        (TestCaseResult::Invalid, "invalid"),
        (TestCaseResult::Overrun, "overrun"),
    ] {
        let dir = tempfile::TempDir::new().unwrap();
        let out = dir.path().join("record.json");
        let calls = AtomicUsize::new(0);
        let result = drive_mode(case(&[], &out), &quiet_settings(), Some("k"), |_| {
            calls.fetch_add(1, Ordering::SeqCst);
            outcome.clone()
        })
        .unwrap();
        assert!(result.failures.is_empty());
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(read_record(&out)["status"], status);
    }
}

#[test]
fn a_cloned_stream_is_recorded_as_a_clone_choice() {
    let dir = tempfile::TempDir::new().unwrap();
    let out = dir.path().join("record.json");
    drive_mode(case(&[], &out), &quiet_settings(), Some("k"), |ds| {
        let child = ds.clone_stream().unwrap();
        child.generate_boolean(0.5, None).unwrap();
        TestCaseResult::Valid
    })
    .unwrap();
    let record = read_record(&out);
    assert_eq!(record["choices"][0]["kind"], "clone");
    assert_eq!(record["choices"][0]["children"][0]["kind"], "boolean");
    assert!(record["choices"][0]["spans"].is_array());
}

#[test]
fn an_unwritable_output_path_is_a_usage_error() {
    let dir = tempfile::TempDir::new().unwrap();
    let out = dir
        .path()
        .join("no")
        .join("such")
        .join("dir")
        .join("record.json");
    let err = drive_mode(case(&[], &out), &quiet_settings(), Some("k"), |_| {
        TestCaseResult::Valid
    })
    .unwrap_err();
    let message = usage_message(err);
    assert!(message.contains("HEGEL_FUZZ_OUTPUT="), "{message}");
    assert!(message.contains("could not be written"), "{message}");
}

#[test]
fn reproduce_replays_the_entry_shrinks_the_failure_and_persists_it() {
    let dir = tempfile::TempDir::new().unwrap();
    let db_path = dir.path().join("db").to_str().unwrap().to_string();
    let settings = quiet_settings().database(Some(db_path.clone()));
    let entry = serialize_choices(&[int(60)]).unwrap();
    let result = drive_mode(
        FuzzMode::Reproduce { entry },
        &settings,
        Some("k"),
        |ds| match draw_int(ds, 0, 100) {
            Ok(n) if n >= 10 => TestCaseResult::Interesting(crate::backend::Failure {
                origin: "Panic at big".to_string(),
                reproduce_blob: None,
                caveat: None,
            }),
            Ok(_) => TestCaseResult::Valid,
            Err(r) => r,
        },
    )
    .unwrap();
    assert_eq!(result.failures.len(), 1);
    let blob = result.failures[0].reproduce_blob.as_deref().unwrap();
    match crate::native::blob::decode_blob(blob).unwrap() {
        crate::native::blob::DecodedBlob::Choices(choices) => {
            assert_eq!(choices, vec![int(10)], "the failure is shrunk");
        }
        _ => panic!("expected a choices blob"),
    }
    let db = crate::native::database::DirectoryTestCaseDatabase::new(&db_path);
    let stored = crate::native::database::TestCaseDatabase::fetch(&db, b"k");
    assert_eq!(stored, vec![serialize_choices(&[int(10)]).unwrap()]);
}

#[test]
fn reproduce_of_a_stale_entry_runs_it_once_and_generates_nothing() {
    let calls = AtomicUsize::new(0);
    let entry = serialize_choices(&[int(60)]).unwrap();
    let result = drive_mode(
        FuzzMode::Reproduce { entry },
        &quiet_settings(),
        Some("k"),
        |ds| {
            calls.fetch_add(1, Ordering::SeqCst);
            match draw_int(ds, 0, 100) {
                Ok(_) => TestCaseResult::Valid,
                Err(r) => r,
            }
        },
    )
    .unwrap();
    assert!(result.failures.is_empty());
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn every_choice_kind_renders_with_its_constraint() {
    let nodes = vec![
        ChoiceNode::float(
            FloatChoice {
                min_value: f64::NEG_INFINITY,
                max_value: f64::INFINITY,
                allow_nan: true,
                allow_infinity: true,
                smallest_nonzero_magnitude: 5e-324,
            },
            f64::NAN,
            false,
        ),
        ChoiceNode::float(
            FloatChoice {
                min_value: -1.5,
                max_value: 2.0,
                allow_nan: false,
                allow_infinity: false,
                smallest_nonzero_magnitude: 1e-3,
            },
            0.25,
            true,
        ),
        ChoiceNode::bytes(
            BytesChoice {
                min_size: 0,
                max_size: 4,
            },
            vec![0x00, 0xab, 0xff],
            false,
        ),
        ChoiceNode::string(
            StringChoice {
                intervals: Arc::new(IntervalSet::new(vec![(0, 0x10FFFF)]).unwrap()),
                min_size: 1,
                max_size: 9,
            },
            vec![
                'a' as u32,
                '"' as u32,
                '\\' as u32,
                '\n' as u32,
                0x1F,
                0xD800,
                0x1F600,
            ],
            false,
        ),
        ChoiceNode::clone_stream(
            Arc::new(RealizedStream::new(
                vec![ChoiceNode::boolean(BooleanChoice { p: 1.0 }, true, true)],
                vec![Span {
                    start: 0,
                    end: 1,
                    label: u64::MAX,
                    depth: 0,
                    parent: None,
                    discarded: true,
                }],
            )),
            false,
        ),
    ];
    let run = RunResult {
        status: Status::Valid,
        nodes,
        spans: vec![Span {
            start: 0,
            end: 5,
            label: 3,
            depth: 1,
            parent: Some(0),
            discarded: false,
        }],
        origin: None,
        target_observations: crate::native::HashMap::default(),
        events: Vec::new(),
        divergence: None,
        settled: Vec::new(),
        ended: false,
    };
    let record: serde_json::Value =
        serde_json::from_str(&record_json(None, &[], &run, None, false)).unwrap();
    let choices = &record["choices"];
    assert_eq!(choices[0]["kind"], "float");
    assert_eq!(choices[0]["value"], "NaN");
    assert_eq!(choices[0]["min"], "-inf");
    assert_eq!(choices[0]["max"], "inf");
    assert_eq!(choices[0]["allow_nan"], true);
    assert_eq!(choices[1]["value"], 0.25);
    assert_eq!(choices[1]["min"], -1.5);
    assert_eq!(choices[1]["max"], 2.0);
    assert_eq!(choices[1]["smallest_nonzero_magnitude"], 1e-3);
    assert_eq!(choices[1]["allow_infinity"], false);
    assert_eq!(choices[1]["forced"], true);
    assert_eq!(choices[2]["kind"], "bytes");
    assert_eq!(choices[2]["value"], "00abff");
    assert_eq!(choices[2]["max_size"], 4);
    assert_eq!(choices[3]["kind"], "string");
    assert_eq!(choices[3]["value"], "a\"\\\n\u{1F}\u{FFFD}\u{1F600}");
    assert_eq!(choices[3]["min_size"], 1);
    assert_eq!(choices[4]["kind"], "clone");
    assert_eq!(choices[4]["children"][0]["forced"], true);
    assert_eq!(choices[4]["children"][0]["p"], 1.0);
    assert_eq!(choices[4]["spans"][0]["label"], u64::MAX.to_string());
    assert_eq!(choices[4]["spans"][0]["discarded"], true);
    assert_eq!(record["spans"][0]["parent"], 0);
    assert_eq!(record["spans"][0]["depth"], 1);
    assert_eq!(record["elapsed_ms"], serde_json::Value::Null);
    assert_eq!(record["prefix_consumed"], 0);
}

#[test]
fn a_lone_surrogate_in_a_string_choice_prints_as_the_replacement_character() {
    assert_eq!(
        string_of_codepoints(
            [0x41, 0xD800, 0x09, 0x110000, 0x0D, 0x0A, 0x22, 0x5C, 0x01].into_iter()
        ),
        "\"A\u{FFFD}\\t\u{FFFD}\\r\\n\\\"\\\\\\u0001\""
    );
    assert_eq!(number(f64::NEG_INFINITY), "\"-inf\"");
    assert_eq!(number(-0.0), "-0.0");
}

#[test]
fn the_record_shape_is_parsed_and_defaults_to_full() {
    for (vars, expected) in [
        (vec![(OUTPUT_VAR, "/out")], false),
        (vec![(OUTPUT_VAR, "/out"), (RECORD_VAR, "full")], false),
        (vec![(OUTPUT_VAR, "/out"), (RECORD_VAR, "compact")], true),
    ] {
        match from_env_with(env_of(&vars), None).unwrap() {
            Some(FuzzMode::Case { compact, .. }) => assert_eq!(compact, expected),
            other => panic!("{other:?}"),
        }
    }
    let env = env_of(&[("HEGEL_FUZZ_SERVER", "/req,/rep"), (RECORD_VAR, "compact")]);
    assert!(matches!(
        from_env_with(env, Some("k")).unwrap(),
        Some(FuzzMode::Server { compact: true, .. })
    ));
    let err =
        from_env_with(env_of(&[(OUTPUT_VAR, "/out"), (RECORD_VAR, "tiny")]), None).unwrap_err();
    assert_eq!(
        usage_message(err),
        "HEGEL_FUZZ_RECORD must be full or compact, got \"tiny\""
    );
}

#[test]
fn a_compact_record_leaves_the_choice_and_span_arrays_to_the_realized_form() {
    let run = RunResult {
        status: Status::Valid,
        nodes: vec![ChoiceNode::boolean(BooleanChoice { p: 0.5 }, true, false)],
        spans: vec![Span {
            start: 0,
            end: 1,
            label: 3,
            depth: 0,
            parent: None,
            discarded: false,
        }],
        origin: None,
        target_observations: crate::native::HashMap::default(),
        events: Vec::new(),
        divergence: None,
        settled: Vec::new(),
        ended: false,
    };
    let full: serde_json::Value =
        serde_json::from_str(&record_json(None, &[], &run, None, false)).unwrap();
    let compact: serde_json::Value =
        serde_json::from_str(&record_json(None, &[], &run, None, true)).unwrap();
    assert!(full.get("choices").is_some() && full.get("spans").is_some());
    assert!(compact.get("choices").is_none() && compact.get("spans").is_none());
    assert_eq!(compact["realized_base64"], full["realized_base64"]);
    assert_eq!(compact["choices_base64"], full["choices_base64"]);
    assert_eq!(compact["status"], "valid");
}
