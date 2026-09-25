//! From hegel-zoo `rust/debian-changelog`, bug debian-changelog/9, test
//! `prepend_change_line_puts_the_line_first`: a header metadata item that has nothing to do with
//! the bug — `urgency=low`, `binary-only=yes` — survives shrinking in 13 of 30 zoo runs, with 21
//! distinct answers in all.
//!
//! A faithful port of the zoo's `arb_changelog(false)` → `arb_entry` draw structure: package,
//! version, distributions, `nmeta` metadata items, the urgency-comment coin *if* an urgency item
//! exists, body lines, maintainer name, email, date, malformation pick, and then — while the text
//! is formatted — one separator pick per metadata item, two blank-line counts and a footer-space
//! coin, plus the test's own trailing `arb_detail` draw. Bug 9 fires for every entry whose text
//! is clean, has a body and does not have exactly one blank line between header and body, so the
//! predicate is those text checks. Shortlex ideal: one entry, package `gtkmm3.0` (the table pick,
//! two choices; the generated one-letter `a` is three), version `1.0-1`, one distribution
//! `unstable`, no metadata, a body of one author line `  [ Jelmer ]`, no blank lines, empty
//! maintainer name, `jelmer@debian.org`, the earliest date with no weekday and the footer's
//! trailing space.
//!
//! Deleting a dead metadata item means `nmeta − 1` before it, the item's two to four draws, the
//! conditional urgency-comment coin right after the items, and the item's separator pick drawn
//! much later: three regions, one behind everything else, with the pinned body, maintainer and
//! malformation pick between (`distilled_split_element_pinned_value`). Seeds that do not keep
//! the metadata may instead keep the package `a`, where the table pick needs the kind lowered
//! *and* the next value raised. A human writes the ideal and drops nothing more.

use super::assert_shrinks_to;
use hegel::TestCase;
use hegel::generators as gs;

const PACKAGES: &[&str] = &[
    "g++",
    "libsigc++-2.0",
    "gtkmm3.0",
    "libxml++2.6",
    "dpkg",
    "breezy",
    "0ad",
    "a",
    "python3-defaults",
    "Foo",
];
const VERSIONS: &[&str] = &[
    "1.0-1",
    "0.1",
    "2:3.4~rc1-2ubuntu1",
    "1.0+dfsg-1~bpo12+1",
    "1:1.0-1+deb12u1",
    "3.3.4-1",
    "0.0.1-0ubuntu1",
    "1.2.3+really1.2.2-1",
    "20240101-1",
    "1.0-1+b1",
    "1.0-1+nmu1",
    "9",
    "1:0",
    "0.99.1~beta2+git20230101.abcdef-3",
];
const DISTS: &[&str] = &[
    "unstable",
    "experimental",
    "UNRELEASED",
    "bookworm-backports",
    "stable-security",
    "UNRELEASED-1",
    "trixie",
    "noble",
    "focal-proposed",
    "stable",
    "oldstable",
    "bookworm",
    "sid",
    "unstable-debug",
    "jammy",
];
const NAMES: &[&str] = &[
    "Jelmer",
    "Vernooĳ",
    "Jane",
    "Doe",
    "Adeodato",
    "Simó",
    "O'Brien",
    "Jean-Paul",
    "Dr.",
    "van",
    "der",
    "Berg",
    "李",
    "Guillem",
    "Jover",
];
const EMAILS: &[&str] = &[
    "jelmer@debian.org",
    "jane@example.com",
    "bob@x.org",
    "team+pkg@tracker.debian.org",
    "a.b-c_d@sub.example.co.uk",
    "root@localhost",
];
const MONTHS: &[&str] = &[
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];
const WEEKDAYS: &[&str] = &["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
const WORDS: &[&str] = &[
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

fn int<T>(lo: T, hi: T) -> gs::IntegerGenerator<T>
where
    T: gs::Integer,
{
    gs::integers::<T>().min_value(lo).max_value(hi)
}

fn pick<'a>(tc: &TestCase, items: &[&'a str]) -> &'a str {
    items[tc.draw_silent(int(0usize, items.len() - 1))]
}

fn chars_from(tc: &TestCase, alphabet: &str, n: usize) -> String {
    let a: Vec<char> = alphabet.chars().collect();
    (0..n)
        .map(|_| a[tc.draw_silent(int(0usize, a.len() - 1))])
        .collect()
}

fn arb_package(tc: &TestCase) -> String {
    match tc.draw_silent(int(0u8, 9)) {
        0 => pick(tc, PACKAGES).to_string(),
        _ => {
            let first = chars_from(tc, "abcdefgxyz", 1);
            let n = tc.draw_silent(int(0usize, 8));
            let rest = chars_from(tc, "abcdefghijklmnopqrstuvwxyz0123456789.+-", n);
            format!("{first}{rest}")
        }
    }
}

fn arb_version(tc: &TestCase) -> String {
    match tc.draw_silent(int(0u8, 3)) {
        0..=1 => pick(tc, VERSIONS).to_string(),
        _ => {
            let mut s = String::new();
            if tc.draw_silent(int(0u8, 3)) == 0 {
                s.push_str(&format!("{}:", tc.draw_silent(int(1u32, 9))));
            }
            s.push_str(&tc.draw_silent(int(0u32, 99)).to_string());
            for _ in 0..tc.draw_silent(int(0usize, 3)) {
                s.push('.');
                s.push_str(&tc.draw_silent(int(0u32, 99)).to_string());
            }
            if tc.draw_silent(int(0u8, 3)) == 0 {
                s.push_str(pick(
                    tc,
                    &["~rc1", "+dfsg", "+git20240101", "~beta", "a", "+really1.0"],
                ));
            }
            if tc.draw_silent(int(0u8, 2)) != 0 {
                s.push('-');
                s.push_str(pick(
                    tc,
                    &[
                        "1",
                        "0ubuntu1",
                        "2+b1",
                        "1~bpo12+1",
                        "3+nmu1",
                        "0",
                        "1+deb12u2",
                    ],
                ));
            }
            s
        }
    }
}

fn arb_distributions(tc: &TestCase) -> Vec<String> {
    let n = tc.draw_silent(int(1usize, 3));
    (0..n)
        .map(|_| match tc.draw_silent(int(0u8, 4)) {
            0 => {
                let n = tc.draw_silent(int(1usize, 10));
                chars_from(tc, "abcdefghijklmnopqrstuvwxyz0123456789.+-", n)
            }
            _ => pick(tc, DISTS).to_string(),
        })
        .collect()
}

fn arb_metadata_item(tc: &TestCase) -> (String, String) {
    match tc.draw_silent(int(0u8, 9)) {
        0..=5 => {
            let mut u = pick(tc, &["low", "medium", "high", "emergency", "critical"]).to_string();
            if tc.draw_silent(int(0u8, 5)) == 0 {
                u = match tc.draw_silent(int(0u8, 2)) {
                    0 => u.to_uppercase(),
                    1 => {
                        let mut c = u.chars();
                        c.next().unwrap().to_uppercase().collect::<String>() + c.as_str()
                    }
                    _ => u,
                };
            }
            ("urgency".to_string(), u)
        }
        6..=7 => ("binary-only".to_string(), "yes".to_string()),
        _ => {
            let k = pick(tc, &["XS-Foo", "xbs-bar", "XC-Vcs-Git", "xs-a"]).to_string();
            let n = tc.draw_silent(int(1usize, 6));
            (
                k,
                chars_from(tc, "abcdefghijklmnopqrstuvwxyz0123456789.-", n),
            )
        }
    }
}

fn arb_bug_list(tc: &TestCase) -> String {
    let n = tc.draw_silent(int(1usize, 3));
    let mut s = String::new();
    for i in 0..n {
        if i > 0 {
            s.push_str(pick(tc, &[", ", ",", " , ", ",  ", " ", ",, "]));
        }
        s.push_str(pick(
            tc,
            &["#", "#", "#", "", "# ", "bug#", "Bug #", "bug "],
        ));
        s.push_str(&tc.draw_silent(int(1u32, 9_999_999)).to_string());
    }
    s
}

fn arb_detail(tc: &TestCase, bullet: bool) -> String {
    let mut s = String::new();
    if bullet {
        s.push_str(pick(tc, &["* ", "* ", "* ", "+ ", "- ", "  + ", "*"]));
    } else if tc.draw_silent(int(0u8, 2)) == 0 {
        s.push_str("  ");
    }
    let n = tc.draw_silent(int(0usize, 6));
    for i in 0..n {
        if i > 0 {
            s.push(' ');
        }
        s.push_str(pick(tc, WORDS));
    }
    match tc.draw_silent(int(0u8, 9)) {
        0..=2 => {
            let marker = pick(
                tc,
                &[
                    "Closes: ",
                    "Closes:",
                    "closes: ",
                    "CLOSES: ",
                    "(Closes: ",
                    "Closes:  ",
                    "LP: ",
                    "lp: ",
                    "LP:",
                    "LP: #",
                    "(LP: ",
                ],
            );
            let list = arb_bug_list(tc);
            s.push_str(&format!(" {marker}{list}"));
            if marker.starts_with('(') {
                s.push(')');
            }
            if tc.draw_silent(int(0u8, 3)) == 0 {
                s.push(',');
            }
        }
        3 => s.push_str(pick(
            tc,
            &[
                " Closes: #",
                " Closes:",
                " LP:",
                " Fixes: #123",
                " closes bug 5",
                " Encloses: #7",
                " lp:#12",
                " lp: 34",
            ],
        )),
        _ => {}
    }
    s
}

fn arb_body(tc: &TestCase) -> Vec<String> {
    let n = tc.draw_silent(int(0usize, 6));
    let mut lines = Vec::new();
    for i in 0..n {
        match tc.draw_silent(int(0u8, 9)) {
            0 if i > 0 => lines.push(String::new()),
            1 => {
                let k = tc.draw_silent(int(1usize, 2));
                let name: Vec<&str> = (0..k).map(|_| pick(tc, NAMES)).collect();
                lines.push(format!("  [ {} ]", name.join(" ")));
            }
            2 if i > 0 => lines.push(format!("    {}", arb_detail(tc, false))),
            _ => lines.push(format!("  {}", arb_detail(tc, true))),
        }
    }
    lines
}

fn arb_name(tc: &TestCase) -> String {
    let n = tc.draw_silent(int(0usize, 3));
    (0..n)
        .map(|_| pick(tc, NAMES))
        .collect::<Vec<_>>()
        .join(" ")
}

fn arb_date(tc: &TestCase) -> String {
    let year = tc.draw_silent(int(1998i32, 2030));
    let month = tc.draw_silent(int(0usize, 11));
    let day = tc.draw_silent(int(1u32, 28));
    let weekday = match tc.draw_silent(int(0u8, 9)) {
        0 => None,
        1 => Some(pick(tc, WEEKDAYS).to_string()),
        _ => Some("Mon".to_string()),
    };
    let hour = tc.draw_silent(int(0u32, 23));
    let minute = tc.draw_silent(int(0u32, 59));
    let second = tc.draw_silent(int(0u32, 59));
    let tz_sign = if tc.draw_silent(gs::booleans()) {
        '+'
    } else {
        '-'
    };
    let tz = [0, 0, 100, 530, 1200, 45, 1400, 930][tc.draw_silent(int(0usize, 7))];
    let pad_day = tc.draw_silent(int(0u8, 4)) != 0;
    let day = if pad_day {
        format!("{day:02}")
    } else {
        day.to_string()
    };
    let mut s = String::new();
    if let Some(w) = weekday {
        s.push_str(&w);
        s.push_str(", ");
    }
    s.push_str(&format!(
        "{day} {} {year} {hour:02}:{minute:02}:{second:02} {tz_sign}{tz:04}",
        MONTHS[month]
    ));
    s
}

struct Entry {
    text: String,
    package: String,
    dists: Vec<String>,
    urgency_comment: bool,
    malformed: bool,
}

fn arb_entry(tc: &TestCase) -> Entry {
    let package = arb_package(tc);
    let version = arb_version(tc);
    let dists = arb_distributions(tc);
    let nmeta = tc.draw_silent(int(0usize, 3));
    let mut metadata: Vec<(String, String)> = Vec::new();
    for _ in 0..nmeta {
        let item = arb_metadata_item(tc);
        if !metadata
            .iter()
            .any(|(k, _)| k.eq_ignore_ascii_case(&item.0))
        {
            metadata.push(item);
        }
    }
    let urgency_comment =
        metadata.iter().any(|(k, _)| k == "urgency") && tc.draw_silent(int(0u8, 14)) == 0;
    let body = arb_body(tc);
    let name = arb_name(tc);
    let email = pick(tc, EMAILS).to_string();
    let date = arb_date(tc);
    let malformed = match tc.draw_silent(int(0u8, 24)) {
        0 => Some("no-distribution"),
        1 => Some("no-semicolon"),
        2 => Some("no-version"),
        3 => Some("one-space-before-date"),
        4 => Some("no-footer"),
        _ => None,
    };

    let mut text = String::new();
    text.push_str(&package);
    if malformed != Some("no-version") {
        text.push_str(&format!(" ({version})"));
    }
    if malformed != Some("no-distribution") {
        for d in &dists {
            text.push(' ');
            text.push_str(d);
        }
    }
    if malformed != Some("no-semicolon") {
        text.push(';');
        for (i, (k, v)) in metadata.iter().enumerate() {
            if i > 0 {
                text.push_str(pick(tc, &[", ", ",", " , "]));
            } else {
                text.push_str(pick(tc, &[" ", " ", ""]));
            }
            text.push_str(&format!("{k}={v}"));
            if k == "urgency" && urgency_comment {
                text.push_str(" (HIGH for security)");
            }
        }
    }
    text.push('\n');
    for _ in 0..tc.draw_silent(int(0usize, 2)) {
        text.push('\n');
    }
    for l in &body {
        text.push_str(l);
        text.push('\n');
    }
    for _ in 0..tc.draw_silent(int(0usize, 2)) {
        text.push('\n');
    }
    if malformed != Some("no-footer") {
        text.push_str(" -- ");
        text.push_str(&name);
        text.push_str(&format!(" <{email}>"));
        text.push_str(if malformed == Some("one-space-before-date") {
            " "
        } else {
            "  "
        });
        text.push_str(&date);
        if tc.draw_silent(int(0u8, 9)) == 0 {
            text.push(' ');
        }
        text.push('\n');
    }
    Entry {
        text,
        package,
        dists,
        urgency_comment,
        malformed: malformed.is_some(),
    }
}

#[derive(Debug, PartialEq, Eq)]
struct Draws {
    text: String,
    qualifies: bool,
}

fn draw(tc: &TestCase) -> Draws {
    let n = tc.draw_silent(int(1usize, 3));
    let mut first: Option<Entry> = None;
    for i in 0..n {
        if i > 0 {
            tc.draw_silent(int(0usize, 2));
        }
        let e = arb_entry(tc);
        if first.is_none() {
            first = Some(e);
        }
    }
    tc.draw_silent(int(0u8, 9));
    let first = first.unwrap();
    let qualifies = qualifies(&first);
    if qualifies {
        arb_detail(tc, false);
    }
    Draws {
        text: first.text,
        qualifies,
    }
}

fn hits_prepend_bug(entry_text: &str) -> bool {
    let blanks = entry_text
        .lines()
        .skip(1)
        .take_while(|l| l.trim().is_empty())
        .count();
    let has_body = entry_text
        .lines()
        .skip(1)
        .any(|l| l.starts_with("  ") && !l.trim().is_empty());
    has_body && blanks != 1
}

fn qualifies(e: &Entry) -> bool {
    let clean = !e.malformed && !e.urgency_comment;
    let plus = e.package.contains('+') || e.dists.iter().any(|d| d.contains('+'));
    clean && !plus && hits_prepend_bug(&e.text)
}

fn prepend_bug_fires(d: &Draws) -> bool {
    d.qualifies
}

fn ideal() -> Draws {
    Draws {
        text: "gtkmm3.0 (1.0-1) unstable;\n  [ Jelmer ]\n --  <jelmer@debian.org>  1 Jan 1998 00:00:00 -0000 \n".to_string(),
        qualifies: true,
    }
}

#[test]
fn the_ideal_fails_and_is_smallest() {
    assert!(prepend_bug_fires(&ideal()));
    assert!(hits_prepend_bug(&ideal().text));
    assert!(!hits_prepend_bug(
        "gtkmm3.0 (1.0-1) unstable;\n\n  [ Jelmer ]\n --  <jelmer@debian.org>  1 Jan 1998 00:00:00 -0000 \n"
    ));
    assert!(!hits_prepend_bug(
        "gtkmm3.0 (1.0-1) unstable;\n --  <jelmer@debian.org>  1 Jan 1998 00:00:00 -0000 \n"
    ));
    assert!(hits_prepend_bug(
        "a (1.0-1) unstable; urgency=low\n  [ Jelmer ]\n\n --  <jelmer@debian.org>  1 Jan 1998 00:00:00 -0000 \n"
    ));
}

#[test]
#[ignore = "shrinker: no pass deletes two separated regions at once"]
fn dead_header_metadata_is_deleted() {
    assert_shrinks_to(&ideal(), 30, 300, draw, prepend_bug_fires);
}
