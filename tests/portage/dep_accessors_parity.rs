//! Ported from research/portage/lib/portage/tests/dep/test_dep_getcpv.py,
//! test_dep_getslot.py, test_dep_getusedeps.py, test_dep_getrepo.py,
//! test_get_operator.py, and test_isjustname.py.

use diverge::dep::{
    dep_getcpv,
    dep_getrepo,
    dep_getslot,
    dep_getusedeps,
    get_operator,
    isjustname,
};

#[test]
fn dep_getcpv_strips_operators_and_slots() {
    let prefix_ops = ["<", ">", "=", "~", "<=", ">="];
    let cpv = "sys-apps/portage-2.1";
    for slot in [None, Some(":foo"), Some(":2")] {
        for prefix in prefix_ops {
            let mut mydep = format!("{prefix}{cpv}");
            if let Some(slot) = slot {
                mydep.push_str(slot);
            }
            assert_eq!(dep_getcpv(&mydep).as_deref(), Some(cpv), "{mydep}");
        }
        let mut glob = format!("={cpv}*");
        if let Some(slot) = slot {
            glob.push_str(slot);
        }
        assert_eq!(dep_getcpv(&glob).as_deref(), Some(cpv), "{glob}");
    }
}

#[test]
fn dep_getslot_returns_slot_text() {
    let slots = ["a", "1.2", "1", "IloveVapier"];
    for version in ["2.1.1", "2.1-r1"] {
        for slot in slots {
            let mydep = format!("=sys-apps/portage-{version}:{slot}");
            assert_eq!(dep_getslot(&mydep).as_deref(), Some(slot), "{mydep}");
        }
        let mydep = format!("=sys-apps/portage-{version}");
        assert_eq!(dep_getslot(&mydep), None, "{mydep}");
    }
}

#[test]
fn dep_getrepo_returns_repository() {
    let repos = ["a", "repo-name", "repo_name", "repo123"];
    for version in ["2.1.1", "2.1-r1", ""] {
        for use_dep in ["[use]", ""] {
            for repo in repos {
                let mut pkg = String::from("sys-apps/portage");
                if !version.is_empty() {
                    pkg = format!("=sys-apps/portage-{version}");
                }
                pkg.push_str("::");
                pkg.push_str(repo);
                pkg.push_str(use_dep);
                assert_eq!(dep_getrepo(&pkg).as_deref(), Some(repo), "{pkg}");
            }
            // No repository qualifier present.
            let mut pkg = String::from("sys-apps/portage");
            if !version.is_empty() {
                pkg = format!("=sys-apps/portage-{version}");
            }
            pkg.push_str(use_dep);
            assert_eq!(dep_getrepo(&pkg), None, "{pkg}");
        }
    }
}

#[test]
fn dep_getusedeps_extracts_flags() {
    let cps = ["sys-apps/portage", "virtual/portage"];
    let versions = ["1.0", "1.0-r1", "2.3_p4", "1.0_alpha57"];
    let slots = [
        None,
        Some("1"),
        Some("gentoo-sources-2.6.17"),
        Some("spankywashere"),
    ];
    let usedeps: [&[&str]; 5] = [
        &["foo"],
        &["-bar"],
        &["foo", "bar"],
        &["foo", "-bar"],
        &["foo?", "!bar?"],
    ];

    for cp in cps {
        for version in versions {
            for slot in slots {
                for use_group in usedeps {
                    let mut cpv = format!("={cp}-{version}");
                    if let Some(slot) = slot {
                        cpv.push(':');
                        cpv.push_str(slot);
                    }
                    cpv.push('[');
                    cpv.push_str(&use_group.join(","));
                    cpv.push(']');
                    assert_eq!(
                        dep_getusedeps(&cpv).unwrap(),
                        use_group.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
                        "{cpv}",
                    );
                }
            }
        }
    }
}

#[test]
fn get_operator_matches_portage() {
    let tests = [
        ("~", "~"),
        ("=", "="),
        (">", ">"),
        (">=", ">="),
        ("<=", "<="),
    ];
    for slot in [None, Some("1"), Some("linux-2.5.6")] {
        for (op, expected) in tests {
            let mut atom = format!("{op}sys-apps/portage-2.1");
            if let Some(slot) = slot {
                atom.push(':');
                atom.push_str(slot);
            }
            assert_eq!(get_operator(&atom).as_deref(), Some(expected), "{atom}");
        }
    }
    assert_eq!(get_operator("sys-apps/portage"), None);
    assert_eq!(
        get_operator("=sys-apps/portage-2.1*").as_deref(),
        Some("=*")
    );
}

#[test]
fn isjustname_matches_portage() {
    let cats = ["", "sys-apps/", "foo/", "virtual/"];
    let pkgs = ["portage", "paludis", "pkgcore", "notARealPkg"];
    let versioned = ["-2.0-r3", "-1.0_pre2", "-3.1b"];

    for pkg in pkgs {
        for cat in cats {
            assert!(isjustname(&format!("{cat}{pkg}")), "{cat}{pkg}");
            for ver in versioned {
                assert!(!isjustname(&format!("{cat}{pkg}{ver}")), "{cat}{pkg}{ver}");
            }
        }
    }
}
