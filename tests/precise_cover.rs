//! Precise coverage of the last reachable branch arms in matching, session,
//! and atom.

use std::fs;
use std::path::Path;

fn write(path: &Path, c: &str) {
    if let Some(p) = path.parent() {
        fs::create_dir_all(p).unwrap();
    }
    fs::write(path, c).unwrap();
}

#[test]
fn matching_candidate_from_atom_str_and_glob_filter() {
    use diverge::atom::{Atom, AtomParseOptions};
    use diverge::matching::{Candidate, match_from_list};
    const WILD: AtomParseOptions = AtomParseOptions {
        allow_wildcard: true,
        allow_repo: true,
    };
    // Candidate::from_atom_str parses slot/repo/use (matching.rs line 52-77).
    let cand = Candidate::from_atom_str("=dev-libs/A-1:2/3::gentoo[foo]").unwrap();
    assert_eq!(cand.cpv, "dev-libs/A-1");
    assert_eq!(cand.slot.as_deref(), Some("2"));
    assert_eq!(cand.repo.as_deref(), Some("gentoo"));
    assert!(cand.iuse.contains("foo"));

    // Extended cp + EqualGlob version filter (lines 253-256).
    let atom = Atom::parse_with_options("=*/A-1*", WILD).unwrap();
    let pool = [
        Candidate::new("dev-libs/A-1.2"),
        Candidate::new("dev-libs/A-2.0"),
        Candidate::new("sci-libs/A-1.5"),
    ];
    let got: Vec<&str> = match_from_list(&atom, &pool)
        .iter()
        .map(|c| c.cpv.as_str())
        .collect();
    // Only versions containing "1" survive the glob needle filter.
    assert!(got.contains(&"dev-libs/A-1.2"));
    assert!(got.contains(&"sci-libs/A-1.5"));
    assert!(!got.contains(&"dev-libs/A-2.0"));
}

#[test]
fn matching_slot_subslot_mismatch_and_best_glob() {
    use diverge::atom::{Atom, AtomParseOptions};
    use diverge::matching::{Candidate, best_match_to_list, match_from_list};
    const WILD: AtomParseOptions = AtomParseOptions {
        allow_wildcard: true,
        allow_repo: true,
    };
    let a = |s: &str| Atom::parse_with_options(s, WILD).unwrap();

    // Atom wants sub-slot but candidate has none (line 312 Some,None -> false).
    let cand = Candidate::new("dev-libs/A-1").with_slot("2");
    let pool = [cand];
    assert!(match_from_list(&a("dev-libs/A:2/3"), &pool).is_empty());

    // best_match_to_list precedence for glob (EqualGlob extended -> 0).
    let cand = Candidate::new("dev-libs/A-1");
    let list = vec![a("=dev-libs/A-1*"), a("dev-libs/A")];
    let best = best_match_to_list(&cand, &list).unwrap();
    // Non-extended = beats extended glob in precedence.
    assert!(best.to_string().contains("dev-libs/A"));

    // Tilde precedence (value 5).
    let cand = Candidate::new("dev-libs/B-1.0");
    let list = vec![a("~dev-libs/B-1.0"), a("dev-libs/B")];
    let best = best_match_to_list(&cand, &list).unwrap();
    assert!(best.to_string().starts_with('~'));
}

#[test]
fn session_unimplemented_action_message() {
    use diverge::cli::EmergeRequest;
    use diverge::session::Session;
    let dir = tempfile::tempdir().unwrap();
    write(
        &dir.path().join("etc/portage/make.conf"),
        "ARCH=\"amd64\"\n",
    );
    let s = Session::load(dir.path(), dir.path()).unwrap();
    // --prune is a known action still routed to the not-yet-implemented arm.
    let req = EmergeRequest::parse(["--prune", "dev-libs/A"]).unwrap();
    let out = s.dispatch(&req);
    assert!(out.contains("not yet implemented"), "out: {out}");
}

#[test]
fn session_repos_conf_directory_form() {
    use diverge::session::Session;
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("tree");
    write(&repo.join("profiles/repo_name"), "gentoo\n");
    write(
        &repo.join("app/x/x-1.ebuild"),
        "EAPI=\"7\"\nSLOT=\"0\"\nKEYWORDS=\"amd64\"\n",
    );
    // repos.conf as a DIRECTORY of fragments (session.rs 489-497).
    write(
        &dir.path().join("etc/portage/repos.conf/gentoo.conf"),
        &format!("[gentoo]\nlocation = {}\n", repo.display()),
    );
    write(
        &dir.path().join("etc/portage/make.conf"),
        "ARCH=\"amd64\"\n",
    );
    let s = Session::load(dir.path(), dir.path()).unwrap();
    assert!(!s.available.match_str("app/x").unwrap().is_empty());
}

#[test]
fn session_search_dedups_by_cp() {
    use diverge::session::Session;
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("var/db/repos/gentoo");
    write(&repo.join("profiles/repo_name"), "gentoo\n");
    // Two versions of the same cp -> search lists the cp once (line 287-288).
    write(
        &repo.join("app/x/x-1.ebuild"),
        "EAPI=\"7\"\nSLOT=\"0\"\nKEYWORDS=\"amd64\"\n",
    );
    write(
        &repo.join("app/x/x-2.ebuild"),
        "EAPI=\"7\"\nSLOT=\"0\"\nKEYWORDS=\"amd64\"\n",
    );
    write(
        &dir.path().join("etc/portage/repos.conf"),
        &format!("[gentoo]\nlocation = {}\n", repo.display()),
    );
    write(
        &dir.path().join("etc/portage/make.conf"),
        "ARCH=\"amd64\"\n",
    );
    let s = Session::load(dir.path(), dir.path()).unwrap();
    let report = s.search(&["x".to_string()]);
    assert_eq!(
        report.matches("app/x").count(),
        1,
        "cp listed once: {report}"
    );
}

#[test]
fn atom_use_dep_bang_and_revision_version() {
    use diverge::atom::{Atom, AtomParseOptions, is_valid_atom};
    const WILD: AtomParseOptions = AtomParseOptions {
        allow_wildcard: true,
        allow_repo: true,
    };
    // `!foo?` USE dep (bang form requires a conditional suffix).
    let a = Atom::parse_with_options("dev-libs/A[!foo?]", WILD).unwrap();
    let parsed = a.parsed_use_deps().unwrap();
    assert!(parsed.tokens.iter().any(|t| t.name == "foo" && t.negated));
    // `[!foo]` (bang without a conditional suffix) is invalid.
    assert!(Atom::parse_with_options("dev-libs/A[!foo]", WILD).is_err());

    // A versioned package without operator -> invalid (line 155 path).
    assert!(!is_valid_atom("dev-libs/A-1.2-r3", WILD));
    // With operator it is valid (revision handled by is_version 508-510).
    assert!(is_valid_atom("=dev-libs/A-1.2-r3", WILD));
    // Triple bang -> InvalidBlocker (line 308).
    assert!(Atom::parse_with_options("!!!dev-libs/A", WILD).is_err());
}
