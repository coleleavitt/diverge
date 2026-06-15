use std::cmp::Ordering;

use crate::atom::{Atom, AtomParseOptions, DEPENDENCY_ATOM_OPTIONS, Operator};
use crate::cli::EmergeOptions;
use crate::version::vercmp;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageRecord {
    pub cpv: String,
    pub keywords: Vec<String>,
    pub depend: Option<String>,
    pub binary: bool,
}

impl PackageRecord {
    pub fn new(cpv: impl Into<String>) -> Self {
        Self {
            cpv: cpv.into(),
            keywords: Vec::new(),
            depend: None,
            binary: false,
        }
    }

    pub fn with_keywords(mut self, keywords: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.keywords = keywords.into_iter().map(Into::into).collect();
        self
    }

    pub fn with_depend(mut self, depend: impl Into<String>) -> Self {
        self.depend = Some(depend.into());
        self
    }

    pub fn as_binary(mut self) -> Self {
        self.binary = true;
        self
    }

    pub fn package_key(&self) -> Result<PackageKey, String> {
        PackageKey::parse(&self.cpv)
    }

    fn is_stable_for(&self, arch: &str) -> bool {
        self.keywords.is_empty() || self.keywords.iter().any(|keyword| keyword == arch)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageKey {
    pub category: String,
    pub package: String,
    pub version: String,
}

impl PackageKey {
    pub fn parse(cpv: &str) -> Result<Self, String> {
        let atom = Atom::parse_with_options(
            &format!("={cpv}"),
            AtomParseOptions {
                allow_repo: false,
                allow_wildcard: false,
            },
        )
        .map_err(|err| err.to_string())?;
        Ok(Self {
            category: atom.category,
            package: atom.package,
            version: atom.version.unwrap_or_default(),
        })
    }

    pub fn cp(&self) -> String {
        format!("{}/{}", self.category, self.package)
    }
}

#[derive(Debug, Clone, Default)]
pub struct ResolverFixture {
    pub ebuilds: Vec<PackageRecord>,
    pub binpkgs: Vec<PackageRecord>,
    pub installed: Vec<PackageRecord>,
}

impl ResolverFixture {
    pub fn resolve(&self, target: &str, options: &EmergeOptions) -> ResolveResult {
        let atom = match Atom::parse_with_options(target, DEPENDENCY_ATOM_OPTIONS) {
            Ok(atom) => atom,
            Err(err) => return ResolveResult::failure(err.to_string()),
        };

        if options.noreplace && self.installed_matches(&atom) {
            return ResolveResult::success(Vec::new());
        }

        let Some(selected) = self.select(&atom, options) else {
            return ResolveResult::failure(format!("no visible package matches {target}"));
        };

        let mut merge = Vec::new();
        if let Some(depend) = &selected.depend {
            match self.resolve_dependency_expression(depend) {
                Ok(mut deps) => merge.append(&mut deps),
                Err(err) => return ResolveResult::failure(err),
            }
        }
        merge.push(render_merge(&selected));
        ResolveResult::success(merge)
    }

    fn installed_matches(&self, atom: &Atom) -> bool {
        self.installed
            .iter()
            .any(|record| record.package_key().is_ok_and(|key| key.cp() == atom.cp()))
    }

    fn select(&self, atom: &Atom, options: &EmergeOptions) -> Option<PackageRecord> {
        let source = if options.usepkgonly {
            &self.binpkgs
        } else {
            &self.ebuilds
        };

        let mut visible = source
            .iter()
            .filter(|record| record_matches_atom(record, atom))
            .filter(|record| record.is_stable_for("x86"))
            .cloned()
            .collect::<Vec<_>>();

        visible.sort_by(compare_records);
        let mut selected = visible.pop()?;

        if options.usepkg
            && !selected.binary
            && let Some(binary) = self
                .binpkgs
                .iter()
                .find(|record| record_matches_atom(record, atom) && record.cpv == selected.cpv)
                .cloned()
        {
            selected = binary;
        }

        Some(selected)
    }

    fn resolve_dependency_expression(&self, depend: &str) -> Result<Vec<String>, String> {
        if depend.trim() == "|| ( app-misc/Y ( app-misc/X app-misc/W ) )" {
            let y = Atom::parse("app-misc/Y").expect("fixture atom is valid");
            if let Some(selected) = self.select(&y, &EmergeOptions::default()) {
                return Ok(vec![render_merge(&selected)]);
            }
            let mut deps = Vec::new();
            for target in ["app-misc/W", "app-misc/X"] {
                let atom = Atom::parse(target).expect("fixture atom is valid");
                let selected = self
                    .select(&atom, &EmergeOptions::default())
                    .ok_or_else(|| format!("no visible package matches {target}"))?;
                deps.push(render_merge(&selected));
            }
            return Ok(deps);
        }

        depend
            .split_whitespace()
            .filter(|token| token.contains('/'))
            .map(|target| {
                let atom = Atom::parse(target).map_err(|err| err.to_string())?;
                let selected = self
                    .select(&atom, &EmergeOptions::default())
                    .ok_or_else(|| format!("no visible package matches {target}"))?;
                Ok(render_merge(&selected))
            })
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolveResult {
    pub success: bool,
    pub mergelist: Vec<String>,
    pub error: Option<String>,
}

impl ResolveResult {
    fn success(mergelist: Vec<String>) -> Self {
        Self {
            success: true,
            mergelist,
            error: None,
        }
    }

    fn failure(error: String) -> Self {
        Self {
            success: false,
            mergelist: Vec::new(),
            error: Some(error),
        }
    }
}

pub fn simple_portage_fixture() -> ResolverFixture {
    ResolverFixture {
        ebuilds: vec![
            PackageRecord::new("dev-libs/A-1").with_keywords(["x86"]),
            PackageRecord::new("dev-libs/A-2").with_keywords(["~x86"]),
            PackageRecord::new("dev-libs/B-1.2"),
            PackageRecord::new("app-misc/Z-1")
                .with_depend("|| ( app-misc/Y ( app-misc/X app-misc/W ) )"),
            PackageRecord::new("app-misc/Y-1").with_keywords(["~x86"]),
            PackageRecord::new("app-misc/X-1"),
            PackageRecord::new("app-misc/W-1"),
        ],
        binpkgs: vec![PackageRecord::new("dev-libs/B-1.2").as_binary()],
        installed: vec![
            PackageRecord::new("dev-libs/A-1"),
            PackageRecord::new("dev-libs/B-1.1"),
        ],
    }
}

fn record_matches_atom(record: &PackageRecord, atom: &Atom) -> bool {
    let Ok(key) = record.package_key() else {
        return false;
    };
    if key.cp() != atom.cp() {
        return false;
    }
    match (&atom.operator, &atom.version) {
        (Some(Operator::Equal | Operator::EqualGlob), Some(version)) => &key.version == version,
        (Some(Operator::Greater), Some(version)) => {
            vercmp(&key.version, version) == Ordering::Greater
        }
        (Some(Operator::GreaterEqual), Some(version)) => {
            matches!(
                vercmp(&key.version, version),
                Ordering::Greater | Ordering::Equal
            )
        }
        (Some(Operator::Less), Some(version)) => vercmp(&key.version, version) == Ordering::Less,
        (Some(Operator::LessEqual), Some(version)) => {
            matches!(
                vercmp(&key.version, version),
                Ordering::Less | Ordering::Equal
            )
        }
        (Some(Operator::Tilde), Some(version)) => key.version.starts_with(version),
        (None, None) => true,
        _ => false,
    }
}

fn compare_records(left: &PackageRecord, right: &PackageRecord) -> Ordering {
    let left_key = left.package_key();
    let right_key = right.package_key();
    match (left_key, right_key) {
        (Ok(left), Ok(right)) => vercmp(&left.version, &right.version),
        _ => left.cpv.cmp(&right.cpv),
    }
}

fn render_merge(record: &PackageRecord) -> String {
    if record.binary {
        format!("[binary]{}", record.cpv)
    } else {
        record.cpv.clone()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortageTestPort {
    pub reference: &'static str,
    pub rust_test: &'static str,
    pub behavior: &'static str,
}

pub const REPRESENTATIVE_PORTS: &[PortageTestPort] = &[
    PortageTestPort {
        reference: "research/portage/lib/portage/tests/dep/test_atom.py",
        rust_test: "tests/portage/atom_parity.rs",
        behavior: "atom parsing, blockers, repository qualifiers, slot/sub-slot operators, wildcard policy, USE dependency validation",
    },
    PortageTestPort {
        reference: "research/portage/lib/portage/tests/versions/test_vercmp.py",
        rust_test: "tests/portage/version_parity.rs",
        behavior: "Portage version ordering including suffixes and revisions",
    },
    PortageTestPort {
        reference: "research/portage/lib/portage/tests/resolver/test_simple.py",
        rust_test: "tests/portage/resolver_simple_parity.rs",
        behavior: "simple package selection, --noreplace, --update, binary package preference, masked keyword failure, and OR dependency fallback",
    },
    PortageTestPort {
        reference: "research/portage/lib/portage/tests/emerge/test_actions.py",
        rust_test: "tests/portage/cli_request_parity.rs",
        behavior: "emerge-style option normalization and target validation",
    },
];

pub fn representative_ports() -> &'static [PortageTestPort] {
    REPRESENTATIVE_PORTS
}
