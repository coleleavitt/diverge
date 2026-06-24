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

/// Upper bound on backtracking iterations (constraint-propagation passes).
const MAX_BACKTRACK: usize = 16;

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
    /// Backtracking pins: cp -> the single cpv that may satisfy that cp.
    pins: BTreeMap<String, String>,
    /// Every non-blocker dependency/target atom encountered, by cp, used to
    /// detect slot conflicts and compute a shared version when backtracking.
    atoms_by_cp: BTreeMap<String, Vec<Atom>>,
}

/// The result of one resolution pass: either a finished merge list or a slot
/// conflict that backtracking may be able to resolve by pinning a version.
enum PassResult {
    Resolved(Vec<String>),
    /// A cp had conflicting selections; the value is the cp to re-pin.
    Conflict(String),
    Failed(ResolveFailure),
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
        self.select_available_pinned(atom, &BTreeMap::new())
    }

    /// Like [`Self::select_available`], but honors backtracking `pins`: if the
    /// atom's cp is pinned to a specific cpv, only that cpv may be selected
    /// (and only when it also matches the atom). This lets the backtracker
    /// force one shared version across conflicting constraints.
    fn select_available_pinned(
        &self,
        atom: &Atom,
        pins: &BTreeMap<String, String>,
    ) -> Option<String> {
        let mut matches = self.available.match_atom(atom);
        matches.retain(|cpv| {
            self.available
                .metadata(cpv)
                .map(|m| self.params.keyword_visible(&m.keywords))
                .unwrap_or(false)
        });
        if let Some(pinned) = pins.get(&atom.cp()) {
            matches.retain(|cpv| cpv == pinned);
        }
        // match_atom already returns cpvs sorted ascending, so the best
        // (highest version) visible candidate is the last one.
        matches.pop()
    }

    /// True when an installed package already satisfies `atom`.
    fn installed_satisfies(&self, atom: &Atom) -> bool {
        !self.installed.match_atom(atom).is_empty()
    }

    /// The sub-slot of an available cpv, if it declares one.
    fn available_sub_slot(&self, cpv: &str) -> Option<String> {
        self.available
            .metadata(cpv)
            .and_then(|m| m.sub_slot.clone())
    }

    /// Selects the best visible available cpv for a bare `category/package`.
    fn select_available_cp(&self, cp: &str) -> Option<String> {
        Atom::parse_with_options(cp, DEPENDENCY_ATOM_OPTIONS)
            .ok()
            .and_then(|atom| self.select_available(&atom))
    }

    /// Finds installed packages with a slot-operator (`:slot/sub=`) dependency
    /// on `dep_cp`, returning each `(installed_cpv, bound_sub_slot)`.
    ///
    /// Installed deps record the bound sub-slot literally, e.g.
    /// `app-misc/A:0/1=`. This scans the installed packages' dependency strings
    /// for such tokens.
    fn installed_slot_op_bindings(&self, dep_cp: &str) -> Vec<(String, String)> {
        let mut bindings = Vec::new();
        for inst_cpv in self.installed.cpv_all() {
            let Some(meta) = self.installed.metadata(&inst_cpv) else {
                continue;
            };
            for dep_str in meta.deps.values() {
                for token in dep_str.split_whitespace() {
                    if let Some(sub) = slot_op_binding(token, dep_cp) {
                        bindings.push((inst_cpv.clone(), sub));
                    }
                }
            }
        }
        bindings
    }

    /// Resolves a set of target atoms into a merge plan, backtracking over slot
    /// conflicts by pinning a cp to a version satisfying all its constraints.
    pub fn resolve(&self, targets: &[&str]) -> ResolveOutcome {
        // Parse targets once; an invalid target fails immediately.
        let parsed: Vec<Atom> = match targets
            .iter()
            .map(|t| Atom::parse_with_options(t, DEPENDENCY_ATOM_OPTIONS))
            .collect::<Result<Vec<_>, _>>()
        {
            Ok(atoms) => atoms,
            Err(err) => {
                return ResolveOutcome {
                    mergelist: Vec::new(),
                    error: Some(ResolveFailure::InvalidDependency(err.to_string())),
                };
            }
        };

        let mut pins: BTreeMap<String, String> = BTreeMap::new();
        for _ in 0..MAX_BACKTRACK {
            match self.resolve_pass(&parsed, &pins) {
                PassResult::Resolved(mergelist) => {
                    return ResolveOutcome {
                        mergelist,
                        error: None,
                    };
                }
                PassResult::Failed(failure) => {
                    return ResolveOutcome {
                        mergelist: Vec::new(),
                        error: Some(failure),
                    };
                }
                PassResult::Conflict(cp) => {
                    // Compute a version of `cp` satisfying every constraint and
                    // pin it, then re-resolve. If we can't, the conflict stands.
                    match self.shared_version(&cp, &parsed) {
                        Some(cpv) if pins.get(&cp) != Some(&cpv) => {
                            pins.insert(cp, cpv);
                        }
                        _ => {
                            return ResolveOutcome {
                                mergelist: Vec::new(),
                                error: Some(ResolveFailure::Unsatisfied(cp)),
                            };
                        }
                    }
                }
            }
        }
        ResolveOutcome {
            mergelist: Vec::new(),
            error: Some(ResolveFailure::Unsatisfied(
                "backtrack limit exceeded".to_string(),
            )),
        }
    }

    /// Runs a single resolution pass with the given pins.
    fn resolve_pass(&self, targets: &[Atom], pins: &BTreeMap<String, String>) -> PassResult {
        let mut builder = GraphBuilder {
            resolver: self,
            edges: BTreeMap::new(),
            selected: BTreeSet::new(),
            blockers: Vec::new(),
            in_progress: Vec::new(),
            pins: pins.clone(),
            atoms_by_cp: BTreeMap::new(),
        };

        for atom in targets {
            if let Err(failure) = builder.expand_atom(atom, true) {
                return PassResult::Failed(failure);
            }
        }

        // Detect a slot conflict: a cp whose constraints cannot all be met by
        // the single version currently selected.
        if let Some(cp) = builder.first_conflicting_cp() {
            return PassResult::Conflict(cp);
        }

        match builder.finish() {
            Ok(mergelist) => PassResult::Resolved(mergelist),
            Err(failure) => PassResult::Failed(failure),
        }
    }

    /// Computes the highest visible version of `cp` that satisfies every
    /// recorded constraint atom for that cp (from a probe pass), if any.
    fn shared_version(&self, cp: &str, targets: &[Atom]) -> Option<String> {
        // Re-run a probe pass to gather all atoms that constrain `cp`.
        let mut probe = GraphBuilder {
            resolver: self,
            edges: BTreeMap::new(),
            selected: BTreeSet::new(),
            blockers: Vec::new(),
            in_progress: Vec::new(),
            pins: BTreeMap::new(),
            atoms_by_cp: BTreeMap::new(),
        };
        for atom in targets {
            let _ = probe.expand_atom(atom, true);
        }
        let constraints = probe.atoms_by_cp.get(cp)?;

        let mut candidates = self.available.cpv_all();
        candidates.retain(|cpv| crate::version::split_cpv(cpv).0 == cp);
        candidates.retain(|cpv| {
            self.available
                .metadata(cpv)
                .map(|m| self.params.keyword_visible(&m.keywords))
                .unwrap_or(false)
        });
        // Keep only versions satisfying every constraint atom.
        candidates.retain(|cpv| {
            constraints
                .iter()
                .all(|atom| package_matches_atom(self.available, cpv, atom))
        });
        // cpv_all is sorted ascending; the best shared version is the highest.
        candidates.pop()
    }
}

impl GraphBuilder<'_> {
    /// Expands a single atom: select a package, record it, and recurse into its
    /// dependencies. `is_target` distinguishes a top-level request (always
    /// considered for merge) from a transitive dependency (may be satisfied by
    /// an installed package).
    fn expand_atom(&mut self, atom: &Atom, is_target: bool) -> Result<(), ResolveFailure> {
        // Record every non-blocker atom by cp for slot-conflict detection.
        if atom.blocker.is_none() {
            self.atoms_by_cp
                .entry(atom.cp())
                .or_default()
                .push(atom.clone());
        }

        // A transitive dependency already satisfied by an installed package is
        // not re-merged (emerge's default without --deep/--update/--newuse).
        // Top-level targets are always considered for (re)installation.
        if !is_target && self.resolver.installed_satisfies(atom) {
            return Ok(());
        }

        let Some(cpv) = self.resolver.select_available_pinned(atom, &self.pins) else {
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
            if let Some(dep_cpv) = self.resolver.select_available_pinned(&dep_atom, &self.pins) {
                self.edges.entry(cpv.clone()).or_default().insert(dep_cpv);
            }
            self.expand_atom(&dep_atom, false)?;
        }

        self.in_progress.pop();
        Ok(())
    }

    /// Collects the dependency atoms of `cpv` by reducing its dependency
    /// strings with the active USE flags, resolving `|| ( ... )` choices with
    /// cross-choice overlap minimization (bug 632026).
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
        let mut or_choices: Vec<Vec<Vec<(Atom, Option<bool>)>>> = Vec::new();
        for key in &self.resolver.params.dep_keys {
            let Some(dep_str) = metadata.deps.get(key) else {
                continue;
            };
            if dep_str.trim().is_empty() {
                continue;
            }
            let reduced = use_reduce(dep_str, &options)
                .map_err(|err| ResolveFailure::InvalidDependency(err.to_string()))?;
            self.collect_atoms(&reduced, &mut atoms, &mut or_choices)?;
        }
        // Resolve the package's || ( ... ) choices together so overlapping
        // alternatives like "|| ( A B ) || ( B C )" share a provider (B).
        self.resolve_or_choices(&or_choices, &mut atoms);
        Ok(atoms)
    }

    /// Walks a reduced dependency structure, pushing plain atoms into `out` and
    /// collecting each `|| ( ... )` choice's branch list into `or_choices` for
    /// later overlap-aware resolution.
    fn collect_atoms(
        &self,
        nodes: &[Dep],
        out: &mut Vec<(Atom, Option<bool>)>,
        or_choices: &mut Vec<Vec<Vec<(Atom, Option<bool>)>>>,
    ) -> Result<(), ResolveFailure> {
        let mut iter = nodes.iter().peekable();
        while let Some(node) = iter.next() {
            match node {
                Dep::Token(token) if token == "||" => {
                    if let Some(Dep::Group(choices)) = iter.next() {
                        or_choices.push(self.choice_branches(choices)?);
                    }
                }
                Dep::Token(token) => {
                    let (atom, blocker) = parse_dep_token(token)?;
                    out.push((atom, blocker));
                }
                Dep::Group(inner) => self.collect_atoms(inner, out, or_choices)?,
            }
        }
        Ok(())
    }

    /// Reduces an `|| ( ... )` group into its branches; each branch is a flat
    /// atom list (a parenthesized branch contributes all of its atoms).
    fn choice_branches(
        &self,
        choices: &[Dep],
    ) -> Result<Vec<Vec<(Atom, Option<bool>)>>, ResolveFailure> {
        let mut branches = Vec::new();
        for choice in choices {
            let mut branch = Vec::new();
            let mut nested = Vec::new();
            match choice {
                Dep::Token(token) => {
                    let (atom, blocker) = parse_dep_token(token)?;
                    branch.push((atom, blocker));
                }
                Dep::Group(inner) => self.collect_atoms(inner, &mut branch, &mut nested)?,
            }
            branches.push(branch);
        }
        Ok(branches)
    }

    /// Resolves every collected `|| ( ... )` choice, preferring (in order):
    /// an installed-satisfied branch, a branch already chosen by an earlier
    /// choice, a branch overlapping another unresolved choice (minimize the
    /// number of providers), then the first available branch.
    fn resolve_or_choices(
        &self,
        or_choices: &[Vec<Vec<(Atom, Option<bool>)>>],
        out: &mut Vec<(Atom, Option<bool>)>,
    ) {
        let mut committed: BTreeSet<String> = BTreeSet::new();
        for (index, branches) in or_choices.iter().enumerate() {
            let chosen = self.pick_branch(branches, &committed, or_choices, index);
            for (atom, blk) in &chosen {
                if blk.is_none()
                    && let Some(cpv) = self.resolver.select_available(atom)
                {
                    committed.insert(cpv);
                }
            }
            out.extend(chosen);
        }
    }

    /// Picks the best branch of one `|| ( ... )` choice given the providers
    /// already committed by earlier choices and the remaining choices.
    fn pick_branch(
        &self,
        branches: &[Vec<(Atom, Option<bool>)>],
        committed: &BTreeSet<String>,
        all_choices: &[Vec<Vec<(Atom, Option<bool>)>>],
        index: usize,
    ) -> Vec<(Atom, Option<bool>)> {
        // 1. A branch already satisfied by installed packages.
        if let Some(branch) = branches.iter().find(|b| {
            b.iter()
                .all(|(a, blk)| blk.is_some() || self.resolver.installed_satisfies(a))
        }) {
            return branch.clone();
        }
        // 2. A branch whose providers are already committed by an earlier choice.
        if let Some(branch) = branches.iter().find(|b| {
            b.iter().all(|(a, blk)| {
                blk.is_some()
                    || self
                        .resolver
                        .select_available(a)
                        .is_some_and(|cpv| committed.contains(&cpv))
            })
        }) {
            return branch.clone();
        }
        // 3. A branch whose provider also satisfies a later, not-yet-resolved
        //    choice (so one package covers both) — minimize children.
        if let Some(branch) = branches.iter().find(|b| {
            b.iter().any(|(a, blk)| {
                blk.is_none()
                    && self
                        .resolver
                        .select_available(a)
                        .is_some_and(|cpv| self.satisfies_other_choice(&cpv, all_choices, index))
            })
        }) {
            return branch.clone();
        }
        // 4. The first branch whose atoms are all available.
        if let Some(branch) = branches.iter().find(|b| {
            b.iter()
                .all(|(a, blk)| blk.is_some() || self.resolver.select_available(a).is_some())
        }) {
            return branch.clone();
        }
        // 5. Fall back to the first branch (surfaces an unsatisfied error later).
        branches.first().cloned().unwrap_or_default()
    }

    /// True when `cpv` satisfies at least one branch-atom of some choice other
    /// than `index` (used to favor providers shared across `|| ( ... )` groups).
    fn satisfies_other_choice(
        &self,
        cpv: &str,
        all_choices: &[Vec<Vec<(Atom, Option<bool>)>>],
        index: usize,
    ) -> bool {
        all_choices.iter().enumerate().any(|(other, branches)| {
            other != index
                && branches.iter().any(|b| {
                    b.iter().any(|(a, blk)| {
                        blk.is_none() && package_matches_atom(self.resolver.available, cpv, a)
                    })
                })
        })
    }

    /// Finalizes the graph: triggers slot-operator rebuilds, checks blockers,
    /// then topologically orders the selected packages so dependencies precede
    /// dependents.
    fn finish(mut self) -> Result<Vec<String>, ResolveFailure> {
        self.apply_slot_operator_rebuilds();
        self.check_blockers()?;
        topological_order(&self.selected, &self.edges)
    }

    /// Pulls in slot-operator rebuilds: an installed package whose recorded
    /// `:slot/sub=` dependency is being upgraded to a different sub-slot must be
    /// rebuilt against the new sub-slot (mirrors `test_slot_operator_rebuild`).
    ///
    /// For each dependency cpv selected for merge, find installed packages that
    /// declare a `:=`/`:slot=` dep on its cp bound to a different sub-slot, and
    /// add the available rebuild of that dependent to the merge graph.
    fn apply_slot_operator_rebuilds(&mut self) {
        let mut rebuilds: Vec<(String, String)> = Vec::new();
        for dep_cpv in self.selected.clone() {
            let Some(new_sub) = self.resolver.available_sub_slot(&dep_cpv) else {
                continue;
            };
            let (dep_cp, _) = crate::version::split_cpv(&dep_cpv);
            for (inst_cpv, bound_sub) in self.resolver.installed_slot_op_bindings(&dep_cp) {
                if bound_sub == new_sub {
                    continue; // Already built against this sub-slot.
                }
                let (inst_cp, _) = crate::version::split_cpv(&inst_cpv);
                if let Some(rebuild) = self.resolver.select_available_cp(&inst_cp) {
                    rebuilds.push((rebuild, dep_cpv.clone()));
                }
            }
        }
        for (rebuild_cpv, dep_cpv) in rebuilds {
            if self.selected.insert(rebuild_cpv.clone()) {
                self.edges.entry(rebuild_cpv.clone()).or_default();
            }
            // The rebuild must merge after its (new sub-slot) dependency.
            self.edges.entry(rebuild_cpv).or_default().insert(dep_cpv);
        }
    }

    /// Returns the first cp with a slot conflict: two or more selected versions
    /// that occupy the same cp+slot. Such a cp can be re-pinned to a single
    /// shared version by the backtracker.
    fn first_conflicting_cp(&self) -> Option<String> {
        // Group selected cpvs by (cp, slot); a cp+slot with >1 version conflicts.
        let mut by_cp_slot: BTreeMap<(String, String), BTreeSet<String>> = BTreeMap::new();
        for cpv in &self.selected {
            let (cp, _) = crate::version::split_cpv(cpv);
            let slot = self
                .resolver
                .available
                .metadata(cpv)
                .and_then(|m| m.slot.clone())
                .unwrap_or_else(|| "0".to_string());
            by_cp_slot
                .entry((cp, slot))
                .or_default()
                .insert(cpv.clone());
        }
        by_cp_slot
            .into_iter()
            .find(|(_, cpvs)| cpvs.len() > 1)
            .map(|((cp, _), _)| cp)
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

/// If `token` is a slot-operator dependency on `dep_cp` that records a bound
/// sub-slot (e.g. `cat/pkg:0/1=` for `dep_cp == "cat/pkg"`), returns that
/// sub-slot. A bare `cat/pkg:=` (no recorded sub-slot) returns `None`.
fn slot_op_binding(token: &str, dep_cp: &str) -> Option<String> {
    let bare = token.trim_start_matches('!');
    let suffix = bare.strip_suffix('=')?;
    let (cp_slot, sub) = suffix.split_once(':')?;
    if cp_slot != dep_cp {
        return None;
    }
    // `sub` is `slot` or `slot/sub`; the bound sub-slot is after the `/`.
    sub.split_once('/').map(|(_, sub)| sub.to_string())
}
