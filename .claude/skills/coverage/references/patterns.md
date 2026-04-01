# Coverage Patterns and Techniques

Detailed patterns for achieving 100% test coverage through better code design.

## Genuinely Unreachable Code

Code that should never execute under any circumstances.

**Fix**: Make it an explicit error.

```rust
// Bad: Silent unreachable code
fn process(state: State) -> Result<()> {
    match state {
        State::A => handle_a(),
        State::B => handle_b(),
        State::C => return Ok(()), // "Can't happen" - but coverage sees it
    }
}

// Good: Explicit unreachable
fn process(state: State) -> Result<()> {
    match state {
        State::A => handle_a(),
        State::B => handle_b(),
        State::C => unreachable!("State C is never created"),
    }
}
```

The `unreachable!()` macro documents intent and will panic if your assumption is wrong.

## Hard-to-Test Dependencies

Code that interacts with external systems (filesystem, network, time, environment).

**Fix**: Extract and inject dependencies.

### Extract Functions

```rust
// Bad: Monolithic function
fn deploy() -> Result<()> {
    let output = Command::new("git").args(["push"]).output()?;
    if !output.status.success() {
        return Err(Error::GitPushFailed);
    }
    // ... more logic
    Ok(())
}

// Good: Extract the testable logic
fn check_command_success(output: &Output) -> Result<()> {
    if output.status.success() {
        Ok(())
    } else {
        Err(Error::CommandFailed)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_check_command_success() {
        let output = Output { status: ExitStatus::from_raw(0), ... };
        assert!(check_command_success(&output).is_ok());
    }
}
```

### Create Traits for Dependency Injection

```rust
// Bad: Hardcoded dependency
fn get_current_time() -> DateTime<Utc> {
    Utc::now() // Can't test time-dependent logic
}

// Good: Inject the dependency
trait Clock {
    fn now(&self) -> DateTime<Utc>;
}

struct RealClock;
impl Clock for RealClock {
    fn now(&self) -> DateTime<Utc> { Utc::now() }
}

struct MockClock(DateTime<Utc>);
impl Clock for MockClock {
    fn now(&self) -> DateTime<Utc> { self.0 }
}

fn is_expired(clock: &impl Clock, expiry: DateTime<Utc>) -> bool {
    clock.now() > expiry
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_is_expired() {
        let mock = MockClock(Utc.with_ymd_and_hms(2024, 1, 1, 12, 0, 0).unwrap());
        let past = Utc.with_ymd_and_hms(2024, 1, 1, 11, 0, 0).unwrap();
        assert!(is_expired(&mock, past));
    }
}
```

### Parameterize Over Environment

For functions that read env vars, platform information, or global state, extract the logic into a parameterized version and leave a thin wrapper:

```rust
// Hard to test — reads env vars directly
fn cache_dir() -> PathBuf {
    if let Ok(xdg) = std::env::var("XDG_CACHE_HOME") {
        return PathBuf::from(xdg).join("myapp");
    }
    // ...
}

// Testable — takes values as parameters
fn cache_dir_from(xdg: Option<String>, home: Option<PathBuf>) -> PathBuf {
    if let Some(xdg) = xdg {
        return PathBuf::from(xdg).join("myapp");
    }
    // ...
}

// Thin wrapper calls the testable version
fn cache_dir() -> PathBuf {
    cache_dir_from(std::env::var("XDG_CACHE_HOME").ok(), std::env::home_dir())
}
```

### Manipulate PATH to Mock Commands

For code that shells out to external commands:

```rust
#[cfg(test)]
mod tests {
    use std::env;
    use std::fs;
    use tempfile::tempdir;

    fn setup_mock_git(dir: &Path, script: &str) {
        let git_path = dir.join("git");
        fs::write(&git_path, format!("#!/bin/sh\n{}", script)).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&git_path, fs::Permissions::from_mode(0o755)).unwrap();
        }
    }

    #[test]
    fn test_deploy_handles_git_failure() {
        let temp = tempdir().unwrap();
        setup_mock_git(temp.path(), "exit 1"); // Mock git to fail

        // Prepend our mock to PATH
        let original_path = env::var("PATH").unwrap_or_default();
        env::set_var("PATH", format!("{}:{}", temp.path().display(), original_path));

        let result = deploy();

        env::set_var("PATH", original_path); // Restore
        assert!(result.is_err());
    }
}
```

## Error Handling Branches

Error paths that are hard to trigger.

**Fix**: Design for testability.

```rust
// Bad: Can't test the error branch without IO errors — parsing is entangled with IO
fn read_and_parse(path: &Path) -> Result<Data> {
    let content = std::fs::read_to_string(path)?;
    parse(&content)
}

// Good: Separate IO from parsing
fn parse(content: &str) -> Result<Data> {
    // All parsing logic here - easy to test with bad input
}

fn read_and_parse(path: &Path) -> Result<Data> {
    let content = std::fs::read_to_string(path)?;
    parse(&content)
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_parse_invalid_input() {
        assert!(parse("not valid").is_err());
    }
}
```

## Common Anti-Patterns to Avoid

### Don't: Suppress with Annotations

```rust
// Bad: Hiding the problem
fn some_function() {
    if(error_condition()){
        return ...; // nocov
    }
}
```

Either figure out how to trigger the error condition in tests, or if the error is genuinely impossible to trigger, mark it unreachable.

### Don't: Mock Everything

```rust
// Bad: Testing mocks, not code
#[test]
fn test_with_all_mocks() {
    let mock_db = MockDb::new();
    let mock_http = MockHttp::new();
    let mock_fs = MockFs::new();
    // At this point, what are you even testing?
}
```
