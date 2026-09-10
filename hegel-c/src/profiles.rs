//! Named settings profiles.
//!
//! A profile is a named delta over the engine's base settings
//! ([`Settings::base`]). Two names are reserved:
//!
//! - `base`: the base settings themselves, set once and immutable. A chain
//!   that reaches `base` terminates, so selecting or extending it pins
//!   settings to the plain base, independent of the environment.
//! - `default`: the default profile, the one in effect when nothing names
//!   a profile. It is an alias whose candidates, in order, are the named
//!   default (the strongest set of the process override
//!   ([`set_default_profile`]), `HEGEL_DEFAULT_PROFILE`, and the `default`
//!   entry in `hegel.toml`) and the environment's profile (`antithesis`
//!   inside Antithesis, else `ci` on a CI server, else `development`). The
//!   alias resolves to the first candidate not already part of the chain
//!   being resolved, falling back to `base`. Resolving no name at all
//!   resolves `default`, and a custom profile without `extends` extends
//!   `default`, so it picks up the environment's behaviour wherever it
//!   sits. The shipped profiles themselves extend `base` unless a
//!   `hegel.toml` section says otherwise: they are siblings, never layers
//!   over one another, and a delta shared between them must be a profile
//!   they name with `extends`.
//!
//! Three ordinary profiles ship with the library and can be customized in
//! `hegel.toml` or replaced by registration like any other:
//!
//! - `development`: an empty delta, the environment profile of local runs.
//! - `ci`: `derandomize = true`, the database disabled, the `too_slow`
//!   health check suppressed, and `print_blob = true`.
//! - `antithesis`: the database disabled and every health check
//!   suppressed, since Antithesis's thread pausing would trip wall-clock
//!   checks such as `too_slow` spuriously. Like any profile setting these
//!   can be changed in `hegel.toml`, and resolving a profile that does not
//!   extend `antithesis` inside Antithesis runs the health checks. The
//!   urandom backend is driven by Antithesis detection (`backend = "auto"`),
//!   not by this profile.
//!
//! Users modify shipped profiles and define new ones in a `hegel.toml`
//! ([`crate::config`]), or register complete snapshots through the C ABI's
//! `hegel_settings_register_profile`.

use alloc::borrow::ToOwned;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::config::{self, ConfigFile};
use crate::settings::{Backend, Database, HealthCheck, Phase, Settings, Verbosity};
use crate::sys::sync::{Lazy, Mutex};

/// The environment variable naming the default profile. Overridden by
/// [`set_default_profile`]; overrides the `hegel.toml` entry and
/// environment detection.
pub(crate) const DEFAULT_PROFILE_VAR: &str = "HEGEL_DEFAULT_PROFILE";

/// The reserved name of the immutable base profile.
pub(crate) const BASE: &str = "base";

/// The reserved name of the default profile, an alias resolved through
/// [`Candidates`].
pub(crate) const DEFAULT: &str = "default";

/// The environment profile when nothing is detected: what local runs get.
const FALLBACK: &str = "development";

/// A named set of settings overrides: every profile-settable field, each
/// optional. Unset fields inherit from the parent profile: `extends`, or
/// when absent the `default` alias for custom profiles and `base` for
/// shipped ones. Resolution bottoms out at [`Settings::base`].
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct ProfileDelta {
    /// The profile this one layers over. Only meaningful for profiles
    /// defined in `hegel.toml`; registered profiles are complete snapshots
    /// and reject it.
    pub(crate) extends: Option<String>,
    pub(crate) test_cases: Option<u64>,
    pub(crate) verbosity: Option<Verbosity>,
    /// `Some(None)` unsets a parent's seed.
    pub(crate) seed: Option<Option<u64>>,
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
            settings.seed = v;
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
    /// parent: the form registered profiles are stored in.
    pub(crate) fn snapshot(settings: &Settings) -> Self {
        Self {
            extends: None,
            test_cases: Some(settings.test_cases),
            verbosity: Some(settings.verbosity),
            seed: Some(settings.seed),
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
    /// The named profile does not exist. `source` names what asked for it
    /// when the name came from a default-profile setting rather than the
    /// caller; `known` lists the resolvable profile names.
    UnknownProfile {
        name: String,
        source: Option<&'static str>,
        known: Vec<String>,
    },
    UnknownExtends {
        profile: String,
        extends: String,
    },
    /// The `extends` chain revisited a profile; the vector holds the walk
    /// from the requested profile to the repeated name.
    ExtendsCycle(Vec<String>),
    /// A `hegel.toml` section set `extends` on a registered profile, whose
    /// snapshot already terminates the chain.
    ExtendsOnRegistered(String),
    /// A default-profile setting named the `default` alias it resolves.
    /// `source` is the setting that named it.
    CircularDefault {
        source: &'static str,
    },
    ReservedName(String),
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
            ProfileError::UnknownProfile {
                name,
                source,
                known,
            } => {
                write!(f, "unknown settings profile {name:?}")?;
                if let Some(source) = source {
                    write!(f, " (named by {source})")?;
                }
                write!(f, "; known profiles: {}", known.join(", "))
            }
            ProfileError::UnknownExtends { profile, extends } => {
                write!(f, "profile {profile:?} extends unknown profile {extends:?}")
            }
            ProfileError::ExtendsCycle(chain) => {
                write!(f, "profile extends cycle: {}", chain.join(" -> "))
            }
            ProfileError::ExtendsOnRegistered(profile) => {
                write!(
                    f,
                    "cannot set extends on registered profile {profile:?}: \
                     a registered profile is a complete snapshot"
                )
            }
            ProfileError::CircularDefault { source } => {
                write!(f, "{source} cannot name the {DEFAULT:?} alias it resolves")
            }
            ProfileError::ReservedName(name) => {
                write!(f, "cannot register reserved profile name {name:?}")
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
        (FALLBACK, ProfileDelta::default()),
        (
            "ci",
            ProfileDelta {
                derandomize: Some(true),
                database: Some(Database::Disabled),
                suppress_health_check: Some(alloc::vec![HealthCheck::TooSlow]),
                print_blob: Some(true),
                ..ProfileDelta::default()
            },
        ),
        (
            "antithesis",
            ProfileDelta {
                database: Some(Database::Disabled),
                suppress_health_check: Some(alloc::vec![
                    HealthCheck::FilterTooMuch,
                    HealthCheck::TooSlow,
                    HealthCheck::TestCasesTooLarge,
                    HealthCheck::LargeInitialTestCase,
                ]),
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

/// The process-wide default-profile override, the first alias candidate.
/// Set through the C ABI's `hegel_set_default_profile`.
static DEFAULT_OVERRIDE: Lazy<Mutex<Option<String>>> = Lazy::new(|| Mutex::new(None));

/// Register `settings` as the complete snapshot of profile `name`,
/// replacing any earlier registration of the same name. Registering a
/// shipped name replaces its shipped delta as the resolution base for that
/// name; `hegel.toml` deltas still merge on top.
pub(crate) fn register(name: &str, settings: &Settings) -> Result<(), ProfileError> {
    if name == BASE || name == DEFAULT {
        return Err(ProfileError::ReservedName(name.to_owned()));
    }
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

/// Set (or with `None` clear) the process-wide default profile, the
/// strongest alias candidate. The entry point behind
/// `hegel_set_default_profile`.
pub(crate) fn set_default_profile(name: Option<&str>) -> Result<(), ProfileError> {
    if let Some(name) = name {
        if name == DEFAULT {
            return Err(ProfileError::CircularDefault {
                source: "hegel_set_default_profile",
            });
        }
        if !is_valid_name(name) {
            return Err(ProfileError::InvalidName(name.to_owned()));
        }
    }
    *DEFAULT_OVERRIDE.lock() = name.map(str::to_owned);
    Ok(())
}

fn registry_snapshot() -> Vec<(String, ProfileDelta)> {
    REGISTRY.lock().clone()
}

fn lookup<'a>(entries: &'a [(String, ProfileDelta)], name: &str) -> Option<&'a ProfileDelta> {
    entries.iter().find(|(n, _)| n == name).map(|(_, d)| d)
}

/// The names [`resolve`] accepts, for unknown-profile diagnostics: `base`
/// and every shipped, configured, and registered profile, sorted.
fn known_names(config: &ConfigFile, registry: &[(String, ProfileDelta)]) -> Vec<String> {
    let mut known: Vec<String> = alloc::vec![BASE.to_owned()];
    known.extend(SHIPPED.iter().map(|(n, _)| (*n).to_owned()));
    known.extend(config.profiles.iter().map(|(n, _)| n.clone()));
    known.extend(registry.iter().map(|(n, _)| n.clone()));
    known.sort_unstable();
    known.dedup();
    known
}

/// The candidates the `default` alias resolves through, strongest first.
pub(crate) struct Candidates {
    overridden: Option<String>,
    env: Option<String>,
    toml: Option<String>,
    /// The environment's profile: `antithesis` or `ci` by detection, else
    /// `development`.
    environment: &'static str,
}

impl Candidates {
    fn gather(
        config: &ConfigFile,
        overridden: Option<String>,
        env: impl Fn(&str) -> Option<String>,
    ) -> Self {
        Candidates {
            overridden,
            env: env(DEFAULT_PROFILE_VAR).filter(|v| !v.is_empty()),
            toml: config.default.clone(),
            environment: if crate::antithesis_detect::antithesis_env_var_set_from(&env) {
                "antithesis"
            } else if crate::settings::is_in_ci_from(&env) {
                "ci"
            } else {
                FALLBACK
            },
        }
    }

    /// A `Candidates` with the volatile process settings (the override and
    /// `HEGEL_DEFAULT_PROFILE`) stripped, for validation: the structure of
    /// the config must be sound regardless of what they name.
    fn structural(&self) -> Self {
        Candidates {
            overridden: None,
            env: None,
            toml: self.toml.clone(),
            environment: self.environment,
        }
    }

    /// The default-profile setting in effect: the strongest of the process
    /// override, the environment variable, and the `hegel.toml` entry. The
    /// weaker settings are displaced entirely, not kept as fallbacks.
    fn named_default(&self) -> Result<Option<(&str, &'static str)>, ProfileError> {
        let named = [
            (self.overridden.as_deref(), "hegel_set_default_profile"),
            (self.env.as_deref(), DEFAULT_PROFILE_VAR),
            (self.toml.as_deref(), "the default entry in hegel.toml"),
        ];
        for (candidate, source) in named {
            let Some(name) = candidate else { continue };
            if name == DEFAULT {
                return Err(ProfileError::CircularDefault { source });
            }
            return Ok(Some((name, source)));
        }
        Ok(None)
    }

    /// The name the `default` alias resolves to within `chain`: the named
    /// default ([`Candidates::named_default`]) if not already in the chain,
    /// else the environment's profile if not already in the chain, else
    /// `base`. The environment profiles never layer over one another, so
    /// every chain roots in the base settings. The source accompanying the
    /// name feeds unknown-profile diagnostics.
    fn resolve(&self, chain: &[&str]) -> Result<(&str, Option<&'static str>), ProfileError> {
        if let Some((name, source)) = self.named_default()? {
            if !chain.contains(&name) {
                return Ok((name, Some(source)));
            }
        }
        if !chain.contains(&self.environment) {
            return Ok((self.environment, None));
        }
        Ok((BASE, None))
    }
}

/// How the walk arrived at the profile it is about to look up, for
/// unknown-name diagnostics.
enum Arrival<'a> {
    /// Named directly by the caller.
    Requested,
    /// Named by a default-profile setting while resolving the alias.
    Default(&'static str),
    /// Named by the `extends` of the given profile.
    Extends(&'a str),
}

/// The deltas making up profile `name`, topmost first: the walk from `name`
/// through `extends` links and `default` aliases until a terminator
/// (`base`, or a registered snapshot). At each name, a `hegel.toml`
/// delta layers over the shipped or registered delta of the same name.
fn delta_chain<'a>(
    name: &'a str,
    arrival: Arrival<'a>,
    config: &'a ConfigFile,
    registry: &'a [(String, ProfileDelta)],
    candidates: &'a Candidates,
) -> Result<Vec<&'a ProfileDelta>, ProfileError> {
    let mut deltas = Vec::new();
    let mut chain: Vec<&str> = Vec::new();
    let mut arrival = arrival;
    let mut current = name;
    loop {
        if current == DEFAULT {
            let (next, source) = candidates.resolve(&chain)?;
            current = next;
            if let Some(source) = source {
                arrival = Arrival::Default(source);
            }
            continue;
        }
        if current == BASE {
            return Ok(deltas);
        }
        if chain.contains(&current) {
            let mut cycle: Vec<String> = chain.iter().map(|n| n.to_string()).collect();
            cycle.push(current.to_owned());
            return Err(ProfileError::ExtendsCycle(cycle));
        }
        chain.push(current);
        let cfg = lookup(&config.profiles, current);
        let registered = lookup(registry, current);
        let ship = shipped(current);
        if cfg.is_none() && registered.is_none() && ship.is_none() {
            return Err(match arrival {
                Arrival::Extends(profile) => ProfileError::UnknownExtends {
                    profile: profile.to_owned(),
                    extends: current.to_owned(),
                },
                Arrival::Requested => ProfileError::UnknownProfile {
                    name: current.to_owned(),
                    source: None,
                    known: known_names(config, registry),
                },
                Arrival::Default(source) => ProfileError::UnknownProfile {
                    name: current.to_owned(),
                    source: Some(source),
                    known: known_names(config, registry),
                },
            });
        }
        if let Some(delta) = cfg {
            deltas.push(delta);
        }
        if let Some(delta) = registered {
            if cfg.is_some_and(|d| d.extends.is_some()) {
                return Err(ProfileError::ExtendsOnRegistered(current.to_owned()));
            }
            deltas.push(delta);
            return Ok(deltas);
        }
        if let Some(delta) = ship {
            deltas.push(delta);
        }
        match cfg.and_then(|d| d.extends.as_deref()) {
            Some(parent) => {
                arrival = Arrival::Extends(current);
                current = parent;
            }
            None => current = if ship.is_some() { BASE } else { DEFAULT },
        }
    }
}

/// Resolve profile `name` against `config`, `registry`, and `candidates`,
/// starting from `base`.
pub(crate) fn resolve(
    name: &str,
    config: &ConfigFile,
    registry: &[(String, ProfileDelta)],
    base: &Settings,
    candidates: &Candidates,
) -> Result<Settings, ProfileError> {
    let deltas = delta_chain(name, Arrival::Requested, config, registry, candidates)?;
    let mut settings = base.clone();
    for delta in deltas.iter().rev() {
        delta.apply(&mut settings);
    }
    Ok(settings)
}

/// Check every profile `config` defines and its `default` entry, not just
/// the one about to be resolved, so a cycle or dangling name in an unused
/// profile fails loudly instead of lingering until someone selects it. The
/// volatile default-profile settings are stripped first: what they name is
/// checked by the resolutions that consult them, and must not fail
/// resolutions that don't (such as `base`).
fn validate(
    config: &ConfigFile,
    registry: &[(String, ProfileDelta)],
    candidates: &Candidates,
) -> Result<(), ProfileError> {
    let structural = candidates.structural();
    for (name, _) in &config.profiles {
        delta_chain(name, Arrival::Requested, config, registry, &structural)?;
    }
    if let Some(name) = &config.default {
        delta_chain(
            name,
            Arrival::Default("the default entry in hegel.toml"),
            config,
            registry,
            &structural,
        )?;
    }
    Ok(())
}

/// Resolve settings for the profile `name`, or for the `default` alias
/// when `name` is `None`. The entry point behind `hegel_settings_new` and
/// `hegel_settings_new_for_profile`.
pub(crate) fn settings_for(name: Option<&str>) -> Result<Settings, ProfileError> {
    let config = config::load()?;
    let registry = registry_snapshot();
    let overridden = DEFAULT_OVERRIDE.lock().clone();
    settings_for_from(name, &config, &registry, overridden, crate::sys::env_var)
}

/// [`settings_for`] with the config, registry, override, and environment
/// injected.
fn settings_for_from(
    name: Option<&str>,
    config: &ConfigFile,
    registry: &[(String, ProfileDelta)],
    overridden: Option<String>,
    env: impl Fn(&str) -> Option<String>,
) -> Result<Settings, ProfileError> {
    let candidates = Candidates::gather(config, overridden, &env);
    validate(config, registry, &candidates)?;
    let base = Settings::base(crate::antithesis_detect::antithesis_env_var_set_from(&env));
    let mut settings = resolve(
        name.unwrap_or(DEFAULT),
        config,
        registry,
        &base,
        &candidates,
    )?;
    settings.config_path = config.path.clone();
    Ok(settings)
}

#[cfg(test)]
#[path = "../tests/embedded/profiles_tests.rs"]
mod tests;
