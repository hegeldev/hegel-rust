//! Distilled from debian-changelog/10 (`add_bullet_stays_within_78_columns`, 9 answers in 10
//! failing zoo runs): a formatted line that must not exceed a width.
//!
//! `n ∈ [1, 12]` words, each by a kind `∈ [0, 3]`: `Closes(n ∈ [1, 9_999_999])` renders as
//! `Closes: #n`; `Letters` is a run of `len ∈ [20, 70]` letters drawn one by one; `Table` picks
//! from the zoo's 38-word table. The line is `"* " + words joined by spaces`; the property fails
//! iff it is 79+ columns wide and no single word explains it. Shortlex ideal, by the choice
//! sequence: four words are at most 73 columns, so five are needed, and five `Closes` words are
//! 11 choices with digits summing to 28 — `[Closes: #1, #100000, #1000000 × 3]`, exactly 79
//! columns. The shrinker lowers every value to its minimum and makes the width up with *more*
//! words (`[Closes: #1 × 6, Closes: #10]`, 15 choices): deleting a word has to be paid for by
//! raising a number a digit class, raise-and-delete under a width budget. The controls are the
//! same trade-off with two adjacent integer draws (a digit count plus padding, a table word plus
//! padding), which `redistribute_numeric_pairs` crosses one digit class at a time. This is a
//! case where the shortlex order and a human disagree: a human writes `[Closes: #1, a × 66]`.

use super::assert_shrinks_to;
use hegel::TestCase;
use hegel::generators as gs;

const LIMIT: usize = 20;
const TABLE: [&str; 4] = ["Closes:", "Fix", "upstream", "Bump"];

const ZOO_WORDS: &[&str] = &[
    "Fix",
    "the",
    "build",
    "on",
    "arm64",
    "New",
    "upstream",
    "release",
    "Team",
    "upload",
    "Thanks",
    "to",
    "A.",
    "Hacker",
    "Drop",
    "dependency",
    "python3-six",
    "Standards-Version",
    "4.7.0",
    "debhelper-compat",
    "13",
    "Closes",
    "closes:",
    "LP",
    "Update",
    "d/copyright",
    "Bump",
    "(no",
    "changes)",
    "Vernooĳ",
    "Simó",
    "ü",
    "foo,",
    "bar.",
    "#",
    "Closes:",
    "->",
    "%s",
];

fn pad() -> gs::IntegerGenerator<usize> {
    gs::integers::<usize>().max_value(100)
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Word {
    Closes(u32),
    Letters(String),
    Table(usize),
}

fn draw_words(tc: &TestCase) -> Vec<Word> {
    let n = tc.draw_silent(gs::integers::<usize>().min_value(1).max_value(12));
    (0..n)
        .map(
            |_| match tc.draw_silent(gs::integers::<u8>().max_value(3)) {
                0 => Word::Closes(
                    tc.draw_silent(gs::integers::<u32>().min_value(1).max_value(9_999_999)),
                ),
                1 => {
                    let len = tc.draw_silent(gs::integers::<usize>().min_value(20).max_value(70));
                    let s: String = (0..len)
                        .map(|_| {
                            (b'a' + tc.draw_silent(gs::integers::<u8>().max_value(25))) as char
                        })
                        .collect();
                    Word::Letters(s)
                }
                _ => Word::Table(
                    tc.draw_silent(gs::integers::<usize>().max_value(ZOO_WORDS.len() - 1)),
                ),
            },
        )
        .collect()
}

fn render(w: &Word) -> String {
    match w {
        Word::Closes(n) => format!("Closes: #{n}"),
        Word::Letters(s) => s.clone(),
        Word::Table(i) => ZOO_WORDS[*i].to_string(),
    }
}

fn bullet_line_too_wide(words: &[Word]) -> bool {
    let text = words.iter().map(render).collect::<Vec<_>>().join(" ");
    let line = format!("* {text}");
    let width = line.chars().count();
    let longest = text
        .split_whitespace()
        .map(|w| w.chars().count())
        .max()
        .unwrap_or(0);
    width > 78 && longest + 2 < width
}

fn words_ideal() -> Vec<Word> {
    vec![
        Word::Closes(1),
        Word::Closes(100_000),
        Word::Closes(1_000_000),
        Word::Closes(1_000_000),
        Word::Closes(1_000_000),
    ]
}

fn human_ideal() -> Vec<Word> {
    vec![Word::Closes(1), Word::Letters("a".repeat(66))]
}

type Numbered = (u32, usize);

fn draw_numbered(tc: &TestCase) -> Numbered {
    let n = tc.draw_silent(gs::integers::<u32>().min_value(1).max_value(1_000_000));
    let pad = tc.draw_silent(pad());
    (n, pad)
}

fn numbered_line_too_wide((n, pad): &Numbered) -> bool {
    format!("#{n} {}", "a".repeat(*pad)).chars().count() >= LIMIT
}

type Worded = (usize, usize);

fn draw_worded(tc: &TestCase) -> Worded {
    let word = tc.draw_silent(gs::integers::<usize>().max_value(TABLE.len() - 1));
    let pad = tc.draw_silent(pad());
    (word, pad)
}

fn worded_line_too_wide((word, pad): &Worded) -> bool {
    format!("{} {}", TABLE[*word], "a".repeat(*pad))
        .chars()
        .count()
        >= LIMIT
}

#[test]
fn the_ideal_fails_and_is_smallest() {
    assert!(bullet_line_too_wide(&words_ideal()));
    assert!(bullet_line_too_wide(&human_ideal()));
    assert!(!bullet_line_too_wide(&[
        Word::Closes(1),
        Word::Closes(10_000),
        Word::Closes(1_000_000),
        Word::Closes(1_000_000),
        Word::Closes(1_000_000),
    ]));
    assert!(!bullet_line_too_wide(&vec![Word::Table(17); 4]));
    assert!(!bullet_line_too_wide(&vec![Word::Closes(9_999_999); 4]));
    assert!(!bullet_line_too_wide(&[
        Word::Closes(1),
        Word::Letters("a".repeat(65))
    ]));
    assert!(!bullet_line_too_wide(&[Word::Letters("a".repeat(77))]));
    assert!(bullet_line_too_wide(&[
        Word::Closes(100),
        Word::Letters("a".repeat(64))
    ]));
    assert!(bullet_line_too_wide(&[
        Word::Table(6),
        Word::Letters("a".repeat(68))
    ]));

    assert!(numbered_line_too_wide(&(1, 17)));
    assert!(!numbered_line_too_wide(&(1, 16)));
    assert!(!numbered_line_too_wide(&(9, 16)));
    assert!(numbered_line_too_wide(&(10, 16)));
    assert!(!numbered_line_too_wide(&(99, 15)));
    assert!(numbered_line_too_wide(&(100, 15)));

    assert!(worded_line_too_wide(&(0, 12)));
    assert!(!worded_line_too_wide(&(0, 11)));
    assert!(worded_line_too_wide(&(1, 16)));
    assert!(!worded_line_too_wide(&(1, 15)));
    assert!(worded_line_too_wide(&(2, 11)));
    assert!(!worded_line_too_wide(&(2, 10)));
}

#[test]
#[ignore = "shrinker: no pass raises a number a digit class while deleting a word"]
fn the_bullet_line_shrinks_to_five_closes_words() {
    assert_shrinks_to(&words_ideal(), 30, 1000, draw_words, |words| {
        bullet_line_too_wide(words)
    });
}

#[test]
fn control_the_number_is_lowered_across_digit_classes() {
    assert_shrinks_to(&(1, 17), 30, 500, draw_numbered, numbered_line_too_wide);
}

#[test]
fn control_the_word_is_lowered_to_the_first_of_the_table() {
    assert_shrinks_to(&(0, 12), 30, 500, draw_worded, worded_line_too_wide);
}
