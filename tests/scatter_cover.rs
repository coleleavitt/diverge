//! Scattered reachable-arm coverage: ProfileError Display + From, depgraph
//! update no-higher-version, matching extended recursion, required_use close.

use std::fs;
use std::path::Path;

fn write(path: &Path, c: &str) {
    if let Some(p) = path.parent() {
        fs::create_dir_all(p).unwrap();
    }
    fs::write(path, c).unwrap();
}

#[test]
fn profile_error_display_and_from() {
    use std::path::PathBuf;

    use diverge::config::ParseError;
    use diverge::profile::ProfileError;
    assert!(format!("{}", ProfileError::MissingProfile(PathBuf::from("/x"))).contains("not found"));
    assert!(format!("{}", ProfileError::EmptyParent(PathBuf::from("/x"))).contains("empty"));
    assert!(
        format!(
            "{}",
            ProfileError::ParentNotFound {
                parent: "p".into(),
                referenced_by: PathBuf::from("/x")
            }
        )
        .contains("not found")
    );
    assert!(format!("{}", ProfileError::Io("boom".into())).contains("boom"));
    // From<ParseError> conversion + its Display.
    let e: ProfileError = ParseError("bad".to_string()).into();
    assert!(format!("{e}").contains("bad"));
}

#[test]
fn depgraph_update_no_higher_keeps_installed() {
    use diverge::dbapi::{PackageDb, PackageMetadata};
    use diverge::depgraph::{ResolveParams, Resolver};
    fn pkg(deps: &[(&str, &str)]) -> PackageMetadata {
        let mut m = PackageMetadata {
            slot: Some("0".into()),
            sub_slot: None,
            repo: Some("r".into()),
            eapi: Some("7".into()),
            iuse: vec![],
            use_enabled: vec![],
            keywords: vec!["x86".into()],
            deps: Default::default(),
        };
        for (k, v) in deps {
            m.deps.insert((*k).to_string(), (*v).to_string());
        }
        m
    }
    // --update but the installed dep is already the highest version available
    // -> no reinstall (depgraph update path, `higher` false / line 361 _ arm).
    let mut av = PackageDb::new();
    av.insert("a/main-1", pkg(&[("RDEPEND", "a/lib")]));
    av.insert("a/lib-1", pkg(&[]));
    let mut installed = PackageDb::new();
    installed.insert("a/lib-1", pkg(&[]));
    let params = ResolveParams::default().with_update(true).with_deep(true);
    let outcome = Resolver::new(&av, &installed, params).resolve(&["a/main"]);
    assert!(outcome.is_success(), "{:?}", outcome.error);
    // lib-1 already newest -> not in the merge list.
    assert!(!outcome.mergelist.contains(&"a/lib-1".to_string()));
}

#[test]
fn matching_extended_star_mid_pattern() {
    use diverge::atom::{Atom, AtomParseOptions};
    use diverge::matching::{Candidate, match_from_list};
    const WILD: AtomParseOptions = AtomParseOptions {
        allow_wildcard: true,
        allow_repo: true,
    };
    let a = |s: &str| Atom::parse_with_options(s, WILD).unwrap();
    // `dev-*s/A` exercises extended_cp_match's `*` recursion in the middle.
    let pool = [
        Candidate::new("dev-libs/A-1"),
        Candidate::new("dev-utils/A-1"),
        Candidate::new("sci-libs/A-1"),
    ];
    let got: Vec<&str> = match_from_list(&a("dev-*/A"), &pool)
        .iter()
        .map(|c| c.cpv.as_str())
        .collect();
    assert!(got.contains(&"dev-libs/A-1"));
    assert!(got.contains(&"dev-utils/A-1"));
    assert!(!got.contains(&"sci-libs/A-1"));
}

#[test]
fn required_use_flags_unmatched_close_errors() {
    use diverge::matching::get_required_use_flags;
    // A `)` with no opening group -> bad() (matching.rs 525 region).
    assert!(get_required_use_flags("a )").is_err());
    assert!(get_required_use_flags("( a").is_err()); // unclosed
    assert!(get_required_use_flags("|| ( a )").is_ok());
}

#[test]
fn session_depclean_and_install_error() {
    use diverge::cli::EmergeRequest;
    use diverge::session::Session;
    let dir = tempfile::tempdir().unwrap();
    write(
        &dir.path().join("etc/portage/make.conf"),
        "ARCH=\"amd64\"\n",
    );
    let s = Session::load(dir.path(), dir.path()).unwrap();
    // dispatch a depclean (line 271) with an empty system -> empty cleanlist.
    let req = EmergeRequest::parse(["--depclean"]).unwrap();
    let report = s.dispatch(&req);
    assert!(report.contains("Total: 0 package"), "report: {report}");
    // install_image of a missing image -> error (session install error arm).
    let res = s.install_image("app/x-1", &dir.path().join("no-image"), true);
    assert!(res.is_err());
}
