//! End-to-end tests for draw-time pretty printing: the `let name = value;`
//! lines a failing test reports, produced by [`PrintableGenerator`]
//! implementations through the engine's document printer.

use std::panic::{AssertUnwindSafe, catch_unwind};

mod common;

use common::utils::printed_draw_lines;
use std::sync::{Arc, Mutex};

use hegel::generators::{self as gs, Generator};
use hegel::{Hegel, Phase, PrintableGenerator, Settings, Verbosity};

/// Run a failing property and capture the final replay's draw/note lines.
///
/// The explain phase is disabled so the assertions here pin the printed
/// *shapes* alone; how explain annotations attach to those shapes is covered
/// by `tests/test_explain.rs`.
fn failing_lines<F>(body: F) -> Vec<String>
where
    F: FnMut(hegel::TestCase) + 'static,
{
    lines_at(Verbosity::Normal, body)
}

fn lines_at<F>(verbosity: Verbosity, body: F) -> Vec<String>
where
    F: FnMut(hegel::TestCase) + 'static,
{
    let buf: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let buf_writer = buf.clone();
    let sink: Arc<dyn Fn(&str) + Send + Sync> =
        Arc::new(move |s: &str| buf_writer.lock().unwrap().push(s.to_string()));

    let result = catch_unwind(AssertUnwindSafe(|| {
        hegel::with_output_override(sink, || {
            Hegel::new(body)
                .settings(
                    Settings::new()
                        .test_cases(50)
                        .database(None)
                        .verbosity(verbosity)
                        .derandomize(true)
                        .phases([
                            Phase::Explicit,
                            Phase::Reuse,
                            Phase::Generate,
                            Phase::Target,
                            Phase::Shrink,
                        ]),
                )
                .run();
        });
    }));
    assert!(result.is_err(), "expected the property to fail");
    let mut lines = buf.lock().unwrap().clone();
    if let Some(pos) = lines
        .iter()
        .position(|l| l.starts_with("thread '") && l.contains("panicked at"))
    {
        lines.truncate(pos);
    }
    lines
}

fn index_of(lines: &[String], needle: &str) -> usize {
    lines
        .iter()
        .position(|l| l == needle)
        .unwrap_or_else(|| panic!("expected {needle:?} in {lines:?}"))
}

#[test]
fn leaf_draws_print_their_shrunk_value() {
    let lines = failing_lines(|tc| {
        let n: i64 = tc.draw(gs::integers::<i64>().min_value(0).max_value(100));
        let _ = n;
        panic!("boom");
    });
    assert_eq!(lines, vec!["let draw_1 = 0;"]);
}

#[test]
fn notes_and_draws_interleave_in_order() {
    let lines = failing_lines(|tc| {
        let _ = tc.draw(gs::booleans());
        tc.note("mid");
        let _ = tc.draw(gs::booleans());
        panic!("boom");
    });
    assert_eq!(
        lines,
        vec!["let draw_1 = false;", "mid", "let draw_2 = false;"]
    );
}

#[test]
fn wide_values_wrap_across_lines() {
    let lines = failing_lines(|tc| {
        let element = "aaaaaaaaaaaaaaaaaaaa".to_string();
        let _ = tc.draw(gs::vecs(gs::just(element)).min_size(3).max_size(3));
        panic!("boom");
    });
    assert_eq!(
        lines,
        vec![
            "let draw_1 = vec![\"aaaaaaaaaaaaaaaaaaaa\".to_string(),",
            "     \"aaaaaaaaaaaaaaaaaaaa\".to_string(),",
            "     \"aaaaaaaaaaaaaaaaaaaa\".to_string()];",
        ]
    );
}

#[test]
fn structural_combinators_print_compositionally() {
    let lines = failing_lines(|tc| {
        let _ = tc.draw(hegel::tuples!(gs::booleans(), gs::integers::<i64>()));
        panic!("boom");
    });
    assert_eq!(lines, vec!["let draw_1 = (false, 0);"]);

    let lines = failing_lines(|tc| {
        let _ = tc.draw(gs::optional(gs::integers::<u8>()));
        panic!("boom");
    });
    assert_eq!(lines, vec!["let draw_1 = None;"]);

    let lines = failing_lines(|tc| {
        let _ = tc.draw(hegel::one_of!(gs::just(5i32), gs::just(7i32)));
        panic!("boom");
    });
    assert_eq!(lines, vec!["let draw_1 = 5;"]);

    let lines = failing_lines(|tc| {
        let _ = tc.draw(hegel::one_of!(gs::just(9i32)));
        panic!("boom");
    });
    assert_eq!(lines, vec!["let draw_1 = 9;"]);

    let lines = failing_lines(|tc| {
        let v = tc.draw(hegel::one_of!(gs::just(5i32), gs::just(7i32)));
        assert_ne!(v, 7);
    });
    assert_eq!(lines, vec!["let draw_1 = 7;"]);

    let lines = failing_lines(|tc| {
        let _ = tc.draw(
            gs::integers::<i64>()
                .min_value(1)
                .max_value(3)
                .flat_map(|n| gs::vecs(gs::just(n)).min_size(1).max_size(1)),
        );
        panic!("boom");
    });
    assert_eq!(lines, vec!["let draw_1 = vec![1];"]);

    let lines = failing_lines(|tc| {
        let _ = tc.draw(gs::arrays::<_, _, 2>(gs::booleans()));
        panic!("boom");
    });
    assert_eq!(lines, vec!["let draw_1 = [false, false];"]);
}

#[test]
fn sets_and_maps_print_in_draw_order() {
    let lines = failing_lines(|tc| {
        let _ = tc.draw(gs::hashsets(gs::sampled_from(vec![1, 2, 3])).min_size(1));
        panic!("boom");
    });
    assert_eq!(lines, vec!["let draw_1 = HashSet::from([1]);"]);

    let lines = failing_lines(|tc| {
        let _ = tc.draw(gs::hashsets(gs::text().max_size(2)).min_size(1));
        panic!("boom");
    });
    assert_eq!(
        lines,
        vec!["let draw_1 = HashSet::from([\"\".to_string()]);"]
    );

    let lines = failing_lines(|tc| {
        let _ = tc.draw(gs::hashmaps(gs::sampled_from(vec![9]), gs::booleans()).min_size(1));
        panic!("boom");
    });
    assert_eq!(lines, vec!["let draw_1 = HashMap::from([(9, false)]);"]);

    let lines = failing_lines(|tc| {
        let _ = tc.draw(gs::hashmaps(gs::text().max_size(2), gs::booleans()).min_size(1));
        panic!("boom");
    });
    assert_eq!(
        lines,
        vec!["let draw_1 = HashMap::from([(\"\".to_string(), false)]);"]
    );
}

#[test]
fn filtered_draws_print_only_the_accepted_value() {
    let lines = failing_lines(|tc| {
        let n: i64 = tc.draw(
            gs::integers::<i64>()
                .min_value(0)
                .max_value(100)
                .filter(|n| n % 2 == 1),
        );
        let _ = n;
        panic!("boom");
    });
    assert_eq!(lines, vec!["let draw_1 = 1;"]);
}

#[test]
fn rejected_filter_attempts_never_corrupt_verbose_output() {
    let lines = lines_at(Verbosity::Verbose, |tc| {
        let n: i64 = tc.draw(
            gs::integers::<i64>()
                .min_value(0)
                .max_value(1000)
                .filter(|n| n % 5 == 0),
        );
        assert!(n == 0);
    });
    let draw_lines: Vec<&String> = lines.iter().filter(|l| l.contains("let ")).collect();
    assert!(!draw_lines.is_empty());
    let pattern = regex::Regex::new(r"^let draw_\d+ = \d+;$").unwrap();
    for line in draw_lines {
        assert!(pattern.is_match(line), "malformed draw line {line:?}");
    }
}

#[test]
fn exhausted_filter_retries_print_nothing() {
    let lines = lines_at(Verbosity::Verbose, |tc| {
        let n: i64 = tc.draw(
            gs::integers::<i64>()
                .min_value(0)
                .max_value(100)
                .filter(|_| false),
        );
        let _ = n;
    });
    assert!(
        lines.iter().all(|l| !l.contains("let ")),
        "rejected attempts leaked into the document: {lines:?}"
    );
}

#[test]
fn unique_vec_rejections_never_corrupt_verbose_output() {
    let lines = lines_at(Verbosity::Verbose, |tc| {
        let v: Vec<i64> = tc.draw(
            gs::vecs(gs::integers::<i64>().min_value(0).max_value(3))
                .unique(true)
                .max_size(3),
        );
        let _ = v;
        let b: bool = tc.draw(gs::booleans());
        assert!(!b);
    });
    let pattern = regex::Regex::new(r"^let draw_1 = vec!\[(\d+(, \d+)*)?\];$").unwrap();
    let vec_lines: Vec<&String> = lines.iter().filter(|l| l.contains("= vec![")).collect();
    assert!(!vec_lines.is_empty());
    for line in vec_lines {
        assert!(pattern.is_match(line), "malformed vec line {line:?}");
    }
}

#[derive(hegel::DefaultGenerator)]
struct Sonar {
    #[allow(dead_code)]
    active: bool,
    #[allow(dead_code)]
    depth: u8,
}

#[test]
fn derived_generators_print_compositionally_without_pretty_printable() {
    let lines = failing_lines(|tc| {
        let _ = tc.draw(gs::default::<Sonar>());
        panic!("boom");
    });
    assert_eq!(
        lines,
        vec!["let draw_1 = Sonar { active: false, depth: 0 };"]
    );
}

#[derive(hegel::DefaultGenerator)]
enum Signal {
    Quiet,
    Level {
        #[allow(dead_code)]
        db: u8,
    },
    Pair(#[allow(dead_code)] bool, #[allow(dead_code)] u8),
}

#[test]
fn derived_enum_generators_print_every_variant_shape() {
    let unit = failing_lines(|tc| {
        let _ = tc.draw(gs::default::<Signal>());
        panic!("boom");
    });
    assert_eq!(unit, vec!["let draw_1 = Signal::Quiet;"]);

    let named = failing_lines(|tc| {
        let signal: Signal = tc.draw(gs::default::<Signal>());
        assert!(!matches!(signal, Signal::Level { .. }), "boom");
    });
    assert_eq!(named, vec!["let draw_1 = Signal::Level { db: 0 };"]);

    let tuple = failing_lines(|tc| {
        let signal: Signal = tc.draw(gs::default::<Signal>());
        assert!(!matches!(signal, Signal::Pair(..)), "boom");
    });
    assert_eq!(tuple, vec!["let draw_1 = Signal::Pair(false, 0);"]);
}

#[derive(Clone, hegel::DefaultGenerator, hegel::PrettyPrintable)]
enum Depth {
    Surface,
    Dive { meters: u8, staged: bool },
    Split(u8, bool),
}

#[test]
fn derived_generator_printing_matches_derived_pretty_printable() {
    for force in [
        (|d: &Depth| matches!(d, Depth::Surface)) as fn(&Depth) -> bool,
        |d| matches!(d, Depth::Dive { .. }),
        |d| matches!(d, Depth::Split(..)),
    ] {
        let captured: Arc<Mutex<Option<Depth>>> = Arc::new(Mutex::new(None));
        let saved = captured.clone();
        let lines = failing_lines(move |tc| {
            let depth: Depth = tc.draw(gs::default::<Depth>());
            let hit = force(&depth);
            *saved.lock().unwrap() = Some(depth);
            assert!(!hit, "boom");
        });
        let value = captured.lock().unwrap().take().unwrap();
        let mut printer = hegel::PrettyPrinter::new(79);
        hegel::PrettyPrintable::pretty_print(&value, &mut printer);
        assert_eq!(lines, vec![format!("let draw_1 = {};", printer.value())]);
    }
}

#[test]
fn print_adapters_control_the_representation() {
    let lines = failing_lines(|tc| {
        let _ = tc.draw(
            gs::integers::<i64>()
                .min_value(0)
                .max_value(9)
                .print_with(|v, p| p.text(&format!("#{v}"))),
        );
        panic!("boom");
    });
    assert_eq!(lines, vec!["let draw_1 = #0;"]);

    let lines = failing_lines(|tc| {
        let _ = tc.draw(gs::booleans().print_as_value());
        panic!("boom");
    });
    assert_eq!(lines, vec!["let draw_1 = false;"]);
}

#[test]
fn notes_inside_composites_flush_after_the_draw() {
    let lines = failing_lines(|tc| {
        let _ = tc.draw(hegel::compose!(|tc| {
            tc.note("inner note");
            tc.draw(gs::booleans())
        }));
        panic!("boom");
    });
    assert_eq!(lines, vec!["let draw_1 = false;", "inner note"]);
}

#[test]
fn multiline_notes_split_into_lines() {
    let lines = failing_lines(|tc| {
        let _ = tc.draw(gs::booleans());
        tc.note("first\nsecond");
        panic!("boom");
    });
    assert_eq!(lines, vec!["let draw_1 = false;", "first", "second"]);
}

#[test]
fn silent_draws_print_nothing() {
    let lines = failing_lines(|tc| {
        let _ = tc.draw_silent(gs::integers::<i64>());
        let _ = tc.draw(gs::booleans());
        panic!("boom");
    });
    assert_eq!(lines, vec!["let draw_1 = false;"]);
}

#[test]
fn draws_after_notes_keep_document_order() {
    let lines = failing_lines(|tc| {
        tc.note("before anything");
        let _ = tc.draw(gs::booleans());
        panic!("boom");
    });
    assert!(index_of(&lines, "before anything") < index_of(&lines, "let draw_1 = false;"));
}

#[test]
fn every_leaf_generator_prints() {
    assert_eq!(
        printed_draw_lines(gs::text()),
        vec!["let draw_1 = \"\".to_string();"]
    );
    printed_draw_lines(gs::characters());
    printed_draw_lines(gs::from_regex("[a-z]{2}"));
    assert_eq!(
        printed_draw_lines(gs::binary()),
        vec!["let draw_1 = vec![];"]
    );
    printed_draw_lines(gs::emails());
    printed_draw_lines(gs::urls());
    printed_draw_lines(gs::domains());
    printed_draw_lines(gs::ip_addresses());
    printed_draw_lines(gs::ip_addresses().v4());
    printed_draw_lines(gs::ip_addresses().v6());
    printed_draw_lines(gs::date_strings());
    printed_draw_lines(gs::time_strings());
    printed_draw_lines(gs::datetime_strings());
    printed_draw_lines(gs::uuids());
    printed_draw_lines(gs::durations());
    printed_draw_lines(gs::floats::<f64>());
    printed_draw_lines(gs::characters().print_as_value());
}

#[test]
fn small_tuples_print_with_rust_syntax() {
    assert_eq!(
        printed_draw_lines(hegel::tuples!()),
        vec!["let draw_1 = ();"]
    );
    assert_eq!(
        printed_draw_lines(hegel::tuples!(gs::booleans())),
        vec!["let draw_1 = (false,);"]
    );
    assert_eq!(
        printed_draw_lines(hegel::tuples!(
            gs::booleans(),
            gs::booleans(),
            gs::booleans()
        )),
        vec!["let draw_1 = (false, false, false);"]
    );
}

#[test]
fn optional_some_prints_the_inner_draw() {
    let lines = failing_lines(|tc| {
        let v = tc.draw(gs::optional(gs::integers::<u8>()));
        assert!(v.is_none());
    });
    assert_eq!(lines, vec!["let draw_1 = Some(0);"]);
}

#[test]
fn filtered_sampled_from_prints_the_chosen_value() {
    let lines = failing_lines(|tc| {
        let _ = tc.draw(gs::sampled_from(vec![1, 2, 3]).filter(|n| *n > 1));
        panic!("boom");
    });
    assert_eq!(lines, vec!["let draw_1 = 2;"]);

    let lines = failing_lines(|tc| {
        let _ = tc.draw(
            gs::sampled_from(vec![1, 2])
                .print_as_value()
                .filter(|n| *n > 1),
        );
        panic!("boom");
    });
    assert_eq!(lines, vec!["let draw_1 = 2;"]);
}

#[test]
fn invalid_collection_sizes_report_while_printing() {
    for body in [
        (|tc: hegel::TestCase| {
            let _ = tc.draw(gs::vecs(gs::booleans()).min_size(5).max_size(2));
        }) as fn(hegel::TestCase),
        |tc| {
            let _ = tc.draw(gs::hashsets(gs::booleans()).min_size(5).max_size(2));
        },
        |tc| {
            let _ = tc.draw(
                gs::hashmaps(gs::booleans(), gs::booleans())
                    .min_size(5)
                    .max_size(2),
            );
        },
    ] {
        let result = catch_unwind(AssertUnwindSafe(|| {
            Hegel::new(body)
                .settings(
                    Settings::new()
                        .test_cases(2)
                        .database(None)
                        .verbosity(Verbosity::Verbose),
                )
                .run();
        }));
        let message = format!("{:?}", result.unwrap_err().downcast_ref::<String>());
        assert!(
            message.contains("min_size") || message.contains("max_size"),
            "{message}"
        );
    }
}

#[test]
fn multi_element_sets_and_maps_print_separators() {
    let lines = failing_lines(|tc| {
        let _ = tc.draw(gs::hashsets(gs::sampled_from(vec![1, 2, 3])).min_size(2));
        panic!("boom");
    });
    assert_eq!(lines.len(), 1);
    assert!(lines[0].contains(", "), "{lines:?}");

    let lines = failing_lines(|tc| {
        let _ = tc.draw(gs::hashsets(gs::integers::<i64>().min_value(0).max_value(1)).min_size(2));
        panic!("boom");
    });
    assert_eq!(lines.len(), 1);
    assert!(lines[0].contains(", "), "{lines:?}");

    let lines = failing_lines(|tc| {
        let _ = tc.draw(gs::hashmaps(gs::sampled_from(vec![1, 2]), gs::booleans()).min_size(2));
        panic!("boom");
    });
    assert_eq!(lines.len(), 1);
    assert!(lines[0].contains(", false), ("), "{lines:?}");
}

/// A compositional generator written the recommended way: one drawing body
/// in `do_draw_and_print`, with `do_draw` forwarding through the no-op
/// printer, and expensive formatting guarded by `should_print`.
struct PairGenerator;

impl Generator<(bool, bool)> for PairGenerator {
    fn do_draw(&self, tc: &hegel::TestCase) -> (bool, bool) {
        self.do_draw_and_print(tc, &mut hegel::PrettyPrinter::noop())
    }
}

impl hegel::PrintableGenerator<(bool, bool)> for PairGenerator {
    fn do_draw_and_print(
        &self,
        tc: &hegel::TestCase,
        printer: &mut hegel::PrettyPrinter,
    ) -> (bool, bool) {
        printer.begin_group(1, "(");
        let first = gs::booleans().draw_and_print(tc, printer);
        if printer.should_print() {
            printer.text(", ");
        }
        let second = gs::booleans().draw_and_print(tc, printer);
        printer.end_group(1, ")");
        (first, second)
    }
}

#[test]
fn print_as_debug_makes_foreign_debug_types_printable() {
    let lines = failing_lines(|tc| {
        let _ = tc.draw(gs::text().map(std::path::PathBuf::from).print_as_debug());
        panic!("boom");
    });
    assert_eq!(lines, vec!["let draw_1 = \"\";"]);
}

#[test]
fn single_body_generators_draw_and_print_through_the_noop_printer() {
    let lines = failing_lines(|tc| {
        let pair = tc.draw(PairGenerator);
        assert_ne!(pair, (false, false), "force a failure on the minimal pair");
    });
    assert_eq!(lines, vec!["let draw_1 = (false, false);"]);
}

/// A hand-written generator that calls `tc.note()` from inside `do_draw`
/// without opening a span, as composite bodies do.
struct NotingGenerator;

impl Generator<i64> for NotingGenerator {
    fn do_draw(&self, tc: &hegel::TestCase) -> i64 {
        tc.note("noted mid-draw");
        tc.draw_silent(gs::integers::<i64>().min_value(5).max_value(5))
    }
}

#[test]
fn note_inside_a_printed_draw_buffers_until_the_line_completes() {
    let lines = failing_lines(|tc| {
        let _ = tc.draw(NotingGenerator.print_with(|v, p| p.text(&format!("{v}"))));
        panic!("boom");
    });
    assert_eq!(lines, vec!["let draw_1 = 5;", "noted mid-draw"]);
}

/// A hand-written generator that makes a named `tc.draw` from inside
/// `do_draw`; during a printed draw the nested draw must stay silent, like a
/// draw inside any combinator span.
struct NestedDrawGenerator;

impl Generator<bool> for NestedDrawGenerator {
    fn do_draw(&self, tc: &hegel::TestCase) -> bool {
        tc.draw(gs::booleans())
    }
}

#[test]
fn nested_draw_inside_a_printed_draw_stays_silent() {
    let lines = failing_lines(|tc| {
        let _ = tc.draw(NestedDrawGenerator.print_with(|v, p| p.text(&format!("{v:?}"))));
        panic!("boom");
    });
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(lines[0].starts_with("let draw_1 = "), "{lines:?}");
}

#[test]
fn duplicate_set_elements_reject_cleanly_while_printing() {
    let lines = lines_at(Verbosity::Verbose, |tc| {
        let s = tc.draw(gs::hashsets(gs::integers::<i64>().min_value(0).max_value(1)).min_size(2));
        let _ = s;
        panic!("boom");
    });
    let pattern = regex::Regex::new(r"^let draw_\d+ = HashSet::from\(\[\d+, \d+\]\);$").unwrap();
    let set_lines: Vec<&String> = lines
        .iter()
        .filter(|l| l.contains("= HashSet::from(["))
        .collect();
    assert!(!set_lines.is_empty());
    for line in set_lines {
        assert!(pattern.is_match(line), "malformed set line {line:?}");
    }
}

#[test]
fn duplicate_map_keys_reject_cleanly_while_printing() {
    let lines = lines_at(Verbosity::Verbose, |tc| {
        let m = tc.draw(
            gs::hashmaps(
                gs::integers::<i64>().min_value(0).max_value(1),
                gs::booleans(),
            )
            .min_size(2),
        );
        let _ = m;
        panic!("boom");
    });
    let pattern = regex::Regex::new(
        r"^let draw_\d+ = HashMap::from\(\[\(\d+, (true|false)\), \(\d+, (true|false)\)\]\);$",
    )
    .unwrap();
    let map_lines: Vec<&String> = lines
        .iter()
        .filter(|l| l.contains("= HashMap::from(["))
        .collect();
    assert!(!map_lines.is_empty());
    for line in map_lines {
        assert!(pattern.is_match(line), "malformed map line {line:?}");
    }
}

#[test]
fn pool_draws_print_their_values() {
    let lines = failing_lines(|tc| {
        let mut pool = hegel::stateful::pool::<i64>(&tc);
        pool.add(5);
        pool.add(6);
        let reused: i64 = *tc.draw(pool.values_reusable());
        let consumed: i64 = tc.draw(pool.values_consumed());
        let _ = (reused, consumed);
        panic!("boom");
    });
    assert_eq!(lines.len(), 2);
    assert!(lines[0].starts_with("let draw_1 = "), "{lines:?}");
    assert!(lines[1].starts_with("let draw_2 = "), "{lines:?}");
}
