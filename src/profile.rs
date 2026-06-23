//! Profile parent-chain resolution and stacked profile settings.
//!
//! A Portage profile is a directory that may contain a `parent` file listing
//! other profile directories (relative or absolute), plus settings files such
//! as `make.defaults`, `packages`, `package.use`, `package.mask`,
//! `package.keywords`/`package.accept_keywords`, `use.force`, and `use.mask`.
//!
//! This module resolves the parent chain depth-first (parents before children,
//! matching upstream `LocationsManager._addProfile`) and stacks each settings
//! file across the chain with the [`crate::util`] primitives. Filesystem
//! ownership is explicit: every entry point takes the profile directory path.
//!
//! Reference:
//! - `research/portage/lib/portage/package/ebuild/_config/LocationsManager.py`
//! - `research/portage/lib/portage/package/ebuild/config.py`

use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::path::{Path, PathBuf};

use crate::config::{ParseError, getconfig};
use crate::util::{grabdict, grabfile, normalize_path, stack_dicts, stack_lists};

/// Error raised while loading a profile tree.
#[derive(Debug)]
pub enum ProfileError {
    /// A `parent` file referenced a profile directory that does not exist.
    ParentNotFound {
        parent: String,
        referenced_by: PathBuf,
    },
    /// A `parent` file existed but contained no usable entries.
    EmptyParent(PathBuf),
    /// A profile directory does not exist.
    MissingProfile(PathBuf),
    /// A settings file failed to parse.
    Config(ParseError),
    /// An I/O error reading a profile file.
    Io(String),
}

impl fmt::Display for ProfileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ParentNotFound {
                parent,
                referenced_by,
            } => write!(
                f,
                "parent '{parent}' not found: '{}'",
                referenced_by.display()
            ),
            Self::EmptyParent(path) => write!(f, "empty parent file: '{}'", path.display()),
            Self::MissingProfile(path) => write!(f, "profile not found: '{}'", path.display()),
            Self::Config(err) => write!(f, "{err}"),
            Self::Io(msg) => f.write_str(msg),
        }
    }
}

impl std::error::Error for ProfileError {}

impl From<ParseError> for ProfileError {
    fn from(err: ParseError) -> Self {
        Self::Config(err)
    }
}

/// Reads a profile file's text, returning `Ok(None)` when the file is absent.
fn read_optional(path: &Path) -> Result<Option<String>, ProfileError> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(ProfileError::Io(format!("{}: {err}", path.display()))),
    }
}

/// The ordered profile directory stack: parents first, the selected profile
/// last. Higher index wins for stacked settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileStack {
    pub profiles: Vec<PathBuf>,
}

impl ProfileStack {
    /// Resolves the parent chain rooted at `profile_dir` depth-first. Each
    /// `parent` entry is resolved relative to the directory containing it,
    /// matching upstream's `os.path.join(currentPath, parentPath)`.
    pub fn resolve(profile_dir: impl AsRef<Path>) -> Result<Self, ProfileError> {
        let mut profiles = Vec::new();
        let mut visited = Vec::new();
        add_profile(profile_dir.as_ref(), &mut profiles, &mut visited)?;
        Ok(Self { profiles })
    }
}

fn add_profile(
    current: &Path,
    profiles: &mut Vec<PathBuf>,
    visited: &mut Vec<PathBuf>,
) -> Result<(), ProfileError> {
    if !current.is_dir() {
        return Err(ProfileError::MissingProfile(current.to_path_buf()));
    }
    let canonical = current
        .canonicalize()
        .map_err(|err| ProfileError::Io(format!("{}: {err}", current.display())))?;
    if visited.contains(&canonical) {
        // Already in the stack via another path; avoid cycles/duplicates.
        return Ok(());
    }
    visited.push(canonical);

    let parents_file = current.join("parent");
    if let Some(text) = read_optional(&parents_file)? {
        let parents = grabfile(&text);
        if parents.is_empty() {
            return Err(ProfileError::EmptyParent(parents_file));
        }
        for parent in parents {
            let resolved = resolve_parent(current, &parent);
            if !resolved.is_dir() {
                return Err(ProfileError::ParentNotFound {
                    parent,
                    referenced_by: parents_file.clone(),
                });
            }
            add_profile(&resolved, profiles, visited)?;
        }
    }

    profiles.push(current.to_path_buf());
    Ok(())
}

fn resolve_parent(current: &Path, parent: &str) -> PathBuf {
    if parent.starts_with('/') {
        PathBuf::from(normalize_path(parent))
    } else {
        let joined = current.join(parent);
        PathBuf::from(normalize_path(&joined.to_string_lossy()))
    }
}

/// The settings produced by stacking a profile chain.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StackedProfile {
    /// Variables from chained `make.defaults` (later profiles override; the
    /// incremental variables are space-accumulated).
    pub variables: BTreeMap<String, String>,
    /// The stacked system set from `packages` lines that begin with `*`
    /// (with the `*` stripped), supporting `-atom` removal.
    pub system_set: Vec<String>,
    /// Stacked `package.use` entries: `cp/atom -> [flags]`.
    pub package_use: BTreeMap<String, Vec<String>>,
    /// Stacked `package.mask` atoms (with `-atom` unmasking).
    pub package_mask: Vec<String>,
    /// Stacked `use.force` flags.
    pub use_force: Vec<String>,
    /// Stacked `use.mask` flags.
    pub use_mask: Vec<String>,
}

/// Incremental make.conf/make.defaults variables that accumulate rather than
/// overwrite across the profile chain. Mirrors Portage's `INCREMENTALS`.
pub const INCREMENTALS: &[&str] = &[
    "USE",
    "USE_EXPAND",
    "USE_EXPAND_HIDDEN",
    "CONFIG_PROTECT",
    "CONFIG_PROTECT_MASK",
    "IUSE_IMPLICIT",
    "FEATURES",
    "ACCEPT_KEYWORDS",
    "ACCEPT_LICENSE",
    "ACCEPT_PROPERTIES",
    "ACCEPT_RESTRICT",
    "ENV_UNSET",
    "PROFILE_ONLY_VARIABLES",
];

impl StackedProfile {
    /// Loads and stacks the standard settings files across the resolved chain.
    pub fn load(stack: &ProfileStack) -> Result<Self, ProfileError> {
        let mut make_defaults: Vec<Option<BTreeMap<String, String>>> = Vec::new();
        let mut packages_lists: Vec<Vec<String>> = Vec::new();
        let mut package_use_dicts: Vec<BTreeMap<String, Vec<String>>> = Vec::new();
        let mut mask_lists: Vec<Vec<String>> = Vec::new();
        let mut use_force_lists: Vec<Vec<String>> = Vec::new();
        let mut use_mask_lists: Vec<Vec<String>> = Vec::new();

        for dir in &stack.profiles {
            if let Some(text) = read_optional(&dir.join("make.defaults"))? {
                // Seed expansion with the variables stacked so far so a later
                // profile's make.defaults can reference earlier assignments.
                let initial: HashMap<String, String> = make_defaults
                    .iter()
                    .flatten()
                    .flat_map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())))
                    .collect();
                let parsed = getconfig(&text, true, &initial)?;
                make_defaults.push(Some(parsed.into_iter().collect()));
            }
            if let Some(text) = read_optional(&dir.join("packages"))? {
                packages_lists.push(system_atoms(&grabfile(&text)));
            }
            if let Some(text) = read_optional(&dir.join("package.use"))? {
                package_use_dicts.push(grabdict(&text, true, false));
            }
            if let Some(text) = read_optional(&dir.join("package.mask"))? {
                mask_lists.push(grabfile(&text));
            }
            if let Some(text) = read_optional(&dir.join("use.force"))? {
                use_force_lists.push(grabfile(&text));
            }
            if let Some(text) = read_optional(&dir.join("use.mask"))? {
                use_mask_lists.push(grabfile(&text));
            }
        }

        let variables = stack_dicts(&make_defaults, false, INCREMENTALS, true).unwrap_or_default();

        Ok(Self {
            variables,
            system_set: stack_lists(&packages_lists, true),
            package_use: stack_package_dict(&package_use_dicts),
            package_mask: stack_lists(&mask_lists, true),
            use_force: stack_lists(&use_force_lists, true),
            use_mask: stack_lists(&use_mask_lists, true),
        })
    }

    /// Convenience: resolve `profile_dir`'s chain and load its settings.
    pub fn from_dir(profile_dir: impl AsRef<Path>) -> Result<Self, ProfileError> {
        Self::load(&ProfileStack::resolve(profile_dir)?)
    }

    /// Resolves an incremental variable (e.g. `USE`, `FEATURES`) into its final
    /// token set. The stacked variable string still contains literal `-token`
    /// removals and `-*` clears; this applies them in order via
    /// [`crate::util::stack_lists`], matching how Portage resolves incrementals.
    pub fn incremental_tokens(&self, name: &str) -> Vec<String> {
        let Some(value) = self.variables.get(name) else {
            return Vec::new();
        };
        let tokens: Vec<String> = value.split_whitespace().map(str::to_string).collect();
        stack_lists(&[tokens], true)
    }
}

/// Extracts the system-set atoms from `packages` lines: entries that begin
/// with `*` contribute the atom (minus the `*`); `-*atom` removal is preserved.
fn system_atoms(lines: &[String]) -> Vec<String> {
    let mut atoms = Vec::new();
    for line in lines {
        if let Some(rest) = line.strip_prefix('*') {
            atoms.push(rest.to_string());
        } else if let Some(rest) = line.strip_prefix("-*") {
            atoms.push(format!("-{rest}"));
        }
    }
    atoms
}

/// Stacks per-package value lists (`package.use`-style) across profiles,
/// applying `-flag` removals within each package's accumulated list.
fn stack_package_dict(dicts: &[BTreeMap<String, Vec<String>>]) -> BTreeMap<String, Vec<String>> {
    let mut by_key: BTreeMap<String, Vec<Vec<String>>> = BTreeMap::new();
    for dict in dicts {
        for (key, values) in dict {
            by_key.entry(key.clone()).or_default().push(values.clone());
        }
    }
    by_key
        .into_iter()
        .map(|(key, lists)| (key, stack_lists(&lists, true)))
        .collect()
}
