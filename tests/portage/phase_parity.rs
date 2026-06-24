//! Tests for the ebuild phase sequencing and environment construction.
//!
//! Reference:
//! - `research/portage/bin/ebuild.sh`, `phase-functions.sh`
//! - `research/portage/lib/_emerge/EbuildPhase.py`

use std::collections::BTreeMap;
use std::path::PathBuf;

use diverge::executor::phase::{
    BuildDirs,
    Phase,
    PhaseContext,
    PhaseOutcome,
    PhaseSpawner,
    build_phases,
    merge_phases,
    phase_argv,
    run_build_phases,
};

/// A fake spawner that records the phases and envs it was asked to run, and can
/// be told to fail at a specific phase.
#[derive(Default)]
struct RecordingSpawner {
    runs: Vec<(Phase, BTreeMap<String, String>)>,
    fail_at: Option<Phase>,
}

impl PhaseSpawner for RecordingSpawner {
    fn run_phase(&mut self, phase: Phase, env: &BTreeMap<String, String>) -> PhaseOutcome {
        self.runs.push((phase, env.clone()));
        let success = self.fail_at != Some(phase);
        PhaseOutcome {
            phase,
            success,
            message: None,
        }
    }
}

fn ctx(eapi: &str) -> PhaseContext {
    let build = PathBuf::from("/var/tmp/portage/dev-libs/A-1");
    let ebuild_dir = PathBuf::from("/repo/dev-libs/A");
    PhaseContext {
        ebuild: ebuild_dir.join("A-1.ebuild"),
        cpv: "dev-libs/A-1".to_string(),
        eapi: eapi.to_string(),
        root: PathBuf::from("/test-root"),
        dirs: BuildDirs::new(build, ebuild_dir),
        use_flags: vec!["foo".to_string(), "bar".to_string()],
    }
}

#[test]
fn build_phase_order_respects_eapi() {
    // EAPI 0: no src_prepare/src_configure.
    let phases = build_phases("0");
    assert!(!phases.contains(&Phase::SrcPrepare));
    assert!(!phases.contains(&Phase::SrcConfigure));
    assert_eq!(phases.first(), Some(&Phase::PkgSetup));

    // EAPI 7: full modern sequence in order.
    let phases = build_phases("7");
    assert_eq!(
        phases,
        vec![
            Phase::PkgSetup,
            Phase::SrcUnpack,
            Phase::SrcPrepare,
            Phase::SrcConfigure,
            Phase::SrcCompile,
            Phase::SrcTest,
            Phase::SrcInstall,
        ]
    );
}

#[test]
fn merge_phases_are_preinst_then_postinst() {
    assert_eq!(merge_phases(), vec![Phase::PkgPreinst, Phase::PkgPostinst]);
}

#[test]
fn environment_sets_required_phase_variables() {
    let context = ctx("7");
    let env = context.environment(Phase::SrcCompile);
    assert_eq!(
        env.get("EBUILD_PHASE_FUNC").map(String::as_str),
        Some("src_compile")
    );
    assert_eq!(env.get("EBUILD_PHASE").map(String::as_str), Some("compile"));
    assert_eq!(env.get("CATEGORY").map(String::as_str), Some("dev-libs"));
    assert_eq!(env.get("PF").map(String::as_str), Some("A-1"));
    assert_eq!(env.get("EAPI").map(String::as_str), Some("7"));
    assert_eq!(env.get("USE").map(String::as_str), Some("foo bar"));
    assert!(env.get("WORKDIR").unwrap().ends_with("/work"));
    assert!(env.get("D").unwrap().ends_with("/image"));
    assert!(env.get("T").unwrap().ends_with("/temp"));
    assert!(env.get("FILESDIR").unwrap().ends_with("/files"));
}

#[test]
fn run_build_phases_runs_full_sequence_on_success() {
    let context = ctx("7");
    let mut spawner = RecordingSpawner::default();
    let outcomes = run_build_phases(&context, &mut spawner);
    assert_eq!(outcomes.len(), 7);
    assert!(outcomes.iter().all(|o| o.success));
    // The recorded order matches the build sequence.
    let ran: Vec<Phase> = spawner.runs.iter().map(|(p, _)| *p).collect();
    assert_eq!(ran, build_phases("7"));
}

#[test]
fn run_build_phases_stops_at_first_failure() {
    let context = ctx("7");
    let mut spawner = RecordingSpawner {
        fail_at: Some(Phase::SrcConfigure),
        ..Default::default()
    };
    let outcomes = run_build_phases(&context, &mut spawner);
    // setup, unpack, prepare, configure(fail) -> 4 outcomes, last failed.
    assert_eq!(outcomes.len(), 4);
    assert!(!outcomes.last().unwrap().success);
    assert_eq!(outcomes.last().unwrap().phase, Phase::SrcConfigure);
    // Nothing after the failure ran.
    assert!(!spawner.runs.iter().any(|(p, _)| *p == Phase::SrcCompile));
}

#[test]
fn phase_argv_is_structured_not_concatenated() {
    let argv = phase_argv(
        &PathBuf::from("/usr/lib/portage/bin/ebuild.sh"),
        Phase::SrcInstall,
    );
    assert_eq!(
        argv,
        vec![
            "/usr/lib/portage/bin/ebuild.sh".to_string(),
            "src_install".to_string()
        ]
    );
}
