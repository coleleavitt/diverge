//! End-to-end session: load a real on-disk config root and run an emerge
//! request through config → repositories → profile → resolver → plan.
//!
//! This is the integration layer that makes the binary actually *do* something:
//! it reads `make.conf`, `repos.conf`, the active profile chain, and the
//! installed-package database from a configurable root (defaulting to the host
//! `/`), assembles the resolver inputs, and renders the resulting plan.
//!
//! Safety: loading is read-only. The only mutating path is an explicit
//! merge/unmerge through the executor against the configured root, which the
//! `--pretend` flow never invokes.
//!
//! Reference:
//! - `research/portage/lib/_emerge/main.py` (`emerge_main`)
//! - `research/portage/lib/portage/package/ebuild/config.py`
//! - `research/portage/lib/portage/repository/config.py` (repos.conf)

use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};

use crate::cli::{EmergeAction, EmergeRequest};
use crate::config::{getconfig, varexpand};
use crate::dbapi::PackageDb;
use crate::depgraph::{ResolveOutcome, ResolveParams, Resolver};
use crate::profile::StackedProfile;
use crate::repository::Repository;
use crate::vardb;

/// Error raised while building or running a session.
#[derive(Debug)]
pub enum SessionError {
    /// A config file failed to parse.
    Config(String),
    /// A repository failed to load.
    Repository(String),
    /// The installed database failed to load.
    Vardb(String),
    /// An I/O error.
    Io(String),
}

impl fmt::Display for SessionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Config(m) => write!(f, "config error: {m}"),
            Self::Repository(m) => write!(f, "repository error: {m}"),
            Self::Vardb(m) => write!(f, "installed-db error: {m}"),
            Self::Io(m) => write!(f, "io error: {m}"),
        }
    }
}

impl std::error::Error for SessionError {}

/// A configured emerge session: merged config variables, the combined available
/// package store (all repositories), the installed store, and the active
/// profile settings.
pub struct Session {
    /// The config root (`PORTAGE_CONFIGROOT`, holds `etc/portage`).
    pub config_root: PathBuf,
    /// The install root (`ROOT`/`EROOT`, holds `var/db/pkg`).
    pub eroot: PathBuf,
    /// Merged make.globals + make.conf + profile make.defaults variables.
    pub variables: HashMap<String, String>,
    /// Combined available packages across all configured repositories.
    pub available: PackageDb,
    /// Installed packages (vartree).
    pub installed: PackageDb,
    /// Stacked profile (system set, package.use/mask, use.force/mask).
    pub profile: Option<StackedProfile>,
}

impl Session {
    /// Loads a session from a config root and install root. Pass `/` for both
    /// to operate on the host system (read-only until an explicit merge).
    pub fn load(
        config_root: impl AsRef<Path>,
        eroot: impl AsRef<Path>,
    ) -> Result<Self, SessionError> {
        let config_root = config_root.as_ref().to_path_buf();
        let eroot = eroot.as_ref().to_path_buf();

        let variables = load_config_variables(&config_root)?;
        let available = load_repositories(&config_root, &variables)?;
        let installed =
            vardb::load(vardb::vdb_path(&eroot)).map_err(|e| SessionError::Vardb(e.to_string()))?;
        let profile = load_profile(&config_root)?;

        Ok(Self {
            config_root,
            eroot,
            variables,
            available,
            installed,
            profile,
        })
    }

    /// The system arch keyword (`ARCH`), defaulting to `amd64` when unset.
    pub fn arch(&self) -> String {
        self.variables
            .get("ARCH")
            .cloned()
            .or_else(|| {
                self.profile
                    .as_ref()
                    .and_then(|p| p.variables.get("ARCH").cloned())
            })
            .unwrap_or_else(|| "amd64".to_string())
    }

    /// The accepted keywords (`ACCEPT_KEYWORDS`) as a token list.
    pub fn accept_keywords(&self) -> Vec<String> {
        self.variables
            .get("ACCEPT_KEYWORDS")
            .map(|v| v.split_whitespace().map(str::to_string).collect())
            .unwrap_or_default()
    }

    /// The globally enabled USE flags (`USE`) as a set.
    pub fn use_flags(&self) -> Vec<String> {
        let mut flags: Vec<String> = self
            .profile
            .as_ref()
            .map(|p| p.incremental_tokens("USE"))
            .unwrap_or_default();
        if let Some(use_var) = self.variables.get("USE") {
            for tok in use_var.split_whitespace() {
                if let Some(stripped) = tok.strip_prefix('-') {
                    flags.retain(|f| f != stripped);
                } else if !flags.iter().any(|f| f == tok) {
                    flags.push(tok.to_string());
                }
            }
        }
        flags
    }

    /// Builds the resolver parameters from this session's configuration and the
    /// request options.
    pub fn resolve_params(&self, request: &EmergeRequest) -> ResolveParams {
        ResolveParams::default()
            .with_arch(self.arch())
            .with_use(self.use_flags())
            .with_update(request.options.update)
            .with_deep(request.options.deep)
            .with_newuse(request.options.newuse)
            .with_autounmask(request.options.autounmask.is_yes())
            .with_accept_keywords(self.accept_keywords())
    }

    /// Resolves a request's targets (and `@set` expansions) into a plan.
    pub fn resolve(&self, request: &EmergeRequest) -> ResolveOutcome {
        let params = self.resolve_params(request);
        let resolver = Resolver::new(&self.available, &self.installed, params);
        let targets: Vec<&str> = request.raw_targets.iter().map(String::as_str).collect();
        resolver.resolve(&targets)
    }

    /// Renders a request as an emerge-style pretend plan (the `--pretend`/`-p`
    /// output), returning the human-readable report.
    pub fn pretend(&self, request: &EmergeRequest) -> String {
        let outcome = self.resolve(request);
        render_plan(request, &outcome, self)
    }
}

/// Loads make.globals + make.conf with profile-aware variable expansion.
fn load_config_variables(config_root: &Path) -> Result<HashMap<String, String>, SessionError> {
    let mut vars: HashMap<String, String> = HashMap::new();

    // make.globals (defaults) then make.conf (user overrides). Each is parsed
    // with the accumulated map so later files can reference earlier vars.
    for rel in [
        "usr/share/portage/config/make.globals",
        "etc/portage/make.conf",
    ] {
        let path = config_root.join(rel);
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        let parsed = getconfig(&content, true, &vars)
            .map_err(|e| SessionError::Config(format!("{}: {e}", path.display())))?;
        for (k, v) in parsed {
            let expanded = varexpand(&v, &vars);
            vars.insert(k, expanded);
        }
    }

    // make.conf may also be a directory of fragments (etc/portage/make.conf/*).
    let conf_dir = config_root.join("etc/portage/make.conf");
    if conf_dir.is_dir()
        && let Ok(entries) = std::fs::read_dir(&conf_dir)
    {
        let mut files: Vec<PathBuf> = entries.filter_map(Result::ok).map(|e| e.path()).collect();
        files.sort();
        for path in files {
            let Ok(content) = std::fs::read_to_string(&path) else {
                continue;
            };
            if let Ok(parsed) = getconfig(&content, true, &vars) {
                for (k, v) in parsed {
                    vars.insert(k, v);
                }
            }
        }
    }

    Ok(vars)
}

/// Reads `etc/portage/repos.conf` (file or directory) for repository locations,
/// loading each into one combined [`PackageDb`]. Falls back to the conventional
/// `usr/portage`/`var/db/repos/gentoo` locations when no repos.conf is present.
fn load_repositories(
    config_root: &Path,
    variables: &HashMap<String, String>,
) -> Result<PackageDb, SessionError> {
    let mut locations = repo_locations(config_root);

    // Fallback: PORTDIR or conventional tree roots.
    if locations.is_empty() {
        if let Some(portdir) = variables.get("PORTDIR") {
            locations.push(PathBuf::from(portdir));
        }
        for candidate in ["var/db/repos/gentoo", "usr/portage"] {
            let p = config_root.join(candidate);
            if p.is_dir() {
                locations.push(p);
            }
        }
    }

    let mut combined = PackageDb::new();
    for location in locations {
        if !location.is_dir() {
            continue;
        }
        match Repository::load(&location) {
            Ok(repo) => combined.merge_from(&repo.db),
            // A malformed/foreign tree is skipped rather than aborting the run.
            Err(_) => continue,
        }
    }
    Ok(combined)
}

/// Collects repository `location` paths from `repos.conf` (file or `.d` dir).
fn repo_locations(config_root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let base = config_root.join("etc/portage/repos.conf");

    let mut files: Vec<PathBuf> = Vec::new();
    if base.is_file() {
        files.push(base.clone());
    } else if base.is_dir()
        && let Ok(entries) = std::fs::read_dir(&base)
    {
        files.extend(entries.filter_map(Result::ok).map(|e| e.path()));
        files.sort();
    }

    for file in files {
        let Ok(content) = std::fs::read_to_string(&file) else {
            continue;
        };
        for line in content.lines() {
            let line = line.trim();
            if let Some(rest) = line.strip_prefix("location")
                && let Some((_, value)) = rest.split_once('=')
            {
                out.push(PathBuf::from(value.trim()));
            }
        }
    }
    out
}

/// Loads the active profile from the `etc/portage/make.profile` symlink chain.
fn load_profile(config_root: &Path) -> Result<Option<StackedProfile>, SessionError> {
    for rel in ["etc/portage/make.profile", "etc/make.profile"] {
        let link = config_root.join(rel);
        if link.exists() {
            return StackedProfile::from_dir(&link)
                .map(Some)
                .map_err(|e| SessionError::Config(format!("profile: {e}")));
        }
    }
    Ok(None)
}

/// Renders the resolution outcome as an emerge-style plan report.
fn render_plan(request: &EmergeRequest, outcome: &ResolveOutcome, session: &Session) -> String {
    let mut out = String::new();
    if request.action != EmergeAction::Merge {
        out.push_str(&format!(
            "Action {:?} is not yet wired end-to-end.\n",
            request.action
        ));
        return out;
    }

    // A hard failure (not an autounmask suggestion) aborts the plan render.
    if let Some(err) = &outcome.error
        && !outcome.needs_autounmask()
    {
        out.push_str(&format!("!!! Dependency resolution failed: {err}\n"));
        return out;
    }

    out.push_str("\nThese are the packages that would be merged, in order:\n\n");
    for cpv in &outcome.mergelist {
        let installed = !session
            .installed
            .match_str(&format!("={cpv}"))
            .unwrap_or_default()
            .is_empty();
        let tag = if installed { "R" } else { "N" };
        out.push_str(&format!("[ebuild  {tag}     ] {cpv}\n"));
    }
    out.push_str(&format!(
        "\nTotal: {} package(s)\n",
        outcome.mergelist.len()
    ));

    if outcome.needs_autounmask() {
        out.push_str("\nThe following keyword changes are necessary to proceed:\n");
        for cpv in &outcome.unstable_keywords {
            out.push_str(&format!("  ={cpv} ~{}\n", session.arch()));
        }
    }
    out
}
