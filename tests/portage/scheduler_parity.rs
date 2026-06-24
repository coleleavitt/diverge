//! Tests for the merge-plan scheduler.
//!
//! Reference: `research/portage/lib/_emerge/Scheduler.py` (`merge` ordering,
//! stop-on-failure, resume state).

use std::collections::BTreeMap;
use std::path::PathBuf;

use diverge::executor::phase::{BuildDirs, Phase, PhaseContext, PhaseOutcome, PhaseSpawner};
use diverge::executor::scheduler::{PackagePlan, RunMode, Scheduler, TaskStage};

/// A spawner that succeeds unless the cpv (read from the env's PF) is in
/// `fail_pf` for the given phase.
struct FakeSpawner {
    /// (PF, phase) pairs that should fail.
    fail: Vec<(String, Phase)>,
    /// Phases actually run, for assertions.
    ran: Vec<(String, Phase)>,
}

impl PhaseSpawner for FakeSpawner {
    fn run_phase(&mut self, phase: Phase, env: &BTreeMap<String, String>) -> PhaseOutcome {
        let pf = env.get("PF").cloned().unwrap_or_default();
        self.ran.push((pf.clone(), phase));
        let success = !self.fail.iter().any(|(p, ph)| *p == pf && *ph == phase);
        PhaseOutcome {
            phase,
            success,
            message: if success {
                None
            } else {
                Some(format!("{pf}:{} failed", phase.func_name()))
            },
        }
    }
}

/// A plan that builds a trivial phase context for any cpv.
struct FixturePlan;

impl PackagePlan for FixturePlan {
    fn phase_context(&self, cpv: &str) -> PhaseContext {
        PhaseContext {
            ebuild: PathBuf::from(format!("/repo/{cpv}.ebuild")),
            cpv: cpv.to_string(),
            eapi: "7".to_string(),
            root: PathBuf::from("/test-root"),
            dirs: BuildDirs::new(
                PathBuf::from(format!("/build/{cpv}")),
                PathBuf::from("/repo"),
            ),
            use_flags: Vec::new(),
        }
    }
}

fn mergelist(cpvs: &[&str]) -> Vec<String> {
    cpvs.iter().map(|s| s.to_string()).collect()
}

#[test]
fn schedules_full_plan_in_order() {
    let mut spawner = FakeSpawner {
        fail: Vec::new(),
        ran: Vec::new(),
    };
    let mut scheduler = Scheduler::new(RunMode::BuildAndMerge, &mut spawner);
    let plan = FixturePlan;
    let list = mergelist(&["dev-libs/C-1", "dev-libs/B-1", "dev-libs/A-1"]);
    let result = scheduler.run(&list, &plan);

    assert!(result.is_complete(), "{result:?}");
    assert_eq!(result.records.len(), 3);
    // Every package reached the Merged stage.
    assert!(result.records.iter().all(|r| r.stage == TaskStage::Merged));
    // Order is preserved.
    let order: Vec<&str> = result.records.iter().map(|r| r.cpv.as_str()).collect();
    assert_eq!(order, vec!["dev-libs/C-1", "dev-libs/B-1", "dev-libs/A-1"]);
}

#[test]
fn stops_on_first_failure_and_records_remaining() {
    // B fails at src_compile; C should never run.
    let mut spawner = FakeSpawner {
        fail: vec![("B-1".to_string(), Phase::SrcCompile)],
        ran: Vec::new(),
    };
    let mut scheduler = Scheduler::new(RunMode::BuildAndMerge, &mut spawner);
    let plan = FixturePlan;
    let list = mergelist(&["dev-libs/A-1", "dev-libs/B-1", "dev-libs/C-1"]);
    let result = scheduler.run(&list, &plan);

    assert!(!result.is_complete());
    assert_eq!(result.first_failure(), Some("dev-libs/B-1"));
    // A succeeded, B failed, C is left for resume.
    assert_eq!(result.records.len(), 2);
    assert!(result.records[0].success);
    assert!(!result.records[1].success);
    assert_eq!(result.remaining, vec!["dev-libs/C-1"]);
    // C never ran.
    assert!(!spawner.ran.iter().any(|(pf, _)| pf == "C-1"));
}

#[test]
fn pretend_mode_runs_no_phases() {
    let mut spawner = FakeSpawner {
        fail: Vec::new(),
        ran: Vec::new(),
    };
    let mut scheduler = Scheduler::new(RunMode::Pretend, &mut spawner);
    let plan = FixturePlan;
    let result = scheduler.run(&mergelist(&["dev-libs/A-1"]), &plan);
    assert!(result.is_complete());
    assert_eq!(result.records[0].stage, TaskStage::Pending);
    assert!(spawner.ran.is_empty(), "pretend spawns nothing");
}

#[test]
fn buildonly_mode_stops_before_merge_phases() {
    let mut spawner = FakeSpawner {
        fail: Vec::new(),
        ran: Vec::new(),
    };
    let mut scheduler = Scheduler::new(RunMode::BuildOnly, &mut spawner);
    let plan = FixturePlan;
    let result = scheduler.run(&mergelist(&["dev-libs/A-1"]), &plan);
    assert!(result.is_complete());
    assert_eq!(result.records[0].stage, TaskStage::Built);
    // Merge-time phases (pkg_preinst/pkg_postinst) did not run.
    assert!(!spawner.ran.iter().any(|(_, ph)| *ph == Phase::PkgPreinst));
    assert!(!spawner.ran.iter().any(|(_, ph)| *ph == Phase::PkgPostinst));
    // Build phases did run.
    assert!(spawner.ran.iter().any(|(_, ph)| *ph == Phase::SrcCompile));
}

#[test]
fn fetchonly_mode_runs_no_build_phases() {
    let mut spawner = FakeSpawner {
        fail: Vec::new(),
        ran: Vec::new(),
    };
    let mut scheduler = Scheduler::new(RunMode::FetchOnly, &mut spawner);
    let plan = FixturePlan;
    let result = scheduler.run(&mergelist(&["dev-libs/A-1"]), &plan);
    assert!(result.is_complete());
    assert_eq!(result.records[0].stage, TaskStage::Fetched);
    assert!(spawner.ran.is_empty(), "fetch-only spawns no phases");
}
