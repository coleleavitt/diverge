use std::cmp::Ordering;

use diverge::version::vercmp;

#[test]
fn vercmp_greater_cases_ported_from_portage() {
    for (left, right) in [
        ("6.0", "5.0"),
        ("5.0", "5"),
        ("1.0-r1", "1.0-r0"),
        ("1.0-r1", "1.0"),
        (
            "999999999999999999999999999999",
            "999999999999999999999999999998",
        ),
        ("1.0.0", "1.0"),
        ("1.0.0", "1.0b"),
        ("1b", "1"),
        ("1b_p1", "1_p1"),
        ("1.1b", "1.1"),
        ("12.2.5", "12.2b"),
    ] {
        assert_eq!(vercmp(left, right), Ordering::Greater, "{left} > {right}");
    }
}

#[test]
fn vercmp_less_cases_ported_from_portage() {
    for (left, right) in [
        ("4.0", "5.0"),
        ("5", "5.0"),
        ("1.0_pre2", "1.0_p2"),
        ("1.0_alpha2", "1.0_p2"),
        ("1.0_alpha1", "1.0_beta1"),
        ("1.0_beta3", "1.0_rc3"),
        ("1.001000000000000000001", "1.001000000000000000002"),
        ("1.00100000000", "1.0010000000000000001"),
        (
            "999999999999999999999999999998",
            "999999999999999999999999999999",
        ),
        ("1.01", "1.1"),
        ("1.0-r0", "1.0-r1"),
        ("1.0", "1.0-r1"),
        ("1.0", "1.0.0"),
        ("1.0b", "1.0.0"),
        ("1_p1", "1b_p1"),
        ("1", "1b"),
        ("1.1", "1.1b"),
        ("12.2b", "12.2.5"),
    ] {
        assert_eq!(vercmp(left, right), Ordering::Less, "{left} < {right}");
    }
}

#[test]
fn vercmp_equal_cases_ported_from_portage() {
    for (left, right) in [
        ("4.0", "4.0"),
        ("1.0", "1.0"),
        ("1.0-r0", "1.0"),
        ("1.0", "1.0-r0"),
        ("1.0-r0", "1.0-r0"),
        ("1.0-r1", "1.0-r1"),
    ] {
        assert_eq!(vercmp(left, right), Ordering::Equal, "{left} == {right}");
    }
}

#[test]
fn vercmp_not_equal_cases_ported_from_portage() {
    for (left, right) in [
        ("1", "2"),
        ("1.0_alpha", "1.0_pre"),
        ("1.0_beta", "1.0_alpha"),
        ("0", "0.0"),
        ("1.0-r0", "1.0-r1"),
        ("1.0-r1", "1.0-r0"),
        ("1.0", "1.0-r1"),
        ("1.0-r1", "1.0"),
        ("1.0", "1.0.0"),
        ("1_p1", "1b_p1"),
        ("1b", "1"),
        ("1.1b", "1.1"),
        ("12.2b", "12.2"),
    ] {
        assert_ne!(vercmp(left, right), Ordering::Equal, "{left} != {right}");
    }
}
