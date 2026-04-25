RELEASE_TYPE: minor

This release makes several changes to `#[derive(DefaultGenerator)]` ([#149](https://github.com/hegeldev/hegel-rust/issues/149)):

- Tuple variants now take field generators directly as positional arguments
- Named variants use a closure that receives the default variant generator:
- Generated method names are converted to snake_case instead of preserving PascalCase
  - If this would produce a name collision, we keep the original casing for both method names.

```rust
enum Op {
    Reset,
    ReadWrite(usize, usize),
    Configure { retries: u32, timeout: u64 },
}

// before
let g = Op::default_generator();
Op::default_generator()
    .ReadWrite(g.default_ReadWrite().value_0(gs::just(42)).value_1(gs::just(43)))
    .Configure(g.default_Configure().retries(gs::just(44)))

// after
Op::default_generator()
    .read_write(gs::just(42), gs::just(43))
    .configure(|g| g.retries(gs::just(44)))
```

Thanks to Rain for providing feedback on enum ergonomics!

This release also implements `DefaultGenerator` for `PathBuf`.
