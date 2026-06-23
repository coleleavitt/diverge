//! Dependency-string domain helpers ported from Portage's `portage/dep`.
//!
//! These mirror the observable behavior of the upstream functions in
//! `research/portage/lib/portage/dep/__init__.py`. Each port cites the
//! upstream function it reproduces. The parsers operate on the same token
//! grammar emerge uses for `DEPEND`/`RDEPEND`/`REQUIRED_USE` strings.

use std::collections::HashSet;
use std::fmt;

use crate::atom::{Atom, AtomParseOptions};
use crate::version::is_version;

/// A reduced dependency node: either a bare token (atom, operator such as
/// `||`, or a `use?` conditional) or a parenthesized group. Mirrors the
/// nested list/str structure Portage's `paren_reduce`/`use_reduce` return.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Dep {
    Token(String),
    Group(Vec<Dep>),
}

impl Dep {
    fn token(value: &str) -> Self {
        Dep::Token(value.to_string())
    }

    fn is_or(&self) -> bool {
        matches!(self, Dep::Token(t) if t == "||")
    }

    fn ends_with_question(&self) -> bool {
        matches!(self, Dep::Token(t) if t.ends_with('?'))
    }
}

/// Error raised for malformed dependency strings, mirroring Portage's
/// `InvalidDependString`/`InvalidAtom` failure paths.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DepError(pub String);

impl fmt::Display for DepError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for DepError {}

fn malformed(input: &str) -> DepError {
    DepError(format!("malformed syntax: '{input}'"))
}

// ----------------------------------------------------------------------------
// Atom accessors (portage.dep.dep_get*)
// ----------------------------------------------------------------------------

/// Port of `portage.dep.get_operator`: returns the operator of an atom, with
/// `=*` for the equal-glob form, or `None` when there is no operator.
///
/// Mirrors upstream's bare `Atom(mydep)` construction (no repo/wildcard
/// allowance), so repository-qualified input is treated as invalid.
pub fn get_operator(mydep: &str) -> Option<String> {
    Atom::parse_with_options(mydep, AtomParseOptions::default())
        .ok()?
        .operator
        .map(|op| op.as_portage_str().to_string())
}

/// Port of `portage.dep.dep_getcpv`: strips operator/slot/use/repo and returns
/// the bare `category/package-version`. Mirrors upstream's bare `Atom(mydep)`.
pub fn dep_getcpv(mydep: &str) -> Option<String> {
    Atom::parse_with_options(mydep, AtomParseOptions::default())
        .ok()
        .map(|atom| atom.cpv())
}

/// Port of `portage.dep.dep_getkey`: returns `category/package`.
pub fn dep_getkey(mydep: &str) -> Option<String> {
    Atom::parse_with_options(
        mydep,
        AtomParseOptions {
            allow_wildcard: true,
            allow_repo: true,
        },
    )
    .ok()
    .map(|atom| atom.cp())
}

/// Port of `portage.dep.dep_getslot`: returns the slot text (`slot`,
/// `slot/sub`, or with a `=`/`*` operator) or `None`.
///
/// String-based to match Portage's `dep_getslot`, which scans for the slot
/// separator after removing any repository qualifier.
pub fn dep_getslot(mydep: &str) -> Option<String> {
    let without_repo = mydep.split("::").next().unwrap_or(mydep);
    let colon = without_repo.find(':')?;
    let after = &without_repo[colon + 1..];
    match after.find('[') {
        Some(bracket) => Some(after[..bracket].to_string()),
        None => Some(after.to_string()),
    }
}

/// Port of `portage.dep.dep_getrepo`: returns the repository qualifier that
/// follows `::`, or `None`.
pub fn dep_getrepo(mydep: &str) -> Option<String> {
    let colon = mydep.find("::")?;
    let after = &mydep[colon + 2..];
    match after.find('[') {
        Some(bracket) => Some(after[..bracket].to_string()),
        None => Some(after.to_string()),
    }
}

/// Port of `portage.dep.dep_getusedeps`: returns the USE-dependency flags in
/// the single allowed `[...]` group.
pub fn dep_getusedeps(depend: &str) -> Result<Vec<String>, DepError> {
    let mut use_list = Vec::new();
    let mut comma_separated = false;
    let mut bracket_count = 0u32;
    let mut search_from = 0usize;

    while let Some(rel) = depend[search_from..].find('[') {
        let open = search_from + rel;
        bracket_count += 1;
        if bracket_count > 1 {
            return Err(DepError(format!(
                "USE Dependency with more than one set of brackets: {depend}"
            )));
        }
        let close = depend[open..]
            .find(']')
            .map(|rel| open + rel)
            .ok_or_else(|| DepError(format!("USE Dependency with no closing bracket: {depend}")))?;
        let use_str = &depend[open + 1..close];
        if use_str.is_empty() {
            return Err(DepError(format!(
                "USE Dependency with no use flag ([]): {depend}"
            )));
        }
        if !comma_separated {
            comma_separated = use_str.contains(',');
        }
        if comma_separated {
            for flag in use_str.split(',') {
                if flag.is_empty() {
                    return Err(DepError(format!(
                        "USE Dependency with no use flag next to comma: {depend}"
                    )));
                }
                use_list.push(flag.to_string());
            }
        } else {
            use_list.push(use_str.to_string());
        }
        search_from = open + 1;
    }

    Ok(use_list)
}

/// Port of `portage.dep.isjustname`: true when the package string carries no
/// version component.
pub fn isjustname(mypkg: &str) -> bool {
    if let Ok(atom) = Atom::parse_with_options(mypkg, AtomParseOptions::default()) {
        return atom.operator.is_none()
            && atom.version.is_none()
            && atom.slot.is_none()
            && atom.use_deps.is_none()
            && atom.repo.is_none()
            && atom.blocker.is_none();
    }
    !mypkg.split('-').rev().take(2).any(is_version)
}

// ----------------------------------------------------------------------------
// paren_reduce (portage.dep.paren_reduce)
// ----------------------------------------------------------------------------

/// Port of `portage.dep.paren_reduce`: parses a dependency string into nested
/// groups, dropping redundant brackets exactly as Portage does.
pub fn paren_reduce(mystr: &str) -> Result<Vec<Dep>, DepError> {
    let mysplit = mystr.split_whitespace();
    let mut stack: Vec<Vec<Dep>> = vec![Vec::new()];
    let mut level = 0usize;
    let mut need_bracket = false;

    for token in mysplit {
        if token == "(" {
            need_bracket = false;
            stack.push(Vec::new());
            level += 1;
        } else if token == ")" {
            if need_bracket {
                return Err(malformed(mystr));
            }
            if level == 0 {
                return Err(malformed(mystr));
            }
            level -= 1;
            let l = stack.pop().expect("stack always has the child level");
            let is_single =
                l.len() == 1 || (l.len() == 2 && (l[0].is_or() || l[0].ends_with_question()));

            if !l.is_empty() {
                let ends_any_below = level >= 1 && stack[level - 1].last().is_some_and(Dep::is_or);
                let ends_op_here = stack[level]
                    .last()
                    .is_some_and(|d| d.is_or() || d.ends_with_question());
                let ends_any_here = stack[level].last().is_some_and(Dep::is_or);
                let last_eq_l0_or_or = stack[level]
                    .last()
                    .is_some_and(|last| *last == l[0] || last.is_or());
                let l0_is_or_or_q = l[0].is_or() || l[0].ends_with_question();

                if !ends_any_below && !ends_op_here {
                    stack[level].extend(l);
                } else {
                    let optimize_pop = !stack[level].is_empty()
                        && ((l.len() == 1 && ends_any_here)
                            || (l.len() == 2 && l0_is_or_or_q && last_eq_l0_or_or));
                    if optimize_pop {
                        stack[level].pop();
                    }
                    pr_special_append(&mut stack[level], l, is_single);
                }
            } else if stack[level]
                .last()
                .is_some_and(|d| d.is_or() || d.ends_with_question())
            {
                stack[level].pop();
            }
        } else if token == "||" {
            if need_bracket {
                return Err(malformed(mystr));
            }
            need_bracket = true;
            stack[level].push(Dep::token(token));
        } else {
            if need_bracket {
                return Err(malformed(mystr));
            }
            if token.ends_with('?') {
                need_bracket = true;
            }
            stack[level].push(Dep::token(token));
        }
    }

    if level != 0 || need_bracket {
        return Err(malformed(mystr));
    }

    Ok(stack.pop().expect("base level always present"))
}

fn pr_special_append(target: &mut Vec<Dep>, l: Vec<Dep>, is_single: bool) {
    let last_is_q = target.last().is_some_and(Dep::ends_with_question);
    if !is_single || last_is_q {
        target.push(Dep::Group(l));
        return;
    }
    match single_inner_group(l) {
        Ok(inner) => target.extend(inner),
        Err(l) => target.extend(l),
    }
}

/// If `l` is exactly `[Group(inner)]`, returns `inner`; otherwise returns `l`
/// unchanged. Replaces Portage's `l = [[...]]` flattening check.
fn single_inner_group(l: Vec<Dep>) -> Result<Vec<Dep>, Vec<Dep>> {
    if l.len() == 1 && matches!(l[0], Dep::Group(_)) {
        let mut l = l;
        match l.pop() {
            Some(Dep::Group(inner)) => Ok(inner),
            Some(other) => Err(vec![other]),
            None => Err(Vec::new()),
        }
    } else {
        Err(l)
    }
}

/// Port of `portage.dep.paren_enclose`: renders a reduced structure back into
/// a dependency string (used by `use_reduce`'s subset preprocessing).
pub fn paren_enclose(mylist: &[Dep]) -> String {
    let mut parts = Vec::new();
    for node in mylist {
        match node {
            Dep::Group(inner) => parts.push(format!("( {} )", paren_enclose(inner))),
            Dep::Token(token) => parts.push(token.clone()),
        }
    }
    parts.join(" ")
}

// ----------------------------------------------------------------------------
// use_reduce (portage.dep.use_reduce) — core subset
// ----------------------------------------------------------------------------

/// Options for [`use_reduce`]. This is a faithful core subset of Portage's
/// `use_reduce`: it supports `uselist`, `masklist`, `matchall`, `matchnone`,
/// `excludeall`, `subset`, `is_valid_flag`, and EAPI empty-group handling.
///
/// The `opconvert`, `flat`, `is_src_uri`, and `token_class` modes are not yet
/// ported; constructing this struct keeps the supported surface explicit.
pub struct UseReduceOptions<'a> {
    pub uselist: &'a [&'a str],
    pub masklist: &'a [&'a str],
    pub matchall: bool,
    pub matchnone: bool,
    pub excludeall: &'a [&'a str],
    pub subset: Option<&'a [&'a str]>,
    pub is_valid_flag: Option<&'a dyn Fn(&str) -> bool>,
    pub empty_groups_always_true: bool,
}

/// Matches Portage's permissive `_get_eapi_attrs(None)` default: empty
/// any-of groups are not implicitly satisfied, and no flags are enabled.
impl Default for UseReduceOptions<'_> {
    fn default() -> Self {
        Self {
            uselist: &[],
            masklist: &[],
            matchall: false,
            matchnone: false,
            excludeall: &[],
            subset: None,
            is_valid_flag: None,
            empty_groups_always_true: false,
        }
    }
}

struct UseReduceCtx<'a> {
    uselist: HashSet<&'a str>,
    masklist: HashSet<&'a str>,
    excludeall: HashSet<&'a str>,
    subset: Option<HashSet<&'a str>>,
    matchall: bool,
    matchnone: bool,
    is_valid_flag: Option<&'a dyn Fn(&str) -> bool>,
    empty_groups_always_true: bool,
}

fn valid_use_flag(flag: &str) -> bool {
    // ^[A-Za-z0-9][A-Za-z0-9+_@-]*$
    let mut chars = flag.chars();
    match chars.next() {
        Some(first) if first.is_ascii_alphanumeric() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '_' | '@' | '-'))
}

impl UseReduceCtx<'_> {
    fn is_active(&self, conditional: &str) -> Result<bool, DepError> {
        let (flag, is_negated) = match conditional.strip_prefix('!') {
            Some(rest) => (rest.strip_suffix('?').unwrap_or(rest), true),
            None => (conditional.strip_suffix('?').unwrap_or(conditional), false),
        };

        match self.is_valid_flag {
            Some(check) => {
                if !check(flag) {
                    return Err(DepError(format!(
                        "USE flag '{flag}' referenced in conditional '{conditional}' is not in IUSE"
                    )));
                }
            }
            None => {
                if !valid_use_flag(flag) {
                    return Err(DepError(format!(
                        "invalid use flag '{flag}' in conditional '{conditional}'"
                    )));
                }
            }
        }

        if is_negated && self.excludeall.contains(flag) {
            return Ok(false);
        }
        if self.masklist.contains(flag) {
            return Ok(is_negated);
        }
        if self.matchall {
            return Ok(true);
        }
        if self.matchnone {
            return Ok(false);
        }
        Ok((self.uselist.contains(flag) && !is_negated)
            || (!self.uselist.contains(flag) && is_negated))
    }
}

/// Port of `portage.dep.use_reduce` (core subset): reduces a dependency
/// string by evaluating USE conditionals against the enabled flags.
pub fn use_reduce(depstr: &str, options: &UseReduceOptions<'_>) -> Result<Vec<Dep>, DepError> {
    if options.matchall && options.matchnone {
        return Err(DepError(
            "use_reduce: 'matchall' and 'matchnone' are mutually exclusive".to_string(),
        ));
    }

    let ctx = UseReduceCtx {
        uselist: options.uselist.iter().copied().collect(),
        masklist: options.masklist.iter().copied().collect(),
        excludeall: options.excludeall.iter().copied().collect(),
        subset: options
            .subset
            .map(|subset| subset.iter().copied().collect()),
        matchall: options.matchall,
        matchnone: options.matchnone,
        is_valid_flag: options.is_valid_flag,
        empty_groups_always_true: options.empty_groups_always_true,
    };

    let working = if ctx.subset.is_some() {
        let reduced = paren_reduce(depstr)?;
        let selected = select_subset(&ctx, &reduced, false, false)?;
        paren_enclose(&selected)
    } else {
        depstr.to_string()
    };

    reduce_tokens(&ctx, &working)
}

fn reduce_tokens(ctx: &UseReduceCtx<'_>, depstr: &str) -> Result<Vec<Dep>, DepError> {
    let mysplit: Vec<&str> = depstr.split_whitespace().collect();
    let mut stack: Vec<Vec<Dep>> = vec![Vec::new()];
    let mut level = 0usize;
    let mut need_bracket = false;

    for (pos, &token) in mysplit.iter().enumerate() {
        if token == "(" {
            if mysplit.get(pos + 1) == Some(&")") {
                return Err(DepError(format!(
                    "expected: dependency string, got: ')', token {}",
                    pos + 1
                )));
            }
            need_bracket = false;
            stack.push(Vec::new());
            level += 1;
        } else if token == ")" {
            if need_bracket {
                return Err(DepError(format!(
                    "expected: '(', got: ')', token {}",
                    pos + 1
                )));
            }
            if level == 0 {
                return Err(DepError(format!(
                    "no matching '(' for ')', token {}",
                    pos + 1
                )));
            }
            level -= 1;
            let mut l = stack.pop().expect("child level present");
            let mut is_single = l.len() == 1 || (l.len() == 2 && l[0].is_or());
            let mut ignore = false;

            let last_token = match stack[level].last() {
                Some(Dep::Token(token)) => Some(token.clone()),
                _ => None,
            };
            if let Some(last_token) = last_token {
                if last_token == "||" && l.is_empty() {
                    if !ctx.empty_groups_always_true {
                        l.push(Dep::token("__const__/empty-any-of"));
                        is_single = l.len() == 1;
                    }
                    stack[level].pop();
                } else if last_token.ends_with('?') {
                    if !ctx.is_active(&last_token)? {
                        ignore = true;
                    }
                    stack[level].pop();
                }
            }

            if !l.is_empty() && !ignore {
                let ends_below = level >= 1 && stack[level - 1].last().is_some_and(Dep::is_or);
                let ends_here = stack[level].last().is_some_and(Dep::is_or);
                let last_op_below = last_any_of_operator_level(&stack, level);

                if !ends_below && !ends_here {
                    stack[level].extend(l);
                } else if stack[level].is_empty() {
                    ur_special_append(&mut stack[level], l, is_single, ends_below, last_op_below);
                } else if is_single && ends_here {
                    stack[level].pop();
                    ur_special_append(&mut stack[level], l, is_single, ends_below, last_op_below);
                } else if ends_here && ends_below {
                    stack[level].pop();
                    stack[level].extend(l);
                } else {
                    ur_special_append(&mut stack[level], l, is_single, ends_below, last_op_below);
                }
            }
        } else if token == "||" {
            if need_bracket {
                return Err(DepError(format!(
                    "expected: '(', got: '||', token {}",
                    pos + 1
                )));
            }
            need_bracket = true;
            stack[level].push(Dep::token(token));
        } else if token == "->" {
            // SRC_URI arrow handling is intentionally out of scope for the
            // dependency-string reducer; reject it as a malformed token.
            return Err(DepError(format!(
                "SRC_URI arrow is invalid in dependency strings: token {}",
                pos + 1
            )));
        } else {
            if need_bracket {
                return Err(DepError(format!(
                    "expected: '(', got: '{token}', token {}",
                    pos + 1
                )));
            }
            if token.ends_with('?') {
                need_bracket = true;
            }
            stack[level].push(Dep::token(token));
        }
    }

    if level != 0 {
        return Err(DepError("Missing ')' at end of string".to_string()));
    }
    if need_bracket {
        return Err(DepError("Missing '(' at end of string".to_string()));
    }

    Ok(stack.pop().expect("base level present"))
}

fn last_any_of_operator_level(stack: &[Vec<Dep>], level: usize) -> bool {
    // Returns true when `last_any_of_operator_level(level-1) != -1` in Portage.
    if level == 0 {
        return false;
    }
    let mut k = level as isize - 1;
    while k >= 0 {
        if let Some(Dep::Token(token)) = stack[k as usize].last() {
            if token == "||" {
                return true;
            }
            if !token.ends_with('?') {
                return false;
            }
        }
        k -= 1;
    }
    false
}

fn ur_special_append(
    target: &mut Vec<Dep>,
    l: Vec<Dep>,
    is_single: bool,
    ends_below: bool,
    last_op_below: bool,
) {
    if is_single {
        if l[0].is_or() && ends_below {
            // stack[level].extend(l[1])
            if let Some(Dep::Group(inner)) = l.into_iter().nth(1) {
                target.extend(inner);
            }
        } else if l.len() == 1 && matches!(l[0], Dep::Group(_)) {
            match single_inner_group(l) {
                Ok(inner) if last_op_below => target.push(Dep::Group(inner)),
                Ok(inner) => target.extend(inner),
                Err(l) => target.extend(l),
            }
        } else {
            target.extend(l);
        }
    } else {
        target.push(Dep::Group(l));
    }
}

fn select_subset(
    ctx: &UseReduceCtx<'_>,
    dep_struct: &[Dep],
    disjunction: bool,
    selected: bool,
) -> Result<Vec<Dep>, DepError> {
    let subset = ctx.subset.as_ref().expect("subset present when selecting");
    let mut result = Vec::new();
    let mut stack: Vec<&Dep> = dep_struct.iter().rev().collect();

    while let Some(token) = stack.pop() {
        match token {
            Dep::Group(inner) => {
                if disjunction {
                    let children = select_subset(ctx, inner, false, selected)?;
                    if !children.is_empty() {
                        result.push(Dep::Group(children));
                    }
                } else {
                    result.extend(select_subset(ctx, inner, false, selected)?);
                }
            }
            Dep::Token(t) if t.ends_with('?') => {
                let children = stack.pop().ok_or_else(|| malformed(t))?;
                if ctx.is_active(t)? {
                    let inner = group_children(children);
                    let flag = t.strip_suffix('?').unwrap_or(t);
                    let sel = selected || subset.contains(flag);
                    if disjunction {
                        let reduced = select_subset(ctx, &inner, false, sel)?;
                        if !reduced.is_empty() {
                            result.push(Dep::Group(reduced));
                        }
                    } else {
                        result.extend(select_subset(ctx, &inner, false, sel)?);
                    }
                }
            }
            Dep::Token(t) if t == "||" => {
                let next = stack.pop().ok_or_else(|| malformed(t))?;
                let inner = group_children(next);
                let children = select_subset(ctx, &inner, true, selected)?;
                if !children.is_empty() {
                    if disjunction {
                        result.extend(children);
                    } else {
                        result.push(Dep::token("||"));
                        result.push(Dep::Group(children));
                    }
                }
            }
            Dep::Token(t) => {
                if selected {
                    result.push(Dep::token(t));
                }
            }
        }
    }

    Ok(result)
}

fn group_children(node: &Dep) -> Vec<Dep> {
    match node {
        Dep::Group(inner) => inner.clone(),
        other => vec![other.clone()],
    }
}

// ----------------------------------------------------------------------------
// check_required_use (portage.dep.check_required_use) — boolean satisfaction
// ----------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
struct RequiredUseEapi {
    at_most_one_of: bool,
    empty_groups_always_true: bool,
}

/// Maps an EAPI string to the REQUIRED_USE feature gates this port needs.
fn required_use_eapi(eapi: Option<&str>) -> RequiredUseEapi {
    match eapi {
        // Permissive default matching Portage's `_get_eapi_attrs(None)`:
        // at-most-one-of is allowed, but empty groups are NOT always true.
        None => RequiredUseEapi {
            at_most_one_of: true,
            empty_groups_always_true: false,
        },
        Some("0" | "1" | "2" | "3" | "4") => RequiredUseEapi {
            at_most_one_of: false,
            empty_groups_always_true: true,
        },
        Some("5" | "6") => RequiredUseEapi {
            at_most_one_of: true,
            empty_groups_always_true: true,
        },
        _ => RequiredUseEapi {
            at_most_one_of: true,
            empty_groups_always_true: false,
        },
    }
}

#[derive(Clone)]
enum StackItem {
    Bool(bool),
    Op(String),
}

/// Port of `portage.dep.check_required_use` (boolean result): checks whether
/// `use_` satisfies `required_use`. `iuse_match` reports whether a flag is a
/// known IUSE flag. Returns an error for malformed strings or unknown flags,
/// matching Portage's `InvalidDependString` behavior.
pub fn check_required_use(
    required_use: &str,
    use_: &[&str],
    iuse_match: impl Fn(&str) -> bool,
    eapi: Option<&str>,
) -> Result<bool, DepError> {
    let attrs = required_use_eapi(eapi);
    let valid_operators: &[&str] = if attrs.at_most_one_of {
        &["||", "^^", "??"]
    } else {
        &["||", "^^"]
    };
    let use_set: HashSet<&str> = use_.iter().copied().collect();

    let is_active = |token: &str| -> Result<bool, DepError> {
        let (flag, is_negated) = match token.strip_prefix('!') {
            Some(rest) => (rest, true),
            None => (token, false),
        };
        if flag.is_empty() || !iuse_match(flag) {
            if !attrs.at_most_one_of && flag == "?" {
                return Err(DepError(format!(
                    "Operator '??' is invalid under EAPI '{}'",
                    eapi.unwrap_or("")
                )));
            }
            return Err(DepError(format!("USE flag '{flag}' is not in IUSE")));
        }
        Ok((use_set.contains(flag) && !is_negated) || (!use_set.contains(flag) && is_negated))
    };

    let is_satisfied = |operator: &str, argument: &[bool]| -> bool {
        if argument.is_empty() && attrs.empty_groups_always_true {
            return true;
        }
        match operator {
            "||" => argument.contains(&true),
            "^^" => argument.iter().filter(|&&v| v).count() == 1,
            "??" => argument.iter().filter(|&&v| v).count() <= 1,
            _ => !argument.contains(&false), // trailing '?'
        }
    };

    let mysplit = required_use.split_whitespace();
    let mut level = 0usize;
    let mut stack: Vec<Vec<StackItem>> = vec![Vec::new()];
    let mut need_bracket = false;

    for token in mysplit {
        if token == "(" {
            need_bracket = false;
            stack.push(Vec::new());
            level += 1;
        } else if token == ")" {
            if need_bracket {
                return Err(malformed(required_use));
            }
            if level == 0 {
                return Err(malformed(required_use));
            }
            level -= 1;
            let l = stack.pop().expect("child level present");
            let l_bools: Vec<bool> = l
                .iter()
                .map(|item| match item {
                    StackItem::Bool(b) => *b,
                    StackItem::Op(_) => false,
                })
                .collect();

            let mut op: Option<String> = None;
            if let Some(last) = stack[level].last().cloned() {
                match last {
                    StackItem::Op(token) if valid_operators.contains(&token.as_str()) => {
                        stack[level].pop();
                        let satisfied = is_satisfied(&token, &l_bools);
                        stack[level].push(StackItem::Bool(satisfied));
                        op = Some(token);
                    }
                    StackItem::Op(token) if token.ends_with('?') => {
                        stack[level].pop();
                        op = Some(token.clone());
                        if is_active(&token[..token.len() - 1])? {
                            let satisfied = is_satisfied(&token, &l_bools);
                            stack[level].push(StackItem::Bool(satisfied));
                        }
                        // Inactive conditional contributes nothing.
                    }
                    // Top of stack is a plain boolean (no operator governs this
                    // group): fall through to the `op.is_none()` handling below.
                    StackItem::Bool(_) | StackItem::Op(_) => {}
                }
            }

            if op.is_none() {
                let satisfied = !l_bools.contains(&false);
                if !l.is_empty() {
                    stack[level].push(StackItem::Bool(satisfied));
                }
            }
        } else if valid_operators.contains(&token) || token.ends_with('?') {
            if need_bracket {
                return Err(malformed(required_use));
            }
            need_bracket = true;
            stack[level].push(StackItem::Op(token.to_string()));
        } else {
            if need_bracket {
                return Err(malformed(required_use));
            }
            let satisfied = is_active(token)?;
            stack[level].push(StackItem::Bool(satisfied));
        }
    }

    if level != 0 || need_bracket {
        return Err(malformed(required_use));
    }

    let top = stack.pop().expect("base level present");
    Ok(!top
        .iter()
        .any(|item| matches!(item, StackItem::Bool(false))))
}
