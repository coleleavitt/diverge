//! Real dependency-graph resolution over a [`crate::dbapi::PackageDb`].
//!
//! This replaces the hard-coded fixture resolver with a genuine depgraph: it
//! selects packages by atom from the available store, recursively expands their
//! `DEPEND`/`RDEPEND`/`PDEPEND`/`BDEPEND` via [`crate::dep::use_reduce`]
//! (evaluating USE conditionals and `|| ( ... )` choices), honors blockers and
//! keyword visibility, treats already-installed packages as satisfied, and
//! emits a deterministic, dependency-ordered merge list.
//!
//! It is not yet full Portage parity (no backtracking across slot conflicts,
//! no autounmask), but it is a coherent model the scheduler/executor build on.
//!
//! Reference:
//! - `research/portage/lib/_emerge/depgraph.py`
//! - `research/portage/lib/_emerge/create_depgraph_params.py`
//! - `research/portage/lib/portage/tests/resolver/test_simple.py`,
//!   `test_or_choices.py`, `test_blocker.py`, `test_merge_order.py`

use std::collections::{BTreeMap, BTreeSet};

use crate::atom::{Atom, AtomParseOptions, Blocker};
use crate::dbapi::PackageDb;
use crate::dep::{Dep, UseReduceOptions, use_reduce};

const DEPENDENCY_ATOM_OPTIONS: AtomParseOptions = AtomParseOptions {
    allow_wildcard: false,
    allow_repo: true,
};

/// Parameters controlling a resolution, derived from CLI options.
#[derive(Debug, Clone)]
pub struct ResolveParams {
    /// The system architecture keyword (e.g. `x86`, `amd64`).
    pub arch: String,
    /// Additionally accepted keywords (e.g. `~x86` for testing).
    pub accept_keywords: Vec<String>,
    /// Enabled USE flags used to evaluate USE-conditional dependencies.
    pub use_flags: BTreeSet<String>,
    /// When true, an already-installed package satisfying an atom is not
    /// reinstalled (mirrors emerge's default "don't reinstall" behavior).
    pub noreplace: bool,
    /// Dependency variables to follow, in priority order.
    pub dep_keys: Vec<String>,
}

impl Default for ResolveParams {
    fn default() -> Self {
        Self {
            arch: "x86".to_string(),
            accept_keywords: Vec::new(),
            use_flags: BTreeSet::new(),
            noreplace: false,
            dep_keys: vec![
                "BDEPEND".to_string(),
                "DEPEND".to_string(),
                "RDEPEND".to_string(),
                "PDEPEND".to_string(),
            ],
        }
    }
}

impl ResolveParams {
    pub fn with_arch(mut self, arch: impl Into<String>) -> Self {
        self.arch = arch.into();
        self
    }

    pub fn accept_keyword(mut self, keyword: impl Into<String>) -> Self {
        self.accept_keywords.push(keyword.into());
        self
    }

    pub fn with_use<I, S>(mut self, flags: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.use_flags = flags.into_iter().map(Into::into).collect();
        self
    }

    fn keyword_visible(&self, keywords: &[String]) -> bool {
        if keywords.is_empty() {
            return true;
        }
        keywords.iter().any(|kw| {
            kw == &self.arch
                || self.accept_keywords.iter().any(|accept| accept == kw)
                || (kw == "**")
        })
    }
}

/// A single planned operation in the merge list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergeItem {
    pub cpv: String,
    /// True when the package is already installed and merely pulled in as a
    /// satisfied dependency (not re-merged).
    pub already_installed: bool,
}

/// The outcome of a resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolveOutcome {
    pub mergelist: Vec<String>,
    pub error: Option<ResolveFailure>,
}

impl ResolveOutcome {
    pub fn is_success(&self) -> bool {
        self.error.is_none()
    }
}

/// A structured resolution failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolveFailure {
    /// No visible package satisfied the named atom.
    Unsatisfied(String),
    /// A blocker atom conflicted with a package selected for merge.
    Blocked { blocker: String, blocked: String },
    /// A circular dependency was detected among build-time deps.
    CircularDependency(Vec<String>),
    /// A dependency string failed to parse.
    InvalidDependency(String),
}

impl std::fmt::Display for ResolveFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unsatisfied(atom) => write!(f, "no visible package matches {atom}"),
            Self::Blocked { blocker, blocked } => {
                write!(f, "blocker {blocker} conflicts with {blocked}")
            }
            Self::CircularDependency(cycle) => {
                write!(f, "circular dependency: {}", cycle.join(" -> "))
            }
            Self::InvalidDependency(msg) => write!(f, "invalid dependency: {msg}"),
        }
    }
}

/// The dependency resolver: holds the available and installed package stores.
pub struct Resolver<'a> {
    available: &'a PackageDb,
    installed: &'a PackageDb,
    params: ResolveParams,
}

/// Internal node state during graph construction.
struct GraphBuilder<'a> {
    resolver: &'a Resolver<'a>,
    /// cpv -> set of dependency cpvs that must merge before it.
    edges: BTreeMap<String, BTreeSet<String>>,
    /// cpvs selected for merge (available packages chosen).
    selected: BTreeSet<String>,
    /// Blockers collected as (blocker_atom_string, is_strong, owning_cpv).
    blockers: Vec<(Atom, String)>,
    /// Atoms currently being expanded, to detect cycles.
    in_progress: Vec<String>,
}

impl<'a> Resolver<'a> {
    pub fn new(available: &'a PackageDb, installed: &'a PackageDb, params: ResolveParams) -> Self {
        Self {
            available,
            installed,
            params,
        }
    }

    /// Selects the best visible available cpv matching `atom`: highest version
    /// among keyword-visible matches.
    fn select_available(&self, atom: &Atom) -> Option<String> {
        let mut matches = self.available.match_atom(atom);
        matches.retain(|cpv| {
            self.available
                .metadata(cpv)
                .map(|m| self.params.keyword_visible(&m.keywords))
                .unwrap_or(false)
        });
        // match_atom already returns cpvs sorted ascending, so the best
        // (highest version) visible candidate is the last one.
        matches.pop()
    }

    /// True when an installed package already satisfies `atom`.
    fn installed_satisfies(&self, atom: &Atom) -> bool {
        !self.installed.match_atom(atom).is_empty()
    }

    /// Resolves a set of target atoms into a merge plan.
    pub fn resolve(&self, targets: &[&str]) -> ResolveOutcome {
        let mut builder = GraphBuilder {
            resolver: self,
            edges: BTreeMap::new(),
            selected: BTreeSet::new(),
            blockers: Vec::new(),
            in_progress: Vec::new(),
        };

        for target in targets {
            let atom = match Atom::parse_with_options(target, DEPENDENCY_ATOM_OPTIONS) {
                Ok(atom) => atom,
                Err(err) => {
                    return ResolveOutcome {
                        mergelist: Vec::new(),
                        error: Some(ResolveFailure::InvalidDependency(err.to_string())),
                    };
                }
            };
            if let Err(failure) = builder.expand_atom(&atom, true) {
                return ResolveOutcome {
                    mergelist: Vec::new(),
                    error: Some(failure),
                };
            }
        }

        match builder.finish() {
            Ok(mergelist) => ResolveOutcome {
                mergelist,
                error: None,
            },
            Err(failure) => ResolveOutcome {
                mergelist: Vec::new(),
                error: Some(failure),
            },
        }
    }
}

impl GraphBuilder<'_> {
    /// Expands a single atom: select a package, record it, and recurse into its
    /// dependencies. `is_target` distinguishes a top-level request (always
    /// considered for merge) from a transitive dependency (may be satisfied by
    /// an installed package).
    fn expand_atom(&mut self, atom: &Atom, is_target: bool) -> Result<(), ResolveFailure> {
        // A transitive dependency already satisfied by an installed package is
        // not re-merged (emerge's default without --deep/--update/--newuse).
        // Top-level targets are always considered for (re)installation.
        if !is_target && self.resolver.installed_satisfies(atom) {
            return Ok(());
        }

        let Some(cpv) = self.resolver.select_available(atom) else {
            // If an installed package satisfies it, accept that silently.
            if self.resolver.installed_satisfies(atom) {
                return Ok(());
            }
            return Err(ResolveFailure::Unsatisfied(atom_to_request(atom)));
        };

        if self.selected.contains(&cpv) {
            return Ok(()); // Already in the graph.
        }
        if self.in_progress.contains(&cpv) {
            // A build-time cycle: report the cycle path.
            let mut cycle = self.in_progress.clone();
            cycle.push(cpv);
            return Err(ResolveFailure::CircularDependency(cycle));
        }

        self.selected.insert(cpv.clone());
        self.edges.entry(cpv.clone()).or_default();
        self.in_progress.push(cpv.clone());

        let dep_atoms = self.dependencies_of(&cpv)?;
        for (dep_atom, blocker) in dep_atoms {
            if let Some(strong) = blocker {
                let _ = strong;
                self.blockers.push((dep_atom, cpv.clone()));
                continue;
            }
            // Record an edge: the dependency package must precede this one.
            if let Some(dep_cpv) = self.resolver.select_available(&dep_atom) {
                self.edges.entry(cpv.clone()).or_default().insert(dep_cpv);
            }
            self.expand_atom(&dep_atom, false)?;
        }

        self.in_progress.pop();
        Ok(())
    }

    /// Collects the dependency atoms of `cpv` by reducing its dependency
    /// strings with the active USE flags, resolving `|| ( ... )` choices.
    fn dependencies_of(&self, cpv: &str) -> Result<Vec<(Atom, Option<bool>)>, ResolveFailure> {
        let Some(metadata) = self.resolver.available.metadata(cpv) else {
            return Ok(Vec::new());
        };
        let use_list: Vec<&str> = self
            .resolver
            .params
            .use_flags
            .iter()
            .map(String::as_str)
            .collect();
        let options = UseReduceOptions {
            uselist: &use_list,
            ..UseReduceOptions::default()
        };

        let mut atoms = Vec::new();
        for key in &self.resolver.params.dep_keys {
            let Some(dep_str) = metadata.deps.get(key) else {
                continue;
            };
            if dep_str.trim().is_empty() {
                continue;
            }
            let reduced = use_reduce(dep_str, &options)
                .map_err(|err| ResolveFailure::InvalidDependency(err.to_string()))?;
            self.collect_atoms(&reduced, &mut atoms)?;
        }
        Ok(atoms)
    }

    /// Walks a reduced dependency structure, picking atoms and resolving any
    /// `|| ( ... )` choice to its first satisfiable branch.
    fn collect_atoms(
        &self,
        nodes: &[Dep],
        out: &mut Vec<(Atom, Option<bool>)>,
    ) -> Result<(), ResolveFailure> {
        let mut iter = nodes.iter().peekable();
        while let Some(node) = iter.next() {
            match node {
                Dep::Token(token) if token == "||" => {
                    // The next node is the group of alternatives.
                    if let Some(Dep::Group(choices)) = iter.next() {
                        self.resolve_or_choice(choices, out)?;
                    }
                }
                Dep::Token(token) => {
                    let (atom, blocker) = parse_dep_token(token)?;
                    out.push((atom, blocker));
                }
                Dep::Group(inner) => self.collect_atoms(inner, out)?,
            }
        }
        Ok(())
    }

    /// Resolves an `|| ( a b ... )` choice: prefer an already-installed or
    /// available branch, falling back to the first branch. Mirrors emerge's
    /// preference for not pulling in new packages when a choice is satisfied.
    fn resolve_or_choice(
        &self,
        choices: &[Dep],
        out: &mut Vec<(Atom, Option<bool>)>,
    ) -> Result<(), ResolveFailure> {
        // Each choice is either a single atom token or a parenthesized group.
        let branches: Vec<Vec<(Atom, Option<bool>)>> = {
            let mut branches = Vec::new();
            for choice in choices {
                let mut branch = Vec::new();
                match choice {
                    Dep::Token(token) => {
                        let (atom, blocker) = parse_dep_token(token)?;
                        branch.push((atom, blocker));
                    }
                    Dep::Group(inner) => self.collect_atoms(inner, &mut branch)?,
                }
                branches.push(branch);
            }
            branches
        };

        // Prefer a branch already satisfied by installed packages.
        for branch in &branches {
            if branch
                .iter()
                .all(|(atom, blk)| blk.is_some() || self.resolver.installed_satisfies(atom))
            {
                out.extend(branch.clone());
                return Ok(());
            }
        }
        // Otherwise the first branch whose atoms are all available.
        for branch in &branches {
            if branch
                .iter()
                .all(|(atom, blk)| blk.is_some() || self.resolver.select_available(atom).is_some())
            {
                out.extend(branch.clone());
                return Ok(());
            }
        }
        // Fall back to the first branch (will surface an unsatisfied error).
        if let Some(first) = branches.into_iter().next() {
            out.extend(first);
        }
        Ok(())
    }

    /// Finalizes the graph: checks blockers, then topologically orders the
    /// selected packages so dependencies precede dependents.
    fn finish(self) -> Result<Vec<String>, ResolveFailure> {
        self.check_blockers()?;
        topological_order(&self.selected, &self.edges)
    }

    /// A blocker atom must not match any package selected for merge.
    fn check_blockers(&self) -> Result<(), ResolveFailure> {
        for (blocker, owner) in &self.blockers {
            let conflict = self
                .selected
                .iter()
                .find(|cpv| package_matches_atom(self.resolver.available, cpv, blocker));
            if let Some(cpv) = conflict {
                return Err(ResolveFailure::Blocked {
                    blocker: format!("!{}", atom_to_request(blocker)),
                    blocked: format!("{cpv} (from {owner})"),
                });
            }
        }
        Ok(())
    }
}

/// Produces a deterministic topological ordering: dependencies before
/// dependents, ties broken by cpv order. Returns a circular-dependency error
/// if the graph has a cycle that was not already reported.
fn topological_order(
    nodes: &BTreeSet<String>,
    edges: &BTreeMap<String, BTreeSet<String>>,
) -> Result<Vec<String>, ResolveFailure> {
    // Kahn's algorithm over "depends-on" edges. An edge cpv -> dep means dep
    // must come first, so we emit nodes whose deps are all already emitted.
    let mut emitted: Vec<String> = Vec::new();
    let mut remaining: BTreeSet<String> = nodes.clone();

    while !remaining.is_empty() {
        // Find ready nodes: all dependencies already emitted (or not in graph).
        let ready: Vec<String> = remaining
            .iter()
            .filter(|cpv| {
                edges
                    .get(*cpv)
                    .map(|deps| {
                        deps.iter()
                            .all(|dep| !remaining.contains(dep) || dep == *cpv)
                    })
                    .unwrap_or(true)
            })
            .cloned()
            .collect();

        if ready.is_empty() {
            // Remaining nodes form a cycle.
            let cycle: Vec<String> = remaining.into_iter().collect();
            return Err(ResolveFailure::CircularDependency(cycle));
        }

        // Emit in deterministic cpv order.
        for cpv in ready {
            remaining.remove(&cpv);
            emitted.push(cpv);
        }
    }

    Ok(emitted)
}

/// Parses a dependency token into an atom plus an optional blocker strength
/// (`Some(true)` for `!!`, `Some(false)` for `!`, `None` for a plain dep).
fn parse_dep_token(token: &str) -> Result<(Atom, Option<bool>), ResolveFailure> {
    let atom = Atom::parse_with_options(token, DEPENDENCY_ATOM_OPTIONS)
        .map_err(|err| ResolveFailure::InvalidDependency(format!("{token}: {err}")))?;
    let blocker = match atom.blocker {
        Some(Blocker::Strong) => Some(true),
        Some(Blocker::Weak) => Some(false),
        None => None,
    };
    Ok((atom, blocker))
}

/// True when `cpv` (looked up in `db`) matches `atom`, ignoring the atom's
/// blocker prefix.
fn package_matches_atom(db: &PackageDb, cpv: &str, atom: &Atom) -> bool {
    let mut bare = atom.clone();
    bare.blocker = None;
    db.match_atom(&bare).iter().any(|m| m == cpv)
}

/// Renders an atom back to its request string (for error messages), stripping
/// any blocker prefix.
fn atom_to_request(atom: &Atom) -> String {
    let mut bare = atom.clone();
    bare.blocker = None;
    bare.to_string()
}
