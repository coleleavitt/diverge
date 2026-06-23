//! Atom-to-candidate matching, ported from Portage's `portage.dep`.
//!
//! This module reproduces the observable behavior of upstream
//! `match_from_list`, `best_match_to_list`, and the USE-dependency / slot /
//! repository filtering they apply. The [`Candidate`] type is the shared,
//! typed package view the resolver consumes, so matching is defined once and
//! reused rather than duplicated as ad-hoc string scanning.
//!
//! Reference: `research/portage/lib/portage/dep/__init__.py`
//! (`match_from_list`, `best_match_to_list`, `extended_cp_match`).

use std::cmp::Ordering;
use std::collections::BTreeSet;

use crate::atom::{Atom, Operator};
use crate::dep::DepError;
use crate::version::{split_cpv, vercmp};

/// A concrete package a request can match against: a `category/package-version`
/// plus optional slot/sub-slot, repository, enabled USE flags, and the set of
/// flags the package actually declares in `IUSE`.
///
/// Mirrors the minimal `_emerge.Package`-like view upstream's
/// `match_from_list` accepts (see the test's `Package` shim).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub cpv: String,
    pub slot: Option<String>,
    pub sub_slot: Option<String>,
    pub repo: Option<String>,
    pub use_enabled: BTreeSet<String>,
    pub iuse: BTreeSet<String>,
}

impl Candidate {
    /// Builds a candidate from a bare `category/package-version` string with
    /// no slot/use/repo metadata.
    pub fn new(cpv: impl Into<String>) -> Self {
        Self {
            cpv: cpv.into(),
            slot: None,
            sub_slot: None,
            repo: None,
            use_enabled: BTreeSet::new(),
            iuse: BTreeSet::new(),
        }
    }

    /// Parses a fully-qualified atom string (e.g. `=dev-libs/A-1:2/3::repo[foo]`)
    /// into the package it describes, the way upstream's test `Package` shim
    /// constructs candidates from atoms.
    pub fn from_atom_str(input: &str) -> Result<Self, crate::atom::AtomError> {
        let atom = Atom::parse_with_options(
            input,
            crate::atom::AtomParseOptions {
                allow_wildcard: false,
                allow_repo: true,
            },
        )?;
        let mut use_enabled = BTreeSet::new();
        let mut iuse = BTreeSet::new();
        if let Some(parsed) = atom.parsed_use_deps() {
            for flag in &parsed.tokens {
                iuse.insert(flag.name.clone());
                if !flag.negated && flag.default.is_none() {
                    use_enabled.insert(flag.name.clone());
                }
            }
        }
        Ok(Self {
            cpv: atom.cpv(),
            slot: atom.slot().map(str::to_string),
            sub_slot: atom.sub_slot().map(str::to_string),
            repo: atom.repo.clone(),
            use_enabled,
            iuse,
        })
    }

    pub fn with_slot(mut self, slot: impl Into<String>) -> Self {
        self.slot = Some(slot.into());
        self
    }

    pub fn with_sub_slot(mut self, sub_slot: impl Into<String>) -> Self {
        self.sub_slot = Some(sub_slot.into());
        self
    }

    pub fn with_repo(mut self, repo: impl Into<String>) -> Self {
        self.repo = Some(repo.into());
        self
    }

    pub fn with_use<I, S>(mut self, flags: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.use_enabled = flags.into_iter().map(Into::into).collect();
        self
    }

    pub fn with_iuse<I, S>(mut self, flags: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.iuse = flags.into_iter().map(Into::into).collect();
        self
    }

    fn cp(&self) -> String {
        split_cpv(&self.cpv).0
    }

    fn version(&self) -> Option<String> {
        split_cpv(&self.cpv).1
    }

    fn is_valid_flag(&self, flag: &str) -> bool {
        self.iuse.contains(flag)
    }
}

/// Default for a USE dependency flag (`foo(+)` / `foo(-)`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UseDefault {
    Enabled,
    Disabled,
}

/// One parsed USE-dependency flag token, e.g. `-foo(+)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UseDepToken {
    pub name: String,
    pub negated: bool,
    pub default: Option<UseDefault>,
}

/// All USE-dependency tokens in an atom's `[...]` group.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ParsedUseDeps {
    pub tokens: Vec<UseDepToken>,
}

/// Port of `extended_cp_match`: matches an extended-syntax cp (with `*`
/// wildcards) against a concrete cp. `*` matches any run of non-`/` chars.
fn extended_cp_match(pattern: &[u8], text: &[u8]) -> bool {
    let Some((&head, rest)) = pattern.split_first() else {
        return text.is_empty();
    };
    if head != b'*' {
        return !text.is_empty() && text[0] == head && extended_cp_match(rest, &text[1..]);
    }
    // `*` matches any (possibly empty) run of non-'/' characters.
    if extended_cp_match(rest, text) {
        return true;
    }
    let consumable = text.iter().take_while(|&&b| b != b'/').count();
    (1..=consumable).any(|idx| extended_cp_match(rest, &text[idx..]))
}

/// Normalizes the version part for `=*` prefix comparison the way upstream
/// does: strip leading zeros, and if the result is empty or no longer starts
/// with a digit, prepend a `0` (so `01` and `1` compare on the same boundary).
fn normalize_glob_version(version: &str) -> String {
    let stripped = version.trim_start_matches('0');
    if stripped.is_empty() || !stripped.starts_with(|c: char| c.is_ascii_digit()) {
        format!("0{stripped}")
    } else {
        stripped.to_string()
    }
}

/// Rebuilds a `cp-version` string with the version's leading zeros normalized,
/// preserving any `-rN` revision. Mirrors upstream's `cpv.replace(...)`.
fn normalized_cpv(cp: &str, version: &str) -> String {
    let normalized = normalize_glob_version(version);
    if normalized == version {
        format!("{cp}-{version}")
    } else {
        format!("{cp}-{normalized}")
    }
}

fn glob_matches(atom_cp: &str, atom_version: &str, candidate: &Candidate) -> bool {
    let Some(cand_version) = candidate.version() else {
        return false;
    };
    let target = normalized_cpv(atom_cp, atom_version);
    let actual = normalized_cpv(&candidate.cp(), &cand_version);
    if !actual.starts_with(&target) {
        return false;
    }
    // `=*` matches only on boundaries between version parts, so `1*` does not
    // match `10` (bug 560466).
    match actual[target.len()..].chars().next() {
        None => true,
        Some('.' | '_' | '-') => true,
        Some(c) => {
            let prev_digit = target.chars().last().is_some_and(|p| p.is_ascii_digit());
            prev_digit != c.is_ascii_digit()
        }
    }
}

fn cpv_equal(left: &str, right: &str) -> bool {
    let (left_cp, left_ver) = split_cpv(left);
    let (right_cp, right_ver) = split_cpv(right);
    left_cp == right_cp
        && match (left_ver, right_ver) {
            (Some(l), Some(r)) => vercmp(&l, &r) == Ordering::Equal,
            (None, None) => true,
            _ => false,
        }
}

fn version_only(version: &str) -> &str {
    // Strip a trailing `-rN` revision to get the upstream version proper.
    match version.rsplit_once("-r") {
        Some((base, rev)) if !base.is_empty() && rev.chars().all(|c| c.is_ascii_digit()) => base,
        _ => version,
    }
}

/// Keeps candidates whose version satisfies an ordering operator vs `want`.
fn ordering_keeps(op: Operator, result: Ordering) -> bool {
    match op {
        Operator::Greater => result == Ordering::Greater,
        Operator::GreaterEqual => matches!(result, Ordering::Greater | Ordering::Equal),
        Operator::Less => result == Ordering::Less,
        Operator::LessEqual => matches!(result, Ordering::Less | Ordering::Equal),
        _ => false,
    }
}

/// Candidates matching an extended-syntax (wildcard `cp`) atom.
fn match_extended<'a>(
    atom: &Atom,
    atom_cp: &str,
    candidates: &'a [Candidate],
) -> Vec<&'a Candidate> {
    let mut matched: Vec<&Candidate> = candidates
        .iter()
        .filter(|candidate| {
            let cp = candidate.cp();
            cp == atom_cp || extended_cp_match(atom_cp.as_bytes(), cp.as_bytes())
        })
        .collect();

    if matches!(atom.operator, Some(Operator::EqualGlob))
        && let Some(version) = &atom.version
    {
        let needle = version.trim_end_matches('*').to_string();
        matched.retain(|c| c.version().is_some_and(|v| v.contains(&needle)));
    }
    matched
}

/// Candidates matching a non-extended atom by its (possibly absent) operator.
fn match_by_operator<'a>(
    atom: &Atom,
    atom_cp: &str,
    candidates: &'a [Candidate],
) -> Vec<&'a Candidate> {
    let want = atom.version.clone().unwrap_or_default();
    match atom.operator {
        None => candidates.iter().filter(|c| c.cp() == atom_cp).collect(),
        Some(Operator::Equal) => {
            let target = atom.cpv();
            candidates
                .iter()
                .filter(|c| cpv_equal(&c.cpv, &target))
                .collect()
        }
        Some(Operator::EqualGlob) => {
            let version = want.trim_end_matches('*');
            candidates
                .iter()
                .filter(|c| c.cp() == atom_cp && glob_matches(atom_cp, version, c))
                .collect()
        }
        Some(Operator::Tilde) => candidates
            .iter()
            .filter(|c| c.cp() == atom_cp && c.version().is_some_and(|v| version_only(&v) == want))
            .collect(),
        Some(op) => candidates
            .iter()
            .filter(|c| {
                c.cp() == atom_cp
                    && c.version()
                        .is_some_and(|v| ordering_keeps(op, vercmp(&v, &want)))
            })
            .collect(),
    }
}

fn slot_matches(atom: &Atom, candidate: &Candidate) -> bool {
    let Some(want_slot) = atom.slot() else {
        return true;
    };
    let Some(have_slot) = &candidate.slot else {
        // Candidate carries no slot data: upstream keeps it (cannot disprove).
        return true;
    };
    if want_slot != have_slot {
        return false;
    }
    match (atom.sub_slot(), &candidate.sub_slot) {
        (Some(want_sub), Some(have_sub)) => want_sub == have_sub,
        (Some(_), None) => false,
        _ => true,
    }
}

fn use_matches(atom: &Atom, candidate: &Candidate) -> bool {
    let Some(parsed) = atom.parsed_use_deps() else {
        return true;
    };
    // All non-defaulted flags must be declared in the candidate's IUSE.
    if parsed
        .tokens
        .iter()
        .any(|t| t.default.is_none() && !candidate.is_valid_flag(&t.name))
    {
        return false;
    }

    let missing = |want: UseDefault| -> BTreeSet<&str> {
        parsed
            .tokens
            .iter()
            .filter(|t| t.default == Some(want) && !candidate.is_valid_flag(&t.name))
            .map(|t| t.name.as_str())
            .collect()
    };
    let missing_enabled = missing(UseDefault::Enabled);
    let missing_disabled = missing(UseDefault::Disabled);

    let names = |negated: bool| -> BTreeSet<&str> {
        parsed
            .tokens
            .iter()
            .filter(|t| t.negated == negated)
            .map(|t| t.name.as_str())
            .collect()
    };
    let enabled = names(false);
    let disabled = names(true);

    enabled_constraints_hold(&enabled, candidate, &missing_enabled, &missing_disabled)
        && disabled_constraints_hold(&disabled, candidate, &missing_enabled, &missing_disabled)
}

fn enabled_constraints_hold(
    enabled: &BTreeSet<&str>,
    candidate: &Candidate,
    missing_enabled: &BTreeSet<&str>,
    missing_disabled: &BTreeSet<&str>,
) -> bool {
    if enabled.is_empty() {
        return true;
    }
    if enabled.iter().any(|f| missing_disabled.contains(f)) {
        return false;
    }
    let need: BTreeSet<&str> = enabled
        .iter()
        .copied()
        .filter(|f| !candidate.use_enabled.contains(*f))
        .collect();
    !(!need.is_empty() && need.iter().any(|f| !missing_enabled.contains(f)))
}

fn disabled_constraints_hold(
    disabled: &BTreeSet<&str>,
    candidate: &Candidate,
    missing_enabled: &BTreeSet<&str>,
    missing_disabled: &BTreeSet<&str>,
) -> bool {
    if disabled.is_empty() {
        return true;
    }
    if disabled.iter().any(|f| missing_enabled.contains(f)) {
        return false;
    }
    let need: BTreeSet<&str> = disabled
        .iter()
        .copied()
        .filter(|f| candidate.use_enabled.contains(*f))
        .collect();
    !(!need.is_empty() && need.iter().any(|f| !missing_disabled.contains(f)))
}

fn repo_matches(atom: &Atom, candidate: &Candidate) -> bool {
    let Some(want) = &atom.repo else {
        return true;
    };
    match &candidate.repo {
        Some(have) => have == want,
        None => true,
    }
}

/// Port of `match_from_list`: returns the candidates that match `atom`,
/// preserving input order. Blocker prefixes on the atom are ignored (upstream
/// strips them before matching).
pub fn match_from_list<'a>(atom: &Atom, candidates: &'a [Candidate]) -> Vec<&'a Candidate> {
    if candidates.is_empty() {
        return Vec::new();
    }
    let atom_cp = atom.cp();
    let mut matched = if atom_cp.contains('*') {
        match_extended(atom, &atom_cp, candidates)
    } else {
        match_by_operator(atom, &atom_cp, candidates)
    };

    matched.retain(|candidate| slot_matches(atom, candidate));
    matched.retain(|candidate| use_matches(atom, candidate));
    matched.retain(|candidate| repo_matches(atom, candidate));
    matched
}

/// Operator precedence used by [`best_match_to_list`], mirroring upstream's
/// `operator_values`.
fn operator_value(atom: &Atom) -> i32 {
    if atom.cp().contains('*') {
        return match atom.operator {
            Some(Operator::EqualGlob) => 0,
            _ if atom.slot().is_some() => -1,
            _ => -2,
        };
    }
    match atom.operator {
        Some(Operator::Equal) => 6,
        Some(Operator::Tilde) => 5,
        Some(Operator::EqualGlob) => 4,
        Some(Operator::Greater | Operator::Less | Operator::GreaterEqual | Operator::LessEqual) => {
            2
        }
        None => 1,
    }
}

/// Port of `best_match_to_list`: of the atoms in `atom_list` that match
/// `candidate`, returns the most specific one by upstream's precedence table.
pub fn best_match_to_list<'a>(candidate: &Candidate, atom_list: &'a [Atom]) -> Option<&'a Atom> {
    let pool = [candidate.clone()];
    let mut max_value = -99;
    let mut best: Option<&Atom> = None;

    for atom in atom_list {
        if match_from_list(atom, &pool).is_empty() {
            continue;
        }
        let extended = atom.cp().contains('*');
        if !extended && atom.slot().is_some() && max_value < 3 {
            max_value = 3;
            best = Some(atom);
        }
        let value = operator_value(atom);
        if value > max_value {
            max_value = value;
            best = Some(atom);
        } else if value == max_value && value == 2 {
            best = closer_ordering_atom(best, atom, candidate);
        }
    }

    best
}

/// For tied ordering operators, prefer an atom whose version exactly equals the
/// candidate's; otherwise keep the incumbent (upstream's closeness tie-break).
fn closer_ordering_atom<'a>(
    best: Option<&'a Atom>,
    atom: &'a Atom,
    candidate: &Candidate,
) -> Option<&'a Atom> {
    let Some(cand_version) = candidate.version() else {
        return best;
    };
    let this_version = atom.version.clone().unwrap_or_default();
    if vercmp(&this_version, &cand_version) == Ordering::Equal {
        return Some(atom);
    }
    best
}

/// Port of `get_required_use_flags`: returns the set of USE flags referenced
/// by a REQUIRED_USE string, validating bracket/operator structure.
pub fn get_required_use_flags(required_use: &str) -> Result<BTreeSet<String>, DepError> {
    let valid_operators = ["||", "^^", "??"];
    let mut level = 0usize;
    let mut need_bracket = false;
    let mut used = BTreeSet::new();
    let bad = || DepError(format!("malformed syntax: '{required_use}'"));

    let register = |token: &str, used: &mut BTreeSet<String>| {
        let flag = token.strip_prefix('!').unwrap_or(token);
        let flag = flag.strip_suffix('?').unwrap_or(flag);
        if !flag.is_empty() {
            used.insert(flag.to_string());
        }
    };

    for token in required_use.split_whitespace() {
        if token == "(" {
            need_bracket = false;
            level += 1;
        } else if token == ")" {
            if need_bracket || level == 0 {
                return Err(bad());
            }
            level -= 1;
        } else if valid_operators.contains(&token) {
            if need_bracket {
                return Err(bad());
            }
            need_bracket = true;
        } else if let Some(stripped) = token.strip_suffix('?') {
            if need_bracket {
                return Err(bad());
            }
            need_bracket = true;
            register(stripped, &mut used);
        } else {
            if need_bracket {
                return Err(bad());
            }
            register(token, &mut used);
        }
    }

    if level != 0 || need_bracket {
        return Err(bad());
    }

    Ok(used)
}
