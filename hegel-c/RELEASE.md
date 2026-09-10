RELEASE_TYPE: patch

This patch fixes a memory leak in string draws. The engine memoises, per alphabet, which of its built-in constant strings fit that alphabet, and that memo was kept in a process-global table that never dropped entries for freed generators. A caller that built and freed a string generator around every draw grew without bound; the memo now lives with the alphabet and is freed with it ([#434](https://github.com/hegeldev/hegel-rust/issues/434)).
