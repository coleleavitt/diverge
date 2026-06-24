//! Package sets, ported from Portage's `portage._sets`.
//!
//! A package set is a named collection of atoms emerge expands when it sees an
//! `@name` target. The built-in sets are `@selected` (the world file),
//! `@system` (the profile's system packages), `@world` (selected + system +
//! nested sets), and static/config-file sets. This module models set storage
//! and expansion with nested-set support and cycle protection.
//!
//! Reference:
//! - `research/portage/lib/portage/_sets/__init__.py`
//! - `research/portage/lib/portage/_sets/base.py` (PackageSet)
//! - `research/portage/lib/portage/tests/sets/**`

use std::collections::{BTreeMap, BTreeSet};

/// One member of a package set: either a literal atom or a reference to another
/// set (`@name`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SetMember {
    Atom(String),
    SetRef(String),
}

impl SetMember {
    /// Parses a set-file token: `@name` is a set reference, anything else is an
    /// atom literal.
    pub fn parse(token: &str) -> Self {
        match token.strip_prefix('@') {
            Some(name) => Self::SetRef(name.to_string()),
            None => Self::Atom(token.to_string()),
        }
    }
}

/// A registry of named package sets that can expand `@name` references,
/// resolving nested sets with cycle protection.
#[derive(Debug, Clone, Default)]
pub struct SetRegistry {
    sets: BTreeMap<String, Vec<SetMember>>,
}

impl SetRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Defines a set from its raw members (atoms and/or `@refs`).
    pub fn define(&mut self, name: impl Into<String>, members: Vec<SetMember>) {
        self.sets.insert(name.into(), members);
    }

    /// Defines a set from whitespace/newline-separated text (a set file body).
    pub fn define_from_text(&mut self, name: impl Into<String>, text: &str) {
        let members = text
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .map(SetMember::parse)
            .collect();
        self.define(name, members);
    }

    /// Registers the standard `@world` set as `@selected` + `@system` + nested.
    /// Mirrors upstream's WorldSet composition.
    pub fn define_world(&mut self) {
        self.define(
            "world",
            vec![
                SetMember::SetRef("selected".to_string()),
                SetMember::SetRef("system".to_string()),
            ],
        );
    }

    /// True when a set with this name is defined.
    pub fn contains(&self, name: &str) -> bool {
        self.sets.contains_key(name)
    }

    /// Expands a set name into its full atom list, resolving nested set
    /// references depth-first. Duplicate atoms are removed (first-seen order is
    /// preserved). An undefined set reference is skipped. Cycles are broken.
    pub fn expand(&self, name: &str) -> Vec<String> {
        let mut seen_sets = BTreeSet::new();
        let mut atoms = Vec::new();
        let mut atom_set = BTreeSet::new();
        self.expand_into(name, &mut seen_sets, &mut atoms, &mut atom_set);
        atoms
    }

    fn expand_into(
        &self,
        name: &str,
        seen_sets: &mut BTreeSet<String>,
        atoms: &mut Vec<String>,
        atom_set: &mut BTreeSet<String>,
    ) {
        if !seen_sets.insert(name.to_string()) {
            return; // Cycle or already-expanded set.
        }
        let Some(members) = self.sets.get(name) else {
            return;
        };
        for member in members {
            match member {
                SetMember::Atom(atom) => {
                    if atom_set.insert(atom.clone()) {
                        atoms.push(atom.clone());
                    }
                }
                SetMember::SetRef(set_name) => {
                    self.expand_into(set_name, seen_sets, atoms, atom_set);
                }
            }
        }
    }
}

/// The world file (`@selected`): the user-requested package atoms, one per
/// line. Ported from `WorldSelectedPackagesSet`.
#[derive(Debug, Clone, Default)]
pub struct WorldFile {
    atoms: Vec<String>,
}

impl WorldFile {
    /// Parses a world file's contents (one atom per line, `#` comments).
    pub fn parse(content: &str) -> Self {
        let atoms = content
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .map(str::to_string)
            .collect();
        Self { atoms }
    }

    /// The atoms currently in the world file.
    pub fn atoms(&self) -> &[String] {
        &self.atoms
    }

    /// Adds an atom if not already present (mirrors `world` update on install).
    /// Returns true if it was newly added.
    pub fn add(&mut self, atom: impl Into<String>) -> bool {
        let atom = atom.into();
        if self.atoms.contains(&atom) {
            false
        } else {
            self.atoms.push(atom);
            true
        }
    }

    /// Removes an atom (mirrors world update on unmerge). Returns true if it
    /// was present.
    pub fn remove(&mut self, atom: &str) -> bool {
        let before = self.atoms.len();
        self.atoms.retain(|a| a != atom);
        self.atoms.len() != before
    }

    /// Renders the world file back to text (sorted, one atom per line), the
    /// canonical on-disk form emerge writes.
    pub fn render(&self) -> String {
        let mut sorted = self.atoms.clone();
        sorted.sort();
        let mut out = sorted.join("\n");
        if !out.is_empty() {
            out.push('\n');
        }
        out
    }
}
