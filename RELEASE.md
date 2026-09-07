RELEASE_TYPE: patch

This patch improves the printability diagnostics and documentation for libraries that use hegel as a dev-dependency. The advice to add `#[derive(hegel::PrettyPrintable)]` to your own type cannot be followed in that setup, so the diagnostics now also point at invoking `hegel::pretty_print_as_debug!` from `#[cfg(test)]` code ([#448](https://github.com/hegeldev/hegel-rust/issues/448)).
