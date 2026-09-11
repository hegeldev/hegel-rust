//! A `#[derive(PrettyPrintable)]` type with one field whose type is not
//! `PrettyPrintable` (standing in for a foreign type like `taffy::Style`).
//! The diagnostic must point at the offending field and suggest
//! `#[pretty(debug)]`, not draw-site fixes.

#[derive(Debug, Clone)]
struct Style {
    flex_grow: f32,
}

#[derive(hegel::PrettyPrintable)]
struct TreeSpec {
    style: Style,
    children: Vec<TreeSpec>,
}

fn main() {}
