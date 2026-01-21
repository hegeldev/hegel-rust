pub mod embedded;
pub mod gen;

pub use gen::Generate;

// Re-export for macro use
#[doc(hidden)]
pub use paste;

// re-export public api
pub use hegel_derive::Generate;
pub use embedded::{hegel, Hegel, Verbosity};

use gen::HegelMode;

/// Note a message which will be displayed with the reported failing test case.
pub fn note(message: &str) {
    gen::note(message)
}

/// Assume a condition is true. If false, reject the current test input.
pub fn assume(condition: bool) {
    if !condition {
        match gen::current_mode() {
            HegelMode::External => {
                let code: i32 = std::env::var("HEGEL_REJECT_CODE")
                    .expect("HEGEL_REJECT_CODE environment variable not set")
                    .parse()
                    .expect("HEGEL_REJECT_CODE must be a valid integer");

                std::process::exit(code);
            }
            HegelMode::Embedded => {
                panic!("HEGEL_REJECT");
            }
        }
    }
}
