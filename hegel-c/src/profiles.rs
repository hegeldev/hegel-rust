//! Named settings profiles.
//!
//! A profile is a named delta over the engine's base defaults
//! ([`Settings::base`]). Three profiles ship with the library:
//!
//! - `default`: the base defaults, unchanged.
//! - `ci`: extends `default` with `derandomize = true`, the database
//!   disabled, and `print_blob = true`. Selected automatically when a CI
//!   environment is detected.
//! - `antithesis`: extends `default` with the database disabled. Selected
//!   automatically inside Antithesis. Health checks and the urandom backend
//!   are driven by Antithesis *detection* rather than by this profile, so
//!   selecting a different profile inside Antithesis does not re-enable
//!   them.
//!
//! Users modify shipped profiles and define new ones in a `hegel.toml`
//! ([`crate::config`]), or register complete snapshots through the C ABI's
//! `hegel_settings_register_profile`. When no profile is named explicitly,
//! [`selected_name`] picks one: `HEGEL_DEFAULT_PROFILE` when set, otherwise
//! `antithesis` or `ci` by environment detection, otherwise `default`.

use alloc::borrow::ToOwned;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::config::{self, ConfigFile};
use crate::settings::{Backend, Database, HealthCheck, Phase, Settings, Verbosity};
use crate::sys::sync::{Lazy, Mutex};

/// The environment variable naming the profile to resolve when none is
/// requested explicitly. Overrides Antithesis and CI detection.
pub(crate) const DEFAULT_PROFILE_VAR: &str = "HEGEL_DEFAULT_PROFILE";

/// A named set of settings overrides: every profile-settable field, each
/// optional. Unset fields inherit from the parent profile (`extends`, or
/// `default` when absent); resolution bottoms out at [`Settings::base`].
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct ProfileDelta {
    /// The profile this one layers over. Only meaningful for profiles
    /// defined in `hegel.toml`; shipped and registered profiles have a
    /// fixed base and reject it.
    pub(crate) extends: Option<String>,
    pub(crate) test_cases: Option<u64>,
    pub(crate) verbosity: Option<Verbosity>,
    pub(crate) seed: Option<u64>,
    pub(crate) derandomize: Option<bool>,
    pub(crate) database: Option<Database>,
    pub(crate) suppress_health_check: Option<Vec<HealthCheck>>,
    pub(crate) phases: Option<Vec<Phase>>,
    pub(crate) report_multiple_failures: Option<bool>,
    pub(crate) show_statistics: Option<bool>,
    pub(crate) print_blob: Option<bool>,
    /// `Some(None)` pins the automatic backend choice (`"auto"`), undoing
    /// a parent's explicit selection.
    pub(crate) backend: Option<Option<Backend>>,
}

impl ProfileDelta {
    /// Overwrite each field of `settings` that this delta sets.
    pub(crate) fn apply(&self, settings: &mut Settings) {
        if let Some(v) = self.test_cases {
            settings.test_cases = v;
        }
        if let Some(v) = self.verbosity {
            settings.verbosity = v;
        }
        if let Some(v) = self.seed {
            settings.seed = Some(v);
        }
        if let Some(v) = self.derandomize {
            settings.derandomize = v;
        }
        if let Some(v) = &self.database {
            settings.database = v.clone();
        }
        if let Some(v) = &self.suppress_health_check {
            settings.suppress_health_check = v.clone();
        }
        if let Some(v) = &self.phases {
            settings.phases = v.clone();
        }
        if let Some(v) = self.report_multiple_failures {
            settings.report_multiple_failures = v;
        }
        if let Some(v) = self.show_statistics {
            settings.show_statistics = v;
        }
        if let Some(v) = self.print_blob {
            settings.print_blob = v;
        }
        if let Some(v) = self.backend {
            settings.backend = v;
        }
    }

    /// A delta that reproduces `settings` exactly when applied over any
    /// parent: the form registered profiles are stored in. A `None` seed is
    /// left unset rather than encoded, which is equivalent because
    /// [`Settings::base`] has no seed either.
    pub(crate) fn snapshot(settings: &Settings) -> Self {
        Self {
            extends: None,
            test_cases: Some(settings.test_cases),
            verbosity: Some(settings.verbosity),
            seed: settings.seed,
            derandomize: Some(settings.derandomize),
            database: Some(settings.database.clone()),
            suppress_health_check: Some(settings.suppress_health_check.clone()),
            phases: Some(settings.phases.clone()),
            report_multiple_failures: Some(settings.report_multiple_failures),
            show_statistics: Some(settings.show_statistics),
            print_blob: Some(settings.print_blob),
            backend: Some(settings.backend),
        }
    }
}

/// Why profile resolution failed. Rendered with `Display` into the
/// diagnostic reported through the C ABI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ProfileError {
    /// `hegel.toml` could not be parsed. `line` is 1-based; 0 means the
    /// error concerns the whole file.
    Config {
        path: String,
        line: usize,
        message: String,
    },
    UnknownProfile(String),
    UnknownExtends {
        profile: String,
        extends: String,
    },
    /// The `extends` chain revisited a profile; the vector holds the walk
    /// from the requested profile to the repeated name.
    ExtendsCycle(Vec<String>),
    /// A `hegel.toml` section set `extends` on a profile whose base is
    /// fixed: `kind` is `"shipped"` or `"registered"`.
    ExtendsNotAllowed {
        profile: String,
        kind: &'static str,
    },
    InvalidName(String),
}

impl core::fmt::Display for ProfileError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ProfileError::Config {
                path,
                line: 0,
                message,
            } => {
                write!(f, "{path}: {message}")
            }
            ProfileError::Config {
                path,
                line,
                message,
            } => {
                write!(f, "{path}:{line}: {message}")
            }
            ProfileError::UnknownProfile(name) => {
                write!(f, "unknown settings profile {name:?}")
            }
            ProfileError::UnknownExtends { profile, extends } => {
                write!(f, "profile {profile:?} extends unknown profile {extends:?}")
            }
            ProfileError::ExtendsCycle(chain) => {
                write!(f, "profile extends cycle: {}", chain.join(" -> "))
            }
            ProfileError::ExtendsNotAllowed { profile, kind } => {
                write!(f, "cannot set extends on {kind} profile {profile:?}")
            }
            ProfileError::InvalidName(name) => {
                write!(
                    f,
                    "invalid profile name {name:?}: profile names use only \
                     ASCII letters, digits, '-' and '_'"
                )
            }
        }
    }
}

/// Whether `name` is usable as a profile name, both for registration and
/// for `hegel.toml` section names.
pub(crate) fn is_valid_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// The deltas of the three shipped profiles.
static SHIPPED: Lazy<[(&'static str, ProfileDelta); 3]> = Lazy::new(|| {
    [
        ("default", ProfileDelta::default()),
        (
            "ci",
            ProfileDelta {
                derandomize: Some(true),
                database: Some(Database::Disabled),
                print_blob: Some(true),
                ..ProfileDelta::default()
            },
        ),
        (
            "antithesis",
            ProfileDelta {
                database: Some(Database::Disabled),
                ..ProfileDelta::default()
            },
        ),
    ]
});

fn shipped(name: &str) -> Option<&'static ProfileDelta> {
    SHIPPED.iter().find(|(n, _)| *n == name).map(|(_, d)| d)
}

/// Profiles registered through the C ABI, as full settings snapshots.
static REGISTRY: Lazy<Mutex<Vec<(String, ProfileDelta)>>> = Lazy::new(|| Mutex::new(Vec::new()));

/// Register `settings` as the complete snapshot of profile `name`,
/// replacing any earlier registration of the same name. Registering a
/// shipped name replaces its shipped delta as the resolution base for that
/// name; `hegel.toml` deltas still merge on top.
pub(crate) fn register(name: &str, settings: &Settings) -> Result<(), ProfileError> {
    if !is_valid_name(name) {
        return Err(ProfileError::InvalidName(name.to_owned()));
    }
    let delta = ProfileDelta::snapshot(settings);
    let mut registry = REGISTRY.lock();
    match registry.iter_mut().find(|(n, _)| n == name) {
        Some(entry) => entry.1 = delta,
        None => registry.push((name.to_owned(), delta)),
    }
    Ok(())
}

fn registry_snapshot() -> Vec<(String, ProfileDelta)> {
    REGISTRY.lock().clone()
}

fn lookup<'a>(entries: &'a [(String, ProfileDelta)], name: &str) -> Option<&'a ProfileDelta> {
    entries.iter().find(|(n, _)| n == name).map(|(_, d)| d)
}

/// The deltas making up profile `name`, topmost first: the walk from `name`
/// up its `extends` chain until a terminator (the `default` profile, or a
/// registered snapshot). At each name, a `hegel.toml` delta layers over the
/// shipped or registered delta of the same name.
fn delta_chain<'a>(
    name: &'a str,
    config: &'a ConfigFile,
    registry: &'a [(String, ProfileDelta)],
) -> Result<Vec<&'a ProfileDelta>, ProfileError> {
    let mut deltas = Vec::new();
    let mut chain: Vec<&str> = alloc::vec![name];
    let mut current = name;
    loop {
        let cfg = lookup(&config.profiles, current);
        let registered = lookup(registry, current);
        let ship = shipped(current);
        if let Some(delta) = cfg {
            if delta.extends.is_some() {
                let kind = match (registered.is_some(), ship.is_some()) {
                    (true, _) => Some("registered"),
                    (_, true) => Some("shipped"),
                    _ => None,
                };
                if let Some(kind) = kind {
                    return Err(ProfileError::ExtendsNotAllowed {
                        profile: current.to_owned(),
                        kind,
                    });
                }
            }
            deltas.push(delta);
        }
        if let Some(delta) = registered {
            deltas.push(delta);
            return Ok(deltas);
        }
        if let Some(delta) = ship {
            deltas.push(delta);
            if current == "default" {
                return Ok(deltas);
            }
            chain.push("default");
            current = "default";
            continue;
        }
        match cfg {
            None => {
                return Err(if chain.len() == 1 {
                    ProfileError::UnknownProfile(name.to_owned())
                } else {
                    ProfileError::UnknownExtends {
                        profile: chain[chain.len() - 2].to_owned(),
                        extends: current.to_owned(),
                    }
                });
            }
            Some(delta) => {
                let parent = delta.extends.as_deref().unwrap_or("default");
                if chain.contains(&parent) {
                    let mut cycle: Vec<String> = chain.iter().map(|n| n.to_string()).collect();
                    cycle.push(parent.to_owned());
                    return Err(ProfileError::ExtendsCycle(cycle));
                }
                chain.push(parent);
                current = parent;
            }
        }
    }
}

/// Resolve profile `name` against `config` and `registry`, starting from
/// `base`.
pub(crate) fn resolve(
    name: &str,
    config: &ConfigFile,
    registry: &[(String, ProfileDelta)],
    base: &Settings,
) -> Result<Settings, ProfileError> {
    let deltas = delta_chain(name, config, registry)?;
    let mut settings = base.clone();
    for delta in deltas.iter().rev() {
        delta.apply(&mut settings);
    }
    Ok(settings)
}

/// Check every profile `config` defines, not just the one about to be
/// resolved, so a cycle or dangling `extends` in an unused profile fails
/// loudly instead of lingering until someone selects it.
fn validate(config: &ConfigFile, registry: &[(String, ProfileDelta)]) -> Result<(), ProfileError> {
    for (name, _) in &config.profiles {
        delta_chain(name, config, registry)?;
    }
    Ok(())
}

/// The profile to resolve when none is named explicitly:
/// `HEGEL_DEFAULT_PROFILE` when set and non-empty, then `antithesis` or
/// `ci` when the corresponding environment is detected, then `default`.
pub(crate) fn selected_name(env: impl Fn(&str) -> Option<String>) -> String {
    if let Some(name) = env(DEFAULT_PROFILE_VAR) {
        if !name.is_empty() {
            return name;
        }
    }
    if crate::antithesis_detect::antithesis_env_var_set_from(&env) {
        "antithesis".to_owned()
    } else if crate::settings::is_in_ci_from(&env) {
        "ci".to_owned()
    } else {
        "default".to_owned()
    }
}

/// Resolve settings for the profile `name`, or for the environment-selected
/// profile when `name` is `None`. The entry point behind
/// `hegel_settings_new` and `hegel_settings_new_for_profile`.
pub(crate) fn settings_for(name: Option<&str>) -> Result<Settings, ProfileError> {
    let config = config::load()?;
    let registry = registry_snapshot();
    settings_for_from(name, &config, &registry, crate::sys::env_var)
}

/// [`settings_for`] with the config, registry, and environment injected.
fn settings_for_from(
    name: Option<&str>,
    config: &ConfigFile,
    registry: &[(String, ProfileDelta)],
    env: impl Fn(&str) -> Option<String>,
) -> Result<Settings, ProfileError> {
    validate(config, registry)?;
    let selected = match name {
        Some(n) => n.to_owned(),
        None => selected_name(&env),
    };
    let base = Settings::base(crate::antithesis_detect::antithesis_env_var_set_from(&env));
    let mut settings = resolve(&selected, config, registry, &base)?;
    settings.config_path = config.path.clone();
    Ok(settings)
}

#[cfg(test)]
#[path = "../tests/embedded/profiles_tests.rs"]
mod tests;
