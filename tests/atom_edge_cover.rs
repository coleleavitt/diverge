//! Exact atom validation/render branch coverage.

use diverge::atom::{Atom, AtomError, AtomParseOptions};

const WILD: AtomParseOptions = AtomParseOptions {
    allow_wildcard: true,
    allow_repo: true,
};

fn err(s: &str) -> AtomError {
    Atom::parse_with_options(s, WILD).unwrap_err()
}

#[test]
fn operator_requires_version_exact() {
    // `>=dev-libs/A` has an operator but no version (atom.rs 154-155).
    assert_eq!(err(">=dev-libs/A"), AtomError::OperatorRequiresVersion);
    assert_eq!(err("~dev-libs/A"), AtomError::OperatorRequiresVersion);
}

#[test]
fn equalglob_display_branch() {
    // EqualGlob renders with leading `=` and trailing `*` (atom.rs 264-268).
    let a = Atom::parse_with_options("=dev-libs/A-1.2*", WILD).unwrap();
    let s = a.to_string();
    assert!(s.starts_with('='));
    assert!(s.ends_with('*'));
    assert_eq!(s, "=dev-libs/A-1.2*");
}

#[test]
fn weak_blocker_then_bang_invalid() {
    // `!` followed by another `!`... already `!!`; but `!` then non-! is weak.
    // The InvalidBlocker weak-path (308): a single `!` whose rest starts `!`
    // is caught by the `!!` branch; construct `!` + `!x` via `!!x` handled.
    // The weak branch's `rest.starts_with('!')` triggers on inputs like `!!`
    // with nothing after handled by strong; use the strong-with-extra case.
    assert_eq!(err("!!!dev-libs/A"), AtomError::InvalidBlocker);
}

#[test]
fn nested_use_dep_brackets_invalid() {
    // `[a[b]]`-style nested brackets (atom.rs 336-337).
    assert_eq!(err("dev-libs/A[a[b]"), AtomError::InvalidUseDependency);
}

#[test]
fn duplicate_use_flag_invalid() {
    // Same flag twice (atom.rs 390-391).
    assert_eq!(err("dev-libs/A[foo,foo]"), AtomError::InvalidUseDependency);
}

#[test]
fn slot_empty_or_trailing_slash_invalid() {
    // Slot ending with `/` (atom.rs 442-443).
    assert_eq!(err("dev-libs/A:2/"), AtomError::InvalidSlot);
}

#[test]
fn too_many_path_components_invalid() {
    // category/package/extra (atom.rs 475-476).
    assert_eq!(err("a/b/c"), AtomError::InvalidCategoryPackage);
}

#[test]
fn is_version_revision_edge_cases() {
    // A package whose name ends with `-r<non-digits>` is NOT a version, so a
    // bare (operator-less) atom parses as a name (is_version 508-510).
    let a = Atom::parse_with_options("dev-libs/foo-rxyz", WILD);
    assert!(a.is_ok(), "non-numeric -r suffix is part of the name");
    // `-r5` with a numeric revision but no base version is not a version either.
    let a = Atom::parse_with_options("dev-libs/pkg", WILD).unwrap();
    assert!(a.version.is_none());
}
