use diverge::cli::EmergeOptions;
use diverge::resolver::{representative_ports, simple_portage_fixture};

#[test]
fn representative_port_manifest_points_to_reference_and_rust_tests() {
    let ports = representative_ports();
    assert!(ports.iter().any(|port| {
        port.reference == "research/portage/lib/portage/tests/dep/test_atom.py"
            && port.rust_test == "tests/portage/atom_parity.rs"
    }));
    assert!(ports.iter().any(|port| {
        port.reference == "research/portage/lib/portage/tests/resolver/test_simple.py"
            && port.rust_test == "tests/portage/resolver_simple_parity.rs"
    }));
}

#[test]
fn simple_resolver_cases_ported_from_portage() {
    let fixture = simple_portage_fixture();

    let result = fixture.resolve("dev-libs/A", &EmergeOptions::default());
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.mergelist, ["dev-libs/A-1"]);

    let result = fixture.resolve(
        "=dev-libs/A-2",
        &EmergeOptions {
            autounmask: Some(false),
            ..EmergeOptions::default()
        },
    );
    assert!(!result.success);

    let result = fixture.resolve(
        "dev-libs/A",
        &EmergeOptions {
            noreplace: true,
            ..EmergeOptions::default()
        },
    );
    assert!(result.success, "{:?}", result.error);
    assert!(result.mergelist.is_empty());

    let result = fixture.resolve(
        "dev-libs/B",
        &EmergeOptions {
            noreplace: true,
            ..EmergeOptions::default()
        },
    );
    assert!(result.success, "{:?}", result.error);
    assert!(result.mergelist.is_empty());

    let result = fixture.resolve(
        "dev-libs/B",
        &EmergeOptions {
            update: true,
            ..EmergeOptions::default()
        },
    );
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.mergelist, ["dev-libs/B-1.2"]);

    let result = fixture.resolve(
        "dev-libs/B",
        &EmergeOptions {
            update: true,
            usepkg: true,
            ..EmergeOptions::default()
        },
    );
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.mergelist, ["[binary]dev-libs/B-1.2"]);

    let result = fixture.resolve(
        "dev-libs/B",
        &EmergeOptions {
            update: true,
            usepkg: true,
            usepkgonly: true,
            ..EmergeOptions::default()
        },
    );
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.mergelist, ["[binary]dev-libs/B-1.2"]);

    let result = fixture.resolve("app-misc/Z", &EmergeOptions::default());
    assert!(result.success, "{:?}", result.error);
    assert!(
        result.mergelist == ["app-misc/W-1", "app-misc/X-1", "app-misc/Z-1"]
            || result.mergelist == ["app-misc/X-1", "app-misc/W-1", "app-misc/Z-1"],
        "Portage marks this dependency order ambiguous; got {:?}",
        result.mergelist,
    );
}
