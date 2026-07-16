RELEASE_TYPE: minor

This release changes the numeric values of `hegel_verbosity_t` so that the default, `HEGEL_VERBOSITY_NORMAL`, is 0. A zero-initialized value now selects the default level
([#357](https://github.com/hegeldev/hegel-rust/issues/357)):

```c
/* before */
HEGEL_VERBOSITY_QUIET = 0, HEGEL_VERBOSITY_NORMAL = 1

/* after */
HEGEL_VERBOSITY_NORMAL = 0, HEGEL_VERBOSITY_QUIET = 1
```
