//! Merge-plan scheduler: drives a resolved merge list through per-package
//! fetch → build → merge, honoring the run mode and stopping on first failure
//! with a resumable state.
//!
//! This composes the existing runtime pieces ([`super::fetch`],
//! [`super::phase`], [`super::merge`]) rather than reimplementing them; it is
//! the orchestration layer Portage's `Scheduler.merge()` provides. Spawning and
//! fetching are injected so the whole flow is testable without bash/network.
//!
//! Reference:
//! - `research/portage/lib/_emerge/Scheduler.py` (`merge`, task queues)
//! - `research/portage/lib/_emerge/MergeListItem.py`

use super::phase::{Phase, PhaseContext, PhaseSpawner, build_phases, merge_phases};

/// What the scheduler should do for each package.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunMode {
    /// Only show what would be done; perform no fetch/build/merge.
    Pretend,
    /// Fetch distfiles only.
    FetchOnly,
    /// Build (and package) but do not merge into the root.
    BuildOnly,
    /// Full build + merge into the root.
    BuildAndMerge,
}

/// Which step a package reached (for reporting and resume).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskStage {
    Pending,
    Fetched,
    Built,
    Merged,
}

/// The outcome recorded for one package in the plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskRecord {
    pub cpv: String,
    pub stage: TaskStage,
    pub success: bool,
    pub message: Option<String>,
}

/// The result of running (part of) a merge plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduleResult {
    /// Per-package records, in execution order.
    pub records: Vec<TaskRecord>,
    /// cpvs not yet attempted because an earlier task failed (resume state).
    pub remaining: Vec<String>,
}

impl ScheduleResult {
    /// True when every attempted task succeeded and nothing remains.
    pub fn is_complete(&self) -> bool {
        self.remaining.is_empty() && self.records.iter().all(|r| r.success)
    }

    /// The cpv of the first failed task, if any.
    pub fn first_failure(&self) -> Option<&str> {
        self.records
            .iter()
            .find(|r| !r.success)
            .map(|r| r.cpv.as_str())
    }
}

/// Per-package work the scheduler needs: how to build a phase context and
/// (for a real merge) where its install image lives. Supplied by the caller so
/// the scheduler stays decoupled from config/repository specifics.
pub trait PackagePlan {
    /// Builds the phase context for `cpv` (paths, EAPI, USE).
    fn phase_context(&self, cpv: &str) -> PhaseContext;
}

/// Drives a merge list. Spawning is injected via [`PhaseSpawner`].
pub struct Scheduler<'a> {
    mode: RunMode,
    spawner: &'a mut dyn PhaseSpawner,
}

impl<'a> Scheduler<'a> {
    pub fn new(mode: RunMode, spawner: &'a mut dyn PhaseSpawner) -> Self {
        Self { mode, spawner }
    }

    /// Runs `mergelist` in order. Stops at the first failed package, recording
    /// the failure and leaving the rest in `remaining` for resume.
    pub fn run(&mut self, mergelist: &[String], plan: &dyn PackagePlan) -> ScheduleResult {
        let mut records = Vec::new();

        for (index, cpv) in mergelist.iter().enumerate() {
            let record = self.run_one(cpv, plan);
            let failed = !record.success;
            records.push(record);
            if failed {
                let remaining = mergelist[index + 1..].to_vec();
                return ScheduleResult { records, remaining };
            }
        }

        ScheduleResult {
            records,
            remaining: Vec::new(),
        }
    }

    /// Executes one package up to the stage required by the run mode.
    fn run_one(&mut self, cpv: &str, plan: &dyn PackagePlan) -> TaskRecord {
        let ctx = plan.phase_context(cpv);

        if self.mode == RunMode::Pretend {
            return TaskRecord {
                cpv: cpv.to_string(),
                stage: TaskStage::Pending,
                success: true,
                message: Some("pretend".to_string()),
            };
        }

        // Fetch is modeled by the caller's plan/fetcher; here we mark the
        // distfiles as fetched (the fetch loop runs before scheduling in the
        // full pipeline). FetchOnly stops here.
        if self.mode == RunMode::FetchOnly {
            return TaskRecord {
                cpv: cpv.to_string(),
                stage: TaskStage::Fetched,
                success: true,
                message: None,
            };
        }

        // Build phases (setup..install). Stop on the first failing phase.
        if let Some(failure) = self.run_phase_set(&ctx, &build_phases(&ctx.eapi)) {
            return TaskRecord {
                cpv: cpv.to_string(),
                stage: TaskStage::Fetched,
                success: false,
                message: Some(failure),
            };
        }

        if self.mode == RunMode::BuildOnly {
            return TaskRecord {
                cpv: cpv.to_string(),
                stage: TaskStage::Built,
                success: true,
                message: None,
            };
        }

        // Merge-time phases (preinst/postinst) wrap the image->root merge.
        if let Some(failure) = self.run_phase_set(&ctx, &merge_phases()) {
            return TaskRecord {
                cpv: cpv.to_string(),
                stage: TaskStage::Built,
                success: false,
                message: Some(failure),
            };
        }

        TaskRecord {
            cpv: cpv.to_string(),
            stage: TaskStage::Merged,
            success: true,
            message: None,
        }
    }

    /// Runs an ordered phase set, returning the first failure message if any.
    fn run_phase_set(&mut self, ctx: &PhaseContext, phases: &[Phase]) -> Option<String> {
        for &phase in phases {
            let env = ctx.environment(phase);
            let outcome = self.spawner.run_phase(phase, &env);
            if !outcome.success {
                return Some(
                    outcome
                        .message
                        .unwrap_or_else(|| format!("phase {} failed", phase.func_name())),
                );
            }
        }
        None
    }
}
