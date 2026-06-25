//! Coverage for version suffix/segment comparison edge cases and depgraph/
//! scheduler/dep deeper branches.

use std::cmp::Ordering;

use diverge::version::vercmp;

#[test]
fn vercmp_letter_and_segment_branches() {
    // letter vs no-letter with trailing segments (lines 139-144).
    assert_eq!(vercmp("1.0a", "1.0.1"), Ordering::Less); // letter, right has more
    assert_eq!(vercmp("1.0.1", "1.0a"), Ordering::Greater);
    assert_eq!(vercmp("1.0a", "1.0b"), Ordering::Less); // both letters
    assert_eq!(vercmp("1.0b", "1.0a"), Ordering::Greater);
    assert_eq!(vercmp("1.0a", "1.0"), Ordering::Greater); // letter, no more
    assert_eq!(vercmp("1.0", "1.0a"), Ordering::Less);
    // differing segment counts.
    assert_eq!(vercmp("1.0.1", "1.0"), Ordering::Greater);
    assert_eq!(vercmp("1.0", "1.0.1"), Ordering::Less);
    assert_eq!(vercmp("1.0", "1.0"), Ordering::Equal);
}

#[test]
fn vercmp_suffix_ranks_and_unknown() {
    // suffix ordering: alpha < beta < pre < rc < (none) < p.
    assert_eq!(vercmp("1_alpha", "1_beta"), Ordering::Less);
    assert_eq!(vercmp("1_beta", "1_pre"), Ordering::Less);
    assert_eq!(vercmp("1_pre", "1_rc"), Ordering::Less);
    assert_eq!(vercmp("1_rc", "1"), Ordering::Less);
    assert_eq!(vercmp("1", "1_p"), Ordering::Less);
    // suffix with numbers.
    assert_eq!(vercmp("1_alpha1", "1_alpha2"), Ordering::Less);
    // unknown suffix token treated as neutral p0 (line 96) -> stays total and
    // self-consistent (reflexive equality).
    assert_eq!(vercmp("1_weird", "1_weird"), Ordering::Equal);
    // revisions.
    assert_eq!(vercmp("1-r1", "1-r2"), Ordering::Less);
    assert_eq!(vercmp("1", "1-r1"), Ordering::Less);
}

#[test]
fn depgraph_autounmask_no_change_returns_failure() {
    use diverge::dbapi::{PackageDb, PackageMetadata};
    use diverge::depgraph::{ResolveFailure, ResolveParams, Resolver};

    fn pkg(kw: &[&str]) -> PackageMetadata {
        PackageMetadata {
            slot: Some("0".into()),
            sub_slot: None,
            repo: Some("r".into()),
            eapi: Some("7".into()),
            iuse: vec![],
            use_enabled: vec![],
            keywords: kw.iter().map(|s| s.to_string()).collect(),
            deps: Default::default(),
        }
    }
    // autounmask on, but the missing package simply does not exist (no unstable
    // variant) -> autounmask finds no change -> falls back to the failure.
    let available = PackageDb::new();
    let params = ResolveParams::default().with_autounmask(true);
    let outcome = Resolver::new(&available, &PackageDb::new(), params).resolve(&["d/missing"]);
    assert!(matches!(
        outcome.error,
        Some(ResolveFailure::Unsatisfied(_))
    ));
    assert!(outcome.unstable_keywords.is_empty());

    // autounmask on with a stable package available -> success, no changes.
    let mut av = PackageDb::new();
    av.insert("d/A-1", pkg(&["amd64"]));
    let params = ResolveParams::default()
        .with_arch("amd64")
        .with_autounmask(true);
    let outcome = Resolver::new(&av, &PackageDb::new(), params).resolve(&["d/A"]);
    assert!(outcome.is_success());
    assert!(outcome.unstable_keywords.is_empty());
}

#[test]
fn scheduler_merge_phase_failure_recorded() {
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    use diverge::executor::phase::{BuildDirs, Phase, PhaseContext, PhaseOutcome, PhaseSpawner};
    use diverge::executor::scheduler::{PackagePlan, RunMode, Scheduler, TaskStage};

    // Spawner that fails at pkg_preinst (a merge-time phase).
    struct FailMerge;
    impl PhaseSpawner for FailMerge {
        fn run_phase(&mut self, phase: Phase, _e: &BTreeMap<String, String>) -> PhaseOutcome {
            PhaseOutcome {
                phase,
                success: phase != Phase::PkgPreinst,
                message: if phase == Phase::PkgPreinst {
                    Some("preinst failed".into())
                } else {
                    None
                },
            }
        }
    }
    struct Plan;
    impl PackagePlan for Plan {
        fn phase_context(&self, cpv: &str) -> PhaseContext {
            PhaseContext {
                ebuild: PathBuf::from(cpv),
                cpv: cpv.to_string(),
                eapi: "7".into(),
                root: PathBuf::from("/r"),
                dirs: BuildDirs::new(PathBuf::from("/b"), PathBuf::from("/r")),
                use_flags: vec![],
            }
        }
    }
    let mut sp = FailMerge;
    let mut sched = Scheduler::new(RunMode::BuildAndMerge, &mut sp);
    let res = sched.run(&["c/a-1".to_string()], &Plan);
    assert!(!res.is_complete());
    // Build phases passed, so it reached Built before the merge-phase failure.
    assert_eq!(res.records[0].stage, TaskStage::Built);
    assert!(!res.records[0].success);
}
