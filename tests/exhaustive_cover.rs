//! Exhaustive coverage of error/Display arms and parser edge branches across
//! the crate, driving each toward 100% line coverage.

use diverge::atom::{Atom, AtomError, AtomParseOptions};
use diverge::config::ParseError;
use diverge::dep::DepError;
use diverge::gpkg::GpkgError;
use diverge::manifest::ManifestError;
use diverge::update::UpdateParseError;
use diverge::version::{cpv_cmp, sort_cpvs, split_cpv, vercmp};
use diverge::xpak::XpakError;

const WILD: AtomParseOptions = AtomParseOptions {
    allow_wildcard: true,
    allow_repo: true,
};
const NOREPO: AtomParseOptions = AtomParseOptions {
    allow_wildcard: false,
    allow_repo: false,
};

#[test]
fn atom_error_display_every_variant() {
    for v in [
        AtomError::Empty,
        AtomError::NonAscii,
        AtomError::InvalidBlocker,
        AtomError::InvalidRepository,
        AtomError::RepositoryNotAllowed,
        AtomError::InvalidUseDependency,
        AtomError::InvalidSlot,
        AtomError::InvalidCategoryPackage,
        AtomError::WildcardNotAllowed,
        AtomError::InvalidVersion,
        AtomError::VersionWithoutOperator,
        AtomError::OperatorRequiresVersion,
    ] {
        assert!(!format!("{v}").is_empty());
        // Also exercise the std::error::Error impl.
        let _: &dyn std::error::Error = &v;
    }
}

#[test]
fn atom_parse_triggers_each_error() {
    assert_eq!(Atom::parse_with_options("", WILD), Err(AtomError::Empty));
    assert_eq!(
        Atom::parse_with_options("dev-libs/café", WILD),
        Err(AtomError::NonAscii)
    );
    assert!(matches!(
        Atom::parse_with_options("dev-libs/A::r", NOREPO),
        Err(AtomError::RepositoryNotAllowed)
    ));
    assert!(matches!(
        Atom::parse_with_options("*/*", NOREPO),
        Err(AtomError::WildcardNotAllowed)
    ));
    assert!(matches!(
        Atom::parse_with_options("dev-libs/A-1", WILD),
        Err(AtomError::VersionWithoutOperator)
    ));
    assert!(matches!(
        Atom::parse_with_options("=dev-libs/A", WILD),
        Err(AtomError::OperatorRequiresVersion)
    ));
    assert!(matches!(
        Atom::parse_with_options("noslash", WILD),
        Err(AtomError::InvalidCategoryPackage)
    ));
    // Malformed USE dep group.
    assert!(Atom::parse_with_options("dev-libs/A[", WILD).is_err());
}

#[test]
fn atom_accessors_and_intersects() {
    let a = Atom::parse_with_options("=dev-libs/A-1:2/3::r[foo,-bar]", WILD).unwrap();
    assert_eq!(a.cpv(), "dev-libs/A-1");
    assert_eq!(a.cp(), "dev-libs/A");
    assert_eq!(a.slot(), Some("2"));
    assert_eq!(a.sub_slot(), Some("3"));
    assert_eq!(a.repo.as_deref(), Some("r"));
    // intersects: same cp overlapping.
    let b = Atom::parse_with_options("dev-libs/A", WILD).unwrap();
    assert!(a.intersects(&b));
    let c = Atom::parse_with_options("dev-libs/B", WILD).unwrap();
    assert!(!a.intersects(&c));
    // parsed_use_deps present.
    assert!(a.parsed_use_deps().is_some());
    assert!(b.parsed_use_deps().is_none());
}

#[test]
fn dep_error_display() {
    let e = DepError("boom".to_string());
    assert_eq!(format!("{e}"), "boom");
    let _: &dyn std::error::Error = &e;
}

#[test]
fn config_parse_error_display() {
    let e = ParseError("bad".to_string());
    assert!(format!("{e}").contains("bad"));
    let _: &dyn std::error::Error = &e;
}

#[test]
fn update_parse_error_display() {
    let e = UpdateParseError("nope".to_string());
    assert_eq!(format!("{e}"), "nope");
    let _: &dyn std::error::Error = &e;
}

#[test]
fn xpak_error_display() {
    assert_eq!(format!("{}", XpakError::BadMagic), "invalid XPAK magic");
    assert_eq!(
        format!("{}", XpakError::Truncated),
        "truncated XPAK segment"
    );
}

#[test]
fn gpkg_error_display_all() {
    assert!(format!("{}", GpkgError::Malformed("m".into())).contains("malformed"));
    assert!(format!("{}", GpkgError::ChecksumMismatch("c".into())).contains("checksum"));
    // From<XpakError> conversion + Display.
    let g: GpkgError = XpakError::BadMagic.into();
    assert!(format!("{g}").contains("metadata"));
}

#[test]
fn manifest_error_display_all() {
    for e in [
        ManifestError::MalformedLine("l".into()),
        ManifestError::UnpairedHash("l".into()),
        ManifestError::UnknownFile("f".into()),
        ManifestError::SizeMismatch {
            name: "f".into(),
            expected: 1,
            actual: 2,
        },
        ManifestError::DigestMismatch {
            name: "f".into(),
            algo: "SHA512".into(),
        },
        ManifestError::NoUsableHash("f".into()),
    ] {
        assert!(!format!("{e}").is_empty());
    }
}

#[test]
fn version_compare_and_sort_branches() {
    // vercmp covering base/suffix/revision compare paths (partial_cmp via Ord).
    assert_eq!(vercmp("1.0", "1.0"), std::cmp::Ordering::Equal);
    assert_eq!(vercmp("1.0", "1.1"), std::cmp::Ordering::Less);
    assert_eq!(vercmp("1.1", "1.0"), std::cmp::Ordering::Greater);
    assert_eq!(vercmp("1.0-r1", "1.0-r2"), std::cmp::Ordering::Less);
    assert_eq!(vercmp("1.0_alpha", "1.0_beta"), std::cmp::Ordering::Less);
    assert_eq!(vercmp("1.0_p1", "1.0"), std::cmp::Ordering::Greater);
    // split_cpv with and without version.
    assert_eq!(
        split_cpv("cat/pkg-1.2"),
        ("cat/pkg".to_string(), Some("1.2".to_string()))
    );
    assert_eq!(split_cpv("cat/pkg"), ("cat/pkg".to_string(), None));
    // cpv_cmp + sort.
    assert_eq!(cpv_cmp("cat/a-1", "cat/a-2"), std::cmp::Ordering::Less);
    let mut v = vec![
        "cat/a-2".to_string(),
        "cat/a-1".to_string(),
        "cat/b-1".to_string(),
    ];
    sort_cpvs(&mut v);
    assert_eq!(v, vec!["cat/a-1", "cat/a-2", "cat/b-1"]);
}
