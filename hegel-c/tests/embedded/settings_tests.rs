use super::*;

static CI_ENV_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn default_is_new() {
    let d = Settings::default();
    let n = Settings::new();
    assert_eq!(d.test_cases, n.test_cases);
    assert_eq!(d.mode, n.mode);
}

#[test]
fn resolved_backend_picks_urandom_under_antithesis() {
    assert_eq!(
        Settings::new()
            .backend(Backend::Default)
            .resolved_backend(true),
        Backend::Default
    );
    assert_eq!(Settings::new().resolved_backend(true), Backend::Urandom);
    assert_eq!(Settings::new().resolved_backend(false), Backend::Default);
}

#[test]
fn new_disables_database_in_ci() {
    let _guard = CI_ENV_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let key = "TEAMCITY_VERSION";
    let had_key = std::env::var_os(key);
    unsafe {
        std::env::set_var(key, "1");
    }
    let settings = Settings::new();
    match had_key {
        Some(v) => unsafe { std::env::set_var(key, v) },
        None => unsafe { std::env::remove_var(key) },
    }
    assert!(matches!(settings.database, Database::Disabled));
    assert!(settings.derandomize);
}
