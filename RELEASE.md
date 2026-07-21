RELEASE_TYPE: minor

This release removes the `antithesis` cargo feature. The Antithesis integration is now always compiled in and activates automatically at runtime when running inside Antithesis (detected via `ANTITHESIS_OUTPUT_DIR`). If you previously enabled the feature, remove it from your `Cargo.toml`:

```toml
# before
hegeltest = { version = "...", features = ["antithesis"] }

# after
hegeltest = "..."
```

Running inside Antithesis without the feature enabled used to be a startup error; that error is gone, and there is nothing left to configure.

This release also adds two new events to the Antithesis integration, both written to Antithesis's SDK message channel (`$ANTITHESIS_OUTPUT_DIR/sdk.jsonl`):

- In stateful testing, a `hegel_strategy_state` event is emitted immediately before each rule is drawn, making the moment at which the next rule is chosen distinguishable to the Antithesis fuzzer as a strategy state.
- In `Mode::SingleTestCase`, a test case that is marked invalid (a failed assumption) emits a `hegel_soft_terminate` event, telling the environment that nothing further of interest will happen on this branch.

Outside Antithesis nothing is emitted and behaviour is unchanged.
