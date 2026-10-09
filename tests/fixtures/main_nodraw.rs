//! Fixture binary: a `#[hegel::main]` body that draws nothing at all, for
//! the fuzz server, which can only serve a request at a draw.

use hegel::TestCase;

#[hegel::main]
fn main(_tc: TestCase) {}
