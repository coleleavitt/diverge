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

use std::collections::{BTreeSet, HashMap};
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
    /// Per-repository configuration parsed from `repos.conf` (location +
    /// sync settings), in declaration order.
    pub repos: Vec<RepoConfig>,
}

/// One repository's configuration from `repos.conf`: its name, on-disk
/// `location`, and optional sync settings (`sync-type`, `sync-uri`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoConfig {
    pub name: String,
    pub location: PathBuf,
    pub sync_type: Option<String>,
    pub sync_uri: Option<String>,
}

/// The result of executing a merge plan (see [`Session::merge_action`]).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MergeReport {
    /// cpvs successfully merged into the root (image installed + VDB recorded).
    pub merged: Vec<String>,
    /// The cpv whose build failed, if any (stops the merge there).
    pub failed: Option<String>,
    /// cpvs not attempted after a failure (resume state).
    pub remaining: Vec<String>,
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
        let repos = load_repo_configs(&config_root, &variables);
        let available = load_repositories(&repos)?;
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
            repos,
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
        let masks = self
            .profile
            .as_ref()
            .map(|p| p.package_mask.clone())
            .unwrap_or_default();
        ResolveParams::default()
            .with_arch(self.arch())
            .with_use(self.use_flags())
            .with_update(request.options.update)
            .with_deep(request.options.deep)
            .with_newuse(request.options.newuse)
            .with_autounmask(request.options.autounmask.is_yes())
            .with_accept_keywords(self.accept_keywords())
            .with_masks(masks)
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

    /// Installs an already-built package image into this session's `eroot`:
    /// merges `image` into the root under CONFIG_PROTECT, records the package in
    /// the VDB (CONTENTS + metadata), and adds `cpv`'s cp to the world file
    /// unless `oneshot`. Returns the merged file list.
    ///
    /// Safety: every write goes under `self.eroot` (tests pass a tempdir). This
    /// is the building block the merge action composes per package.
    pub fn install_image(
        &self,
        cpv: &str,
        image: &Path,
        oneshot: bool,
    ) -> Result<crate::executor::MergeResult, SessionError> {
        let protect = self.config_protect();
        let result = crate::executor::MergeTransaction::new(image, &self.eroot, &protect)
            .run()
            .map_err(|e| SessionError::Io(e.to_string()))?;

        // Record into the VDB using the available metadata (or a default).
        let metadata = self.available.metadata(cpv).cloned().unwrap_or_default();
        crate::vardb::record_install(
            &crate::vardb::vdb_path(&self.eroot),
            cpv,
            &metadata,
            &result.contents,
        )
        .map_err(|e| SessionError::Vardb(e.to_string()))?;

        if !oneshot {
            self.add_to_world(cpv)?;
        }
        Ok(result)
    }

    /// Builds the CONFIG_PROTECT resolver from this session's variables
    /// (defaulting to `/etc` when unset, like Portage).
    fn config_protect(&self) -> crate::executor::ConfigProtect {
        let protect_var = self
            .variables
            .get("CONFIG_PROTECT")
            .cloned()
            .unwrap_or_else(|| "/etc".to_string());
        let mask_var = self
            .variables
            .get("CONFIG_PROTECT_MASK")
            .cloned()
            .unwrap_or_default();
        let protect: Vec<&str> = protect_var.split_whitespace().collect();
        let mask: Vec<&str> = mask_var.split_whitespace().collect();
        crate::executor::ConfigProtect::new(&protect, &mask)
    }

    /// The path to the world file (`<eroot>/var/lib/portage/world`).
    pub fn world_path(&self) -> PathBuf {
        self.eroot.join("var/lib/portage/world")
    }

    /// Adds a cpv's `category/package` to the world file (read-modify-write),
    /// creating it if absent. Returns whether it was newly added.
    fn add_to_world(&self, cpv: &str) -> Result<bool, SessionError> {
        let path = self.world_path();
        let existing = std::fs::read_to_string(&path).unwrap_or_default();
        let mut world = crate::sets::WorldFile::parse(&existing);
        let cp = crate::version::split_cpv(cpv).0;
        let added = world.add(cp);
        if added {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| SessionError::Io(format!("{}: {e}", parent.display())))?;
            }
            std::fs::write(&path, world.render())
                .map_err(|e| SessionError::Io(format!("{}: {e}", path.display())))?;
        }
        Ok(added)
    }

    /// Unmerges an installed package: removes its recorded files from the root
    /// (using the merge result's contents) and deletes its VDB entry. Used by
    /// the unmerge action. Writes only under `self.eroot`.
    pub fn unmerge_package(
        &self,
        cpv: &str,
        contents: &[crate::executor::ContentEntry],
    ) -> Result<(), SessionError> {
        crate::executor::unmerge(&self.eroot, contents)
            .map_err(|e| SessionError::Io(e.to_string()))?;
        crate::vardb::remove_install(&crate::vardb::vdb_path(&self.eroot), cpv)
            .map_err(|e| SessionError::Vardb(e.to_string()))?;
        Ok(())
    }

    /// Dispatches a parsed request to the matching action renderer, returning
    /// the report text.
    pub fn dispatch(&self, request: &EmergeRequest) -> String {
        match request.action {
            EmergeAction::Merge => self.pretend(request),
            EmergeAction::Search => self.search(&request.raw_targets),
            EmergeAction::Depclean => self.depclean_report(request),
            EmergeAction::ListSets => self.list_sets(),
            EmergeAction::Info => self.info(),
            EmergeAction::Version => version_banner(),
            EmergeAction::Moo => MOO.to_string(),
            EmergeAction::Sync => self.sync_action(&mut crate::sync::LocalSync),
            EmergeAction::Regen | EmergeAction::Metadata => self.regen_action(),
            EmergeAction::Config => {
                // Run pkg_config via the real process spawner against the vdb's
                // ebuild for each installed target.
                let mut spawner = crate::executor::CommandSpawner::new("ebuild.sh");
                self.config_action(&request.raw_targets, &mut spawner)
            }
            EmergeAction::Unmerge => {
                render_removal(self.unmerge_action(&request.raw_targets), "unmerged")
            }
            EmergeAction::Clean | EmergeAction::RageClean => {
                render_removal(self.clean_action(), "cleaned")
            }
            EmergeAction::Prune => render_removal(self.prune_action(), "pruned"),
            EmergeAction::CheckNews => {
                let items = self.check_news();
                if items.is_empty() {
                    "No unread news items relevant to this system.\n".to_string()
                } else {
                    let mut out = format!(
                        "{} unread news item(s) relevant to this system:\n",
                        items.len()
                    );
                    for name in &items {
                        out.push_str(&format!("  {name}\n"));
                    }
                    out
                }
            }
            other => format!("Action {other:?} is not yet implemented.\n"),
        }
    }

    /// Port of `emerge --sync`: syncs each configured repository that has a
    /// `sync-type`/`sync-uri`, through an injectable [`crate::sync::SyncBackend`]
    /// so tests run without network. Renders emerge's per-repo banner and
    /// reports per-repository success/failure.
    pub fn sync_action(&self, backend: &mut dyn crate::sync::SyncBackend) -> String {
        use crate::sync::{SyncConfig, SyncType};
        let mut out = String::new();
        let mut synced = 0usize;
        let mut failed = 0usize;

        for repo in &self.repos {
            // Only repos with a sync source are synced (matches emerge).
            let Some(uri) = &repo.sync_uri else {
                continue;
            };
            let sync_type = match repo.sync_type.as_deref() {
                Some("rsync") => SyncType::Rsync,
                Some("git") => SyncType::Git,
                Some("webrsync") => SyncType::WebRsync,
                _ => SyncType::Local,
            };
            out.push_str(&format!(
                ">>> Syncing repository '{}' into '{}'...\n",
                repo.name,
                repo.location.display()
            ));
            let config = SyncConfig {
                name: repo.name.clone(),
                location: repo.location.clone(),
                uri: uri.clone(),
                sync_type,
            };
            match backend.sync(&config) {
                Ok(outcome) => {
                    synced += 1;
                    if outcome.updated {
                        out.push_str(&format!(
                            ">>> Repository '{}' updated ({} file(s) changed).\n",
                            repo.name,
                            outcome.changed_files.len()
                        ));
                    } else {
                        out.push_str(&format!(
                            ">>> Repository '{}' is already up to date.\n",
                            repo.name
                        ));
                    }
                }
                Err(err) => {
                    failed += 1;
                    out.push_str(&format!("!!! Sync error in '{}': {err}\n", repo.name));
                }
            }
        }

        if synced == 0 && failed == 0 {
            out.push_str("No repositories with a configured sync source.\n");
        } else {
            out.push_str(&format!("\nActions: {synced} synced, {failed} failed.\n"));
        }
        out
    }

    /// Port of `emerge --regen`/`--metadata`: regenerates the md5-cache for each
    /// configured repository tree, writing `metadata/md5-cache/<cat>/<pf>`
    /// entries from each ebuild's parsed metadata. Reports the count.
    pub fn regen_action(&self) -> String {
        let mut out = String::new();
        let mut total = 0usize;
        for repo in &self.repos {
            if !repo.location.is_dir() {
                continue;
            }
            match crate::repository::regen_md5_cache(&repo.location) {
                Ok(n) => {
                    total += n;
                    out.push_str(&format!(
                        ">>> Regenerated {n} cache entr{} for '{}'.\n",
                        if n == 1 { "y" } else { "ies" },
                        repo.name
                    ));
                }
                Err(err) => {
                    out.push_str(&format!("!!! Regen error in '{}': {err}\n", repo.name));
                }
            }
        }
        if total == 0 {
            out.push_str("No cache entries regenerated.\n");
        }
        out
    }

    /// Port of `emerge --depclean`/`--clean`/`--rage-clean` *execution*: computes
    /// the removal set (installed packages not reachable from the world+system
    /// protected set) and unmerges each, gated against `ROOT=/`. Returns the
    /// removed cpvs. (`--depclean` itself stays preview via `depclean_report`;
    /// this is the destructive clean path.)
    pub fn clean_action(&self) -> Result<Vec<String>, SessionError> {
        if !self.mutation_allowed() {
            return Err(SessionError::Io(
                "refusing to clean ROOT=/ (set DIVERGE_ALLOW_ROOT to override)".to_string(),
            ));
        }
        let mut protected = self.world_atoms();
        if let Some(profile) = &self.profile {
            protected.extend(profile.system_set.clone());
        }
        let protected_refs: Vec<&str> = protected.iter().map(String::as_str).collect();
        let resolver = Resolver::new(&self.available, &self.installed, ResolveParams::default());
        let cleanlist = resolver.depclean(&protected_refs);
        self.unmerge_cpvs(&cleanlist)
    }

    /// Port of `emerge --prune`: keep only the highest installed version per
    /// `cp` (slot-aware), unmerging the lower ones. Gated against `ROOT=/`.
    pub fn prune_action(&self) -> Result<Vec<String>, SessionError> {
        if !self.mutation_allowed() {
            return Err(SessionError::Io(
                "refusing to prune ROOT=/ (set DIVERGE_ALLOW_ROOT to override)".to_string(),
            ));
        }
        // Group installed cpvs by (cp, slot); keep the highest version in each.
        let mut by_key: std::collections::BTreeMap<(String, String), Vec<String>> =
            std::collections::BTreeMap::new();
        for cpv in self.installed.cpv_all() {
            let (cp, _) = crate::version::split_cpv(&cpv);
            let slot = self
                .installed
                .metadata(&cpv)
                .and_then(|m| m.slot.clone())
                .unwrap_or_else(|| "0".to_string());
            by_key.entry((cp, slot)).or_default().push(cpv);
        }
        let mut prunelist = Vec::new();
        for (_, mut cpvs) in by_key {
            if cpvs.len() < 2 {
                continue;
            }
            cpvs.sort_by(|a, b| crate::version::cpv_cmp(a, b));
            // All but the highest version are pruned.
            cpvs.pop();
            prunelist.extend(cpvs);
        }
        prunelist.sort_by(|a, b| crate::version::cpv_cmp(a, b));
        self.unmerge_cpvs(&prunelist)
    }

    /// Unmerges a list of exact cpvs (reading each one's recorded CONTENTS).
    fn unmerge_cpvs(&self, cpvs: &[String]) -> Result<Vec<String>, SessionError> {
        let vdb = crate::vardb::vdb_path(&self.eroot);
        let mut removed = Vec::new();
        for cpv in cpvs {
            let contents = crate::vardb::read_contents(&vdb, cpv);
            self.unmerge_package(cpv, &contents)?;
            removed.push(cpv.clone());
        }
        Ok(removed)
    }

    /// Port of `emerge --check-news`: returns the GLEP 42 news items relevant to
    /// this system (by installed packages, keyword, and profile) that the user
    /// has not yet read. News items live at `<repo>/metadata/news/<item>/` and
    /// the read set at `<eroot>/var/lib/gentoo/news/news-<repo>.read`.
    pub fn check_news(&self) -> Vec<String> {
        use crate::news::{NewsEnvironment, NewsItem, ReadTracker};
        let env = NewsEnvironment {
            installed: self.installed.cpv_all(),
            keyword: self.arch(),
            profile: self
                .profile
                .as_ref()
                .and_then(|_| {
                    std::fs::read_link(self.config_root.join("etc/portage/make.profile")).ok()
                })
                .map(|p| p.to_string_lossy().into_owned()),
        };

        let mut relevant_unread = Vec::new();
        for repo in &self.repos {
            let news_dir = repo.location.join("metadata").join("news");
            let Ok(entries) = std::fs::read_dir(&news_dir) else {
                continue;
            };
            // The per-repo read set.
            let read_path = self
                .eroot
                .join("var/lib/gentoo/news")
                .join(format!("news-{}.read", repo.name));
            let tracker = std::fs::read_to_string(&read_path)
                .map(|c| ReadTracker::parse(&c))
                .unwrap_or_default();

            let mut names: Vec<String> = entries
                .filter_map(Result::ok)
                .filter(|e| e.path().is_dir())
                .filter_map(|e| e.file_name().to_str().map(str::to_string))
                .collect();
            names.sort();
            for name in names {
                if tracker.is_read(&name) {
                    continue;
                }
                // The news item text lives at <dir>/<name>/<name>.en.txt.
                let item_path = news_dir.join(&name).join(format!("{name}.en.txt"));
                let Ok(text) = std::fs::read_to_string(&item_path) else {
                    continue;
                };
                let item = NewsItem::parse(&text);
                if item.is_valid() && item.is_relevant(&env) {
                    relevant_unread.push(name);
                }
            }
        }
        relevant_unread
    }

    /// Port of `emerge --unmerge`/`-C`: removes each target's installed files
    /// (read from the VDB `CONTENTS`) and its VDB entry, against `ROOT`. Refused
    /// when [`Self::mutation_allowed`] is false. Returns the removed cpvs.
    pub fn unmerge_action(&self, targets: &[String]) -> Result<Vec<String>, SessionError> {
        if !self.mutation_allowed() {
            return Err(SessionError::Io(
                "refusing to unmerge from ROOT=/ (set DIVERGE_ALLOW_ROOT to override)".to_string(),
            ));
        }
        let vdb = crate::vardb::vdb_path(&self.eroot);
        let mut removed = Vec::new();
        for target in targets {
            for cpv in self.installed.match_str(target).unwrap_or_default() {
                let contents = crate::vardb::read_contents(&vdb, &cpv);
                self.unmerge_package(&cpv, &contents)?;
                removed.push(cpv);
            }
        }
        Ok(removed)
    }

    /// True when mutating this session's `eroot` is permitted. The host root
    /// `/` is refused unless `DIVERGE_ALLOW_ROOT` is set, so a real merge can
    /// never accidentally modify the running system. Tests use a temp root,
    /// which is always allowed.
    fn mutation_allowed(&self) -> bool {
        self.eroot != Path::new("/") || std::env::var_os("DIVERGE_ALLOW_ROOT").is_some()
    }

    /// Executes a resolved merge plan against `ROOT`: runs each package's build
    /// phases through the [`Scheduler`](crate::executor::Scheduler) (via the
    /// injectable `spawner`), then merges its image into the root, records the
    /// VDB entry, and updates the world file. The install image for each cpv is
    /// supplied by `image_for` (in a full pipeline this is the package's `D`
    /// produced by the build; tests pass a fixture image), keeping this decoupled
    /// from real ebuild compilation.
    ///
    /// Refuses to run when [`Self::mutation_allowed`] is false (host `ROOT=/`).
    pub fn merge_action(
        &self,
        request: &EmergeRequest,
        spawner: &mut dyn crate::executor::PhaseSpawner,
        image_for: impl Fn(&str) -> Option<std::path::PathBuf>,
    ) -> Result<MergeReport, SessionError> {
        use crate::executor::phase::{BuildDirs, PhaseContext};
        use crate::executor::{RunMode, Scheduler};

        if !self.mutation_allowed() {
            return Err(SessionError::Io(
                "refusing to merge into ROOT=/ (set DIVERGE_ALLOW_ROOT to override)".to_string(),
            ));
        }

        let outcome = self.resolve(request);
        if let Some(err) = &outcome.error {
            return Err(SessionError::Config(format!("resolution failed: {err}")));
        }

        // The per-package build dir layout (`PORTAGE_BUILDDIR`), under which the
        // build's `src_install` writes the install image (`D` = <build>/image).
        let build_root = self.eroot.join("var/tmp/portage");
        let build_dirs = |cpv: &str| BuildDirs::new(build_root.join(cpv), build_root.join(cpv));

        // Build phases for every package via the scheduler.
        let use_flags = self.use_flags();
        struct Plan {
            use_flags: Vec<String>,
            available: PackageDb,
            root: PathBuf,
            build_root: PathBuf,
        }
        impl crate::executor::scheduler::PackagePlan for Plan {
            fn phase_context(&self, cpv: &str) -> PhaseContext {
                let eapi = self
                    .available
                    .metadata(cpv)
                    .and_then(|m| m.eapi.clone())
                    .unwrap_or_else(|| "0".to_string());
                let build = self.build_root.join(cpv);
                PhaseContext {
                    ebuild: build.join("ebuild"),
                    cpv: cpv.to_string(),
                    eapi,
                    root: self.root.clone(),
                    dirs: BuildDirs::new(build.clone(), build),
                    use_flags: self.use_flags.clone(),
                }
            }
        }
        let plan = Plan {
            use_flags,
            available: self.available.clone(),
            root: self.eroot.clone(),
            build_root: build_root.clone(),
        };

        // Build (run setup..install phases) for each package, then merge.
        let mut scheduler = Scheduler::new(RunMode::BuildOnly, spawner);
        let schedule = scheduler.run(&outcome.mergelist, &plan);

        let mut report = MergeReport {
            merged: Vec::new(),
            failed: schedule.first_failure().map(str::to_string),
            remaining: schedule.remaining.clone(),
        };

        // Merge each successfully-built package's image into the root. The
        // install image defaults to the build's `D` (image dir) — populated by
        // a real `src_install` — and `image_for` can override it (e.g. tests or
        // binary-package extraction).
        let oneshot = request.options.oneshot;
        for record in &schedule.records {
            if !record.success {
                break;
            }
            let image =
                image_for(&record.cpv).unwrap_or_else(|| build_dirs(&record.cpv).image_dir.clone());
            if !image.is_dir() {
                // Nothing was actually produced (no real build/image): record
                // the build but skip the filesystem merge.
                continue;
            }
            self.install_image(&record.cpv, &image, oneshot)?;
            report.merged.push(record.cpv.clone());
        }
        Ok(report)
    }

    /// Port of `emerge --config`: for each target atom, find the installed
    /// package(s) and run their `pkg_config` phase via the injectable
    /// [`crate::executor::PhaseSpawner`] against the configured `ROOT`. Reports
    /// which packages were (re)configured and which atoms had no installed
    /// match.
    pub fn config_action(
        &self,
        targets: &[String],
        spawner: &mut dyn crate::executor::PhaseSpawner,
    ) -> String {
        use crate::executor::phase::{BuildDirs, Phase, PhaseContext};
        let mut out = String::new();

        for target in targets {
            let matches = self.installed.match_str(target).unwrap_or_default();
            if matches.is_empty() {
                out.push_str(&format!("!!! '{target}' is not installed.\n"));
                continue;
            }
            for cpv in matches {
                let meta = self.installed.metadata(&cpv).cloned().unwrap_or_default();
                // The VDB entry dir for `category/pf` (cpv is `category/pf`).
                let pkg_dir = match cpv.split_once('/') {
                    Some((category, pf)) => {
                        crate::vardb::vdb_path(&self.eroot).join(category).join(pf)
                    }
                    None => crate::vardb::vdb_path(&self.eroot).join(&cpv),
                };
                let ctx = PhaseContext {
                    ebuild: pkg_dir.clone(),
                    cpv: cpv.clone(),
                    eapi: meta.eapi.clone().unwrap_or_else(|| "0".to_string()),
                    root: self.eroot.clone(),
                    dirs: BuildDirs::new(pkg_dir.clone(), pkg_dir),
                    use_flags: meta.use_enabled.clone(),
                };
                let env = ctx.environment(Phase::PkgConfig);
                let outcome = spawner.run_phase(Phase::PkgConfig, &env);
                if outcome.success {
                    out.push_str(&format!(">>> Configured {cpv}.\n"));
                } else {
                    out.push_str(&format!(
                        "!!! Configuration of {cpv} failed: {}\n",
                        outcome.message.unwrap_or_default()
                    ));
                }
            }
        }
        out
    }

    /// Port of `emerge --search`/`-s`: lists available packages whose name
    /// contains any search term (case-insensitive substring on the cp).
    pub fn search(&self, terms: &[String]) -> String {
        let mut seen_cps: BTreeSet<String> = BTreeSet::new();
        let mut out = String::new();
        for (cpv, _meta) in self.available.iter() {
            let cp = crate::version::split_cpv(cpv).0;
            if !seen_cps.insert(cp.clone()) {
                continue;
            }
            let matches = terms.is_empty()
                || terms
                    .iter()
                    .any(|t| cp.to_lowercase().contains(&t.to_lowercase()));
            if matches {
                let installed = !self.installed.match_str(&cp).unwrap_or_default().is_empty();
                let latest = self
                    .available
                    .match_str(&cp)
                    .ok()
                    .and_then(|v| v.last().cloned())
                    .unwrap_or_else(|| cp.clone());
                out.push_str(&format!(
                    "*  {cp}\n      Latest version available: {}\n      Installed: {}\n",
                    crate::version::split_cpv(&latest).1.unwrap_or_default(),
                    if installed { "yes" } else { "no" }
                ));
            }
        }
        if out.is_empty() {
            out.push_str("No packages found.\n");
        }
        out
    }

    /// Computes the depclean removal list from the world+system protected set
    /// and renders it (preview only — does not unmerge).
    pub fn depclean_report(&self, request: &EmergeRequest) -> String {
        let mut protected: Vec<String> = self.world_atoms();
        if let Some(profile) = &self.profile {
            protected.extend(profile.system_set.clone());
        }
        for set in &request.sets {
            if set == "world" || set == "selected" {
                protected.extend(self.world_atoms());
            }
        }
        let protected_refs: Vec<&str> = protected.iter().map(String::as_str).collect();
        let resolver = Resolver::new(&self.available, &self.installed, ResolveParams::default());
        let cleanlist = resolver.depclean(&protected_refs);

        let mut out = String::from("\nThese are the packages that would be unmerged:\n\n");
        for cpv in &cleanlist {
            out.push_str(&format!(" {cpv}\n"));
        }
        out.push_str(&format!("\nTotal: {} package(s)\n", cleanlist.len()));
        out
    }

    /// The atoms currently in the world file (`@selected`).
    pub fn world_atoms(&self) -> Vec<String> {
        let content = std::fs::read_to_string(self.world_path()).unwrap_or_default();
        crate::sets::WorldFile::parse(&content).atoms().to_vec()
    }

    /// Port of `emerge --list-sets`: lists the known package set names.
    pub fn list_sets(&self) -> String {
        let mut sets = vec!["selected", "system", "world"];
        sets.sort_unstable();
        let mut out = String::new();
        for s in sets {
            out.push_str(&format!("{s}\n"));
        }
        out
    }

    /// Port of a minimal `emerge --info`: key configuration variables.
    pub fn info(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("ARCH={}\n", self.arch()));
        out.push_str(&format!(
            "ACCEPT_KEYWORDS={}\n",
            self.accept_keywords().join(" ")
        ));
        out.push_str(&format!("USE={}\n", self.use_flags().join(" ")));
        out.push_str(&format!(
            "CONFIG_ROOT={}\nROOT={}\n",
            self.config_root.display(),
            self.eroot.display()
        ));
        out.push_str(&format!(
            "Available packages: {}\nInstalled packages: {}\n",
            self.available.len(),
            self.installed.len()
        ));
        out
    }
}

/// Renders a removal-action result (unmerge/clean/prune) as an emerge-style
/// report, listing each removed cpv and the count, or surfacing the error.
fn render_removal(result: Result<Vec<String>, SessionError>, verb: &str) -> String {
    match result {
        Ok(removed) if removed.is_empty() => {
            format!(">>> Nothing to be {verb}.\n")
        }
        Ok(removed) => {
            let mut out = String::new();
            for cpv in &removed {
                out.push_str(&format!(">>> Unmerging {cpv}...\n"));
            }
            out.push_str(&format!("\n>>> {} package(s) {verb}.\n", removed.len()));
            out
        }
        Err(err) => format!("!!! {err}\n"),
    }
}

/// The `emerge --version` banner.
fn version_banner() -> String {
    format!(
        "diverge {} (emerge-compatible)\n",
        env!("CARGO_PKG_VERSION")
    )
}

/// The `emerge --moo` easter egg.
const MOO: &str = concat!(
    "\n",
    "  Gentoo Linux; Bug #1\n",
    "         (__)\n",
    "         (oo)\n",
    "   /------\\/\n",
    "  / |    ||\n",
    " *  /\\---/\\\n",
    "    ~~   ~~\n",
    "...\"Have you mooed today?\"...\n"
);

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

/// Loads every configured repository into one combined [`PackageDb`]. A
/// malformed/foreign tree is skipped rather than aborting the run.
fn load_repositories(repos: &[RepoConfig]) -> Result<PackageDb, SessionError> {
    let mut combined = PackageDb::new();
    for repo in repos {
        if !repo.location.is_dir() {
            continue;
        }
        if let Ok(loaded) = Repository::load(&repo.location) {
            combined.merge_from(&loaded.db);
        }
    }
    Ok(combined)
}

/// Parses `repos.conf` (file or `.d` directory) into per-repo configs, honoring
/// the INI `[section]` form: each `[name]` block carries `location`,
/// `sync-type`, and `sync-uri`. Falls back to `PORTDIR`/conventional tree roots
/// (as an unnamed `gentoo` repo) when no `repos.conf` is present.
fn load_repo_configs(config_root: &Path, variables: &HashMap<String, String>) -> Vec<RepoConfig> {
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

    let mut repos: Vec<RepoConfig> = Vec::new();
    for file in files {
        let Ok(content) = std::fs::read_to_string(&file) else {
            continue;
        };
        parse_repos_conf(&content, &mut repos);
    }

    if repos.is_empty() {
        // Fallback: PORTDIR or the conventional tree roots, as a `gentoo` repo.
        let mut candidates: Vec<PathBuf> = Vec::new();
        if let Some(portdir) = variables.get("PORTDIR") {
            candidates.push(PathBuf::from(portdir));
        }
        for rel in ["var/db/repos/gentoo", "usr/portage"] {
            candidates.push(config_root.join(rel));
        }
        for location in candidates {
            if location.is_dir() {
                repos.push(RepoConfig {
                    name: "gentoo".to_string(),
                    location,
                    sync_type: None,
                    sync_uri: None,
                });
            }
        }
    }
    repos
}

/// Parses INI-style `repos.conf` content, appending each `[name]` section's
/// config to `repos`. Keys before any section header are ignored.
fn parse_repos_conf(content: &str, repos: &mut Vec<RepoConfig>) {
    let mut current: Option<RepoConfig> = None;
    let push = |cur: &mut Option<RepoConfig>, repos: &mut Vec<RepoConfig>| {
        if let Some(repo) = cur.take()
            && !repo.location.as_os_str().is_empty()
        {
            repos.push(repo);
        }
    };
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            push(&mut current, repos);
            current = Some(RepoConfig {
                name: name.trim().to_string(),
                location: PathBuf::new(),
                sync_type: None,
                sync_uri: None,
            });
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let (key, value) = (key.trim(), value.trim().to_string());
        if let Some(repo) = current.as_mut() {
            match key {
                "location" => repo.location = PathBuf::from(value),
                "sync-type" => repo.sync_type = Some(value),
                "sync-uri" => repo.sync_uri = Some(value),
                _ => {}
            }
        }
    }
    push(&mut current, repos);
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
fn render_plan(_request: &EmergeRequest, outcome: &ResolveOutcome, session: &Session) -> String {
    let mut out = String::new();

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
