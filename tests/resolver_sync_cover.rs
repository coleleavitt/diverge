//! Coverage for the fixture resolver's operator/OR-choice/binary branches and
//! the sync error Display.

use diverge::cli::EmergeOptions;
use diverge::resolver::{PackageRecord, ResolverFixture, simple_portage_fixture};
use diverge::sync::SyncError;

fn opts() -> EmergeOptions {
    EmergeOptions::default()
}

#[test]
fn fixture_or_choice_fallback() {
    // app-misc/Z has DEPEND "|| ( app-misc/Y ( app-misc/X app-misc/W ) )".
    // Y is ~x86 (not stable) so the resolver falls back to (X W).
    let fixture = simple_portage_fixture();
    let result = fixture.resolve("app-misc/Z", &opts());
    assert!(result.success, "{:?}", result.error);
    // The fallback branch pulls in W and X before Z.
    assert!(result.mergelist.iter().any(|m| m.contains("app-misc/W")));
    assert!(result.mergelist.iter().any(|m| m.contains("app-misc/X")));
    assert!(result.mergelist.iter().any(|m| m.contains("app-misc/Z")));
}

#[test]
fn fixture_operator_matching() {
    let fixture = ResolverFixture {
        ebuilds: vec![
            PackageRecord::new("dev-libs/A-1").with_keywords(["x86"]),
            PackageRecord::new("dev-libs/A-2").with_keywords(["x86"]),
            PackageRecord::new("dev-libs/A-3").with_keywords(["x86"]),
        ],
        binpkgs: vec![],
        installed: vec![],
    };
    // >= picks the highest matching.
    let r = fixture.resolve(">=dev-libs/A-2", &opts());
    assert!(r.success);
    assert!(r.mergelist.iter().any(|m| m.contains("dev-libs/A-3")));
    // = picks the exact one.
    let r = fixture.resolve("=dev-libs/A-1", &opts());
    assert!(r.mergelist.iter().any(|m| m.contains("dev-libs/A-1")));
    // < picks below.
    let r = fixture.resolve("<dev-libs/A-2", &opts());
    assert!(r.mergelist.iter().any(|m| m.contains("dev-libs/A-1")));
    // ~ matches the base version.
    let r = fixture.resolve("~dev-libs/A-2", &opts());
    assert!(r.success);
}

#[test]
fn fixture_noreplace_skips_installed() {
    let fixture = simple_portage_fixture();
    let result = fixture.resolve(
        "dev-libs/A",
        &EmergeOptions {
            noreplace: true,
            ..EmergeOptions::default()
        },
    );
    // dev-libs/A is installed -> noreplace yields an empty merge list.
    assert!(result.success);
    assert!(result.mergelist.is_empty());
}

#[test]
fn fixture_usepkg_selects_binary() {
    let fixture = simple_portage_fixture();
    // dev-libs/B-1.2 exists as a binary package; usepkgonly selects it.
    let result = fixture.resolve(
        "dev-libs/B",
        &EmergeOptions {
            usepkg: true,
            usepkgonly: true,
            ..EmergeOptions::default()
        },
    );
    assert!(result.success, "{:?}", result.error);
    assert!(result.mergelist.iter().any(|m| m.contains("dev-libs/B")));
}

#[test]
fn fixture_unsatisfied_and_invalid() {
    let fixture = simple_portage_fixture();
    let r = fixture.resolve("dev-libs/does-not-exist", &opts());
    assert!(!r.success);
    assert!(r.error.is_some());
    // Invalid atom.
    let r = fixture.resolve("dev-libs/A[bad", &opts());
    assert!(!r.success);
}

#[test]
fn sync_error_display() {
    assert!(format!("{}", SyncError::SourceMissing("x".into())).contains("source missing"));
    assert!(format!("{}", SyncError::Io("boom".into())).contains("boom"));
}
