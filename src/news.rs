//! GLEP 42 news items, ported from `portage.news`.
//!
//! A news item has a header (`Title`, `Author`, `Posted`, `Revision`,
//! `News-Item-Format`) and optional `Display-If-Installed`,
//! `Display-If-Keyword`, and `Display-If-Profile` restrictions. An item is
//! relevant when, for each restriction *type present*, at least one value
//! matches (OR within a type), across all types (AND between types). An item
//! with no restrictions is always relevant.
//!
//! Reference:
//! - `research/portage/lib/portage/news.py` (`NewsItem.isRelevant`, `parse`)
//! - `research/portage/lib/portage/tests/news/test_NewsItem.py`
//! - GLEP 42: https://www.gentoo.org/glep/glep-0042.html

use std::collections::BTreeSet;

use crate::atom::{Atom, AtomParseOptions};

const NEWS_ATOM_OPTIONS: AtomParseOptions = AtomParseOptions {
    allow_wildcard: true,
    allow_repo: true,
};

/// A parsed news item and its display restrictions.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NewsItem {
    pub title: String,
    pub author: String,
    pub posted: String,
    pub revision: u32,
    pub format: String,
    /// `Display-If-Installed` atoms (relevant if any is installed).
    pub display_if_installed: Vec<String>,
    /// `Display-If-Keyword` keywords (relevant if any is the system keyword).
    pub display_if_keyword: Vec<String>,
    /// `Display-If-Profile` profile paths (relevant if any is the active one).
    pub display_if_profile: Vec<String>,
}

/// The environment a news item's relevance is evaluated against.
#[derive(Debug, Clone, Default)]
pub struct NewsEnvironment {
    /// cpvs currently installed (the vardb view).
    pub installed: Vec<String>,
    /// The system's `ARCH` keyword (e.g. `amd64`).
    pub keyword: String,
    /// The active profile path (matched against `Display-If-Profile`).
    pub profile: Option<String>,
}

impl NewsItem {
    /// Parses a news item from its text header. Unknown lines are ignored; the
    /// body (after the blank line) is not retained.
    pub fn parse(text: &str) -> Self {
        let mut item = NewsItem::default();
        for line in text.lines() {
            let Some((key, value)) = line.split_once(':') else {
                continue;
            };
            let value = value.trim();
            match key.trim() {
                "Title" => item.title = value.to_string(),
                "Author" => item.author = value.to_string(),
                "Posted" => item.posted = value.to_string(),
                "Revision" => item.revision = value.parse().unwrap_or(1),
                "News-Item-Format" => item.format = value.to_string(),
                "Display-If-Installed" => item.display_if_installed.push(value.to_string()),
                "Display-If-Keyword" => item.display_if_keyword.push(value.to_string()),
                "Display-If-Profile" => item.display_if_profile.push(value.to_string()),
                // Other headers (Content-Type, etc.) and body lines are not
                // modeled here; upstream likewise only matches known headers.
                _ => {}
            }
        }
        item
    }

    /// True when the item has at least a title (the minimal validity check
    /// this port models; upstream also validates format/posted/author).
    pub fn is_valid(&self) -> bool {
        !self.title.is_empty()
    }

    /// Whether this item is relevant in `env`. Restrictions of the same type
    /// are OR'd; different types are AND'd. No restrictions => always relevant.
    pub fn is_relevant(&self, env: &NewsEnvironment) -> bool {
        if !self.display_if_installed.is_empty() && !self.installed_matches(env) {
            return false;
        }
        if !self.display_if_keyword.is_empty() && !self.display_if_keyword.contains(&env.keyword) {
            return false;
        }
        if !self.display_if_profile.is_empty()
            && !self
                .display_if_profile
                .iter()
                .any(|p| env.profile.as_deref() == Some(p.as_str()))
        {
            return false;
        }
        true
    }

    fn installed_matches(&self, env: &NewsEnvironment) -> bool {
        self.display_if_installed.iter().any(|atom_str| {
            Atom::parse_with_options(atom_str, NEWS_ATOM_OPTIONS)
                .map(|atom| {
                    env.installed
                        .iter()
                        .any(|cpv| installed_cpv_matches(cpv, &atom))
                })
                .unwrap_or(false)
        })
    }
}

/// True when an installed `cpv` matches a `Display-If-Installed` atom (cp plus
/// version operator if any).
fn installed_cpv_matches(cpv: &str, atom: &Atom) -> bool {
    use crate::matching::{Candidate, match_from_list};
    let pool = [Candidate::new(cpv)];
    !match_from_list(atom, &pool).is_empty()
}

/// Tracks which news items a user has already read, by item name.
#[derive(Debug, Clone, Default)]
pub struct ReadTracker {
    read: BTreeSet<String>,
}

impl ReadTracker {
    /// Parses a `news.read`-style file (one item name per line).
    pub fn parse(content: &str) -> Self {
        Self {
            read: content
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(str::to_string)
                .collect(),
        }
    }

    pub fn is_read(&self, name: &str) -> bool {
        self.read.contains(name)
    }

    /// Marks an item read; returns true if it was newly marked.
    pub fn mark_read(&mut self, name: impl Into<String>) -> bool {
        self.read.insert(name.into())
    }

    /// Of `names`, those not yet read, preserving order.
    pub fn unread<'a>(&self, names: &'a [String]) -> Vec<&'a String> {
        names.iter().filter(|n| !self.read.contains(*n)).collect()
    }

    /// Renders the read set back to file form (sorted, newline-terminated).
    pub fn render(&self) -> String {
        let mut out: String = self.read.iter().cloned().collect::<Vec<_>>().join("\n");
        if !out.is_empty() {
            out.push('\n');
        }
        out
    }
}
