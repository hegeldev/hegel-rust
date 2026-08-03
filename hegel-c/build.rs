fn main() {
    // Mark the cdylib as non-deletable so `dlclose` keeps it resident for the
    // rest of the process's life.
    //
    // A Rust cdylib that an embedder `dlopen`s and later `dlclose`s is unsafe to
    // unload while any thread that called into it is still alive. Running the
    // engine (in particular persisting to the on-disk database, which pulls in
    // `tempfile`/`fastrand`) touches `std` thread-locals whose *destructors* —
    // and the global panic hook — are code inside this library. glibc records
    // those thread-local destructors in the process-global `__pthread_keys` and
    // runs them when the thread exits. If the library has been `dlclose`d and
    // unmapped by then, the destructor pointer dangles and the process
    // segfaults during thread teardown (see the `libhegel_replays_persisted_
    // failure_with_same_database_key` smoke test).
    //
    // `-z nodelete` turns `dlclose` into a no-op for this library: it stays
    // mapped, so those destructors remain valid for the process's lifetime. The
    // flag is ELF-specific (Linux/Android); on macOS and Windows a `dlopen`ed
    // dynamic library is not eagerly unmapped in the same way, so no equivalent
    // is needed. It applies only to the cdylib artifact, leaving the
    // `staticlib` and `rlib` untouched.
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os == "linux" || target_os == "android" {
        println!("cargo:rustc-cdylib-link-arg=-Wl,-z,nodelete");
    }
}
