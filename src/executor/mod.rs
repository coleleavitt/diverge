//! Runtime execution layer: turning a resolved merge plan into filesystem
//! transactions.
//!
//! The executor is split into focused units rather than one god object:
//! - [`config_protect`]: CONFIG_PROTECT path resolution.
//! - [`merge`]: install-image -> root merge transaction with collision checks.
//! - [`unmerge`]: removing an installed package's recorded files.
//!
//! Process/phase spawning (the bash shell-boundary adapter) is intentionally
//! kept separate from this pure-filesystem logic so the merge/unmerge
//! transactions are fully testable against isolated temp roots.

pub mod config_protect;
pub mod ebuild_sh;
pub mod fetch;
pub mod merge;
pub mod phase;
pub mod scheduler;
pub mod spawn;
pub mod unmerge;

pub use config_protect::ConfigProtect;
pub use ebuild_sh::{EBUILD_HELPERS, EbuildSpawner};
pub use fetch::{FetchError, FetchResult, Fetcher, LocalFetcher, Source, fetch_all, fetch_one};
pub use merge::{ContentEntry, MergeError, MergeResult, MergeTransaction};
pub use phase::{
    BuildDirs,
    Phase,
    PhaseContext,
    PhaseOutcome,
    PhaseSpawner,
    build_phases,
    run_build_phases,
};
pub use scheduler::{PackagePlan, RunMode, ScheduleResult, Scheduler, TaskRecord, TaskStage};
pub use spawn::{CommandSpawner, SpawnResult};
pub use unmerge::{UnmergeError, UnmergeResult, unmerge};
