# Changelog

## 0.3.0 - 2026-03-12

This release bumps the Hegel core version to 0.4.0.

## 0.2.5 - 2026-03-11

Set `HEGEL_SERVER_COMMAND` in nix install.

## 0.2.4 - 2026-03-11

Fixes a version mismatch with the hegel-core dependency that was causing the entire library to be broken.

Also improves error reporting to not silently swallow panics outside tests and to log server stdout to a file.

## 0.2.3 - 2026-03-10

Add argument validation.

## 0.2.2 - 2026-03-10

Update references from `hegel::hegel` to `#[hegel::test]`.

## 0.2.1 - 2026-03-10

Remove `with_*` prefix from builder methods.

## 0.2.0 - 2026-03-09

Automatically manage hegel server installation. Adds a runtime requirement on `uv`.

## 0.1.13 - 2026-03-06

Add support for `i128`, `u128`, and `isuze` in `generators::integers`.

## 0.1.12 - 2026-03-02

Support new hegel protocol versions.


## 0.1.11 - 2026-03-01

`#[hegel::test]` now automatically adds `#[test]`, and errors if used in combination with an explicit `#[test]` macro.

## 0.1.10 - 2026-03-01

Refactor some source code layout.

## 0.1.9 - 2026-03-01

Better error message when using `assume()`, `note()`, or `draw()` outside of a Hegel test.

## 0.1.8 - 2026-02-27

Add support for `hegel::arrays` and `hegel::tuples3` through `hegel::tuples12`.

## 0.1.7 - 2026-02-27

Add the `#[hegel::test]` macro as an ergonomic way to declare a hegel test.

## 0.1.6 - 2026-02-27

Minor code style cleanup: elide unnecessary named lifetimes.

## 0.1.5 - 2026-02-26

Rename the `gen` module to `generators`, avoiding a conflict with rust edition 2024, which made `gen` a reserved keyword.

## 0.1.4 - 2026-02-26

Refactor internals for better encapsulation of per-test-case state.

## 0.1.3 - 2026-02-25

Change how to draw a value from a generator from `generator.generate()` to `hegel::draw(generator)`.

## 0.1.2 - 2026-02-25

This patch adds support for setting `seed` as an option to `hegel`.

## 0.1.1 - 2026-02-24

Add `gen::from_type`, for use together with `#[derive(Generate)]`.

