//! Ebuild phase execution: the safe shell-boundary adapter.
//!
//! Emerge runs an ebuild through an ordered sequence of phase functions
//! (`pkg_setup`, `src_unpack`, ... `pkg_postinst`) by invoking `ebuild.sh` with
//! a controlled environment. This module models that contract WITHOUT being a
//! shell interpreter: it computes the phase order for an EAPI, builds the phase
//! environment explicitly (`EBUILD`, `T`, `D`, `WORKDIR`, `FILESDIR`, ...), and
//! spawns each phase through a structured argv — never string concatenation.
//!
//! Process spawning is injected via the [`PhaseSpawner`] trait so the phase
//! sequencing and environment construction are unit-testable without bash.
//!
//! Reference:
//! - `research/portage/bin/ebuild.sh`, `phase-functions.sh`, `phase-helpers.sh`
//! - `research/portage/lib/_emerge/EbuildPhase.py`, `EbuildProcess.py`

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// One ebuild phase function.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    PkgSetup,
    SrcUnpack,
    SrcPrepare,
    SrcConfigure,
    SrcCompile,
    SrcTest,
    SrcInstall,
    PkgPreinst,
    PkgPostinst,
    /// The standalone `pkg_config` phase, run by `emerge --config`.
    PkgConfig,
    /// The standalone `pkg_info` phase.
    PkgInfo,
}

impl Phase {
    /// The phase function name passed to `ebuild.sh`.
    pub fn func_name(self) -> &'static str {
        match self {
            Self::PkgSetup => "pkg_setup",
            Self::SrcUnpack => "src_unpack",
            Self::SrcPrepare => "src_prepare",
            Self::SrcConfigure => "src_configure",
            Self::SrcCompile => "src_compile",
            Self::SrcTest => "src_test",
            Self::SrcInstall => "src_install",
            Self::PkgPreinst => "pkg_preinst",
            Self::PkgPostinst => "pkg_postinst",
            Self::PkgConfig => "pkg_config",
            Self::PkgInfo => "pkg_info",
        }
    }

    /// Whether this phase is gated by an EAPI feature. `src_prepare` and
    /// `src_configure` only exist from EAPI 2 onward.
    fn available_in(self, eapi: &str) -> bool {
        let modern = matches!(eapi, "2" | "3" | "4" | "5" | "6" | "7" | "8");
        match self {
            Self::SrcPrepare | Self::SrcConfigure => modern,
            _ => true,
        }
    }
}

/// The full build phase sequence (setup through install) for an EAPI, in order.
/// `pkg_preinst`/`pkg_postinst` run at merge time and are returned separately
/// by [`merge_phases`].
pub fn build_phases(eapi: &str) -> Vec<Phase> {
    [
        Phase::PkgSetup,
        Phase::SrcUnpack,
        Phase::SrcPrepare,
        Phase::SrcConfigure,
        Phase::SrcCompile,
        Phase::SrcTest,
        Phase::SrcInstall,
    ]
    .into_iter()
    .filter(|p| p.available_in(eapi))
    .collect()
}

/// The merge-time phases (run when installing the image into the root).
pub fn merge_phases() -> Vec<Phase> {
    vec![Phase::PkgPreinst, Phase::PkgPostinst]
}

/// The filesystem layout of a build, mirroring Portage's `PORTAGE_BUILDDIR`.
///
/// Filesystem ownership is explicit: every path is rooted under `build_dir`.
#[derive(Debug, Clone)]
pub struct BuildDirs {
    /// `PORTAGE_BUILDDIR`: the package's build directory.
    pub build_dir: PathBuf,
    /// `WORKDIR`: where sources are unpacked (`<build>/work`).
    pub work_dir: PathBuf,
    /// `T`: the temp dir for the phase (`<build>/temp`).
    pub temp_dir: PathBuf,
    /// `D`: the install image (`<build>/image`).
    pub image_dir: PathBuf,
    /// `FILESDIR`: the ebuild's `files/` directory.
    pub files_dir: PathBuf,
}

impl BuildDirs {
    /// Derives the standard subdirectories from a package build directory.
    pub fn new(build_dir: impl Into<PathBuf>, ebuild_dir: impl AsRef<Path>) -> Self {
        let build_dir = build_dir.into();
        Self {
            work_dir: build_dir.join("work"),
            temp_dir: build_dir.join("temp"),
            image_dir: build_dir.join("image"),
            files_dir: ebuild_dir.as_ref().join("files"),
            build_dir,
        }
    }

    /// Creates the build/work/temp/image directories on disk.
    pub fn create(&self) -> std::io::Result<()> {
        for dir in [
            &self.build_dir,
            &self.work_dir,
            &self.temp_dir,
            &self.image_dir,
        ] {
            std::fs::create_dir_all(dir)?;
        }
        Ok(())
    }
}

/// The immutable inputs needed to run an ebuild's phases.
#[derive(Debug, Clone)]
pub struct PhaseContext {
    /// Path to the ebuild file (`EBUILD`).
    pub ebuild: PathBuf,
    /// The package cpv (`category/package-version`).
    pub cpv: String,
    /// The EAPI (gates phase availability).
    pub eapi: String,
    /// The target install root (`ROOT`).
    pub root: PathBuf,
    /// Build directory layout.
    pub dirs: BuildDirs,
    /// Enabled USE flags, space-joined into `USE`.
    pub use_flags: Vec<String>,
}

impl PhaseContext {
    /// Builds the explicit, controlled environment for a phase. The returned
    /// map is the *complete* set of phase variables — callers spawn with this
    /// env, never inheriting or concatenating untrusted strings.
    pub fn environment(&self, phase: Phase) -> BTreeMap<String, String> {
        let mut env = BTreeMap::new();
        let set = |env: &mut BTreeMap<String, String>, k: &str, v: &Path| {
            env.insert(k.to_string(), v.to_string_lossy().into_owned());
        };
        env.insert(
            "EBUILD_PHASE".to_string(),
            phase.func_name().replace("pkg_", "").replace("src_", ""),
        );
        env.insert(
            "EBUILD_PHASE_FUNC".to_string(),
            phase.func_name().to_string(),
        );
        set(&mut env, "EBUILD", &self.ebuild);
        env.insert("CATEGORY".to_string(), self.category());
        env.insert("PF".to_string(), self.pf());
        env.insert("EAPI".to_string(), self.eapi.clone());
        set(&mut env, "ROOT", &self.root);
        set(&mut env, "PORTAGE_BUILDDIR", &self.dirs.build_dir);
        set(&mut env, "WORKDIR", &self.dirs.work_dir);
        set(&mut env, "T", &self.dirs.temp_dir);
        set(&mut env, "D", &self.dirs.image_dir);
        set(&mut env, "FILESDIR", &self.dirs.files_dir);
        env.insert("USE".to_string(), self.use_flags.join(" "));
        env
    }

    fn category(&self) -> String {
        self.cpv.split('/').next().unwrap_or_default().to_string()
    }

    /// `PF`: the full package name-version (the part after `category/`).
    fn pf(&self) -> String {
        self.cpv
            .split_once('/')
            .map(|x| x.1)
            .unwrap_or(&self.cpv)
            .to_string()
    }
}

/// How a phase spawn finished.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhaseOutcome {
    pub phase: Phase,
    pub success: bool,
    pub message: Option<String>,
}

/// Injectable phase spawner. Production uses a bash adapter; tests use a fake.
pub trait PhaseSpawner {
    /// Runs one phase with the given controlled environment. Implementations
    /// MUST spawn with a structured argv and the provided env only.
    fn run_phase(&mut self, phase: Phase, env: &BTreeMap<String, String>) -> PhaseOutcome;
}

/// Runs the build phase sequence for `ctx`, stopping at the first failure.
/// Returns the outcomes of the phases that ran.
pub fn run_build_phases(ctx: &PhaseContext, spawner: &mut dyn PhaseSpawner) -> Vec<PhaseOutcome> {
    run_sequence(ctx, &build_phases(&ctx.eapi), spawner)
}

/// Runs the merge-time phase sequence (`pkg_preinst`, `pkg_postinst`).
pub fn run_merge_phases(ctx: &PhaseContext, spawner: &mut dyn PhaseSpawner) -> Vec<PhaseOutcome> {
    run_sequence(ctx, &merge_phases(), spawner)
}

fn run_sequence(
    ctx: &PhaseContext,
    phases: &[Phase],
    spawner: &mut dyn PhaseSpawner,
) -> Vec<PhaseOutcome> {
    let mut outcomes = Vec::new();
    for &phase in phases {
        let env = ctx.environment(phase);
        let outcome = spawner.run_phase(phase, &env);
        let failed = !outcome.success;
        outcomes.push(outcome);
        if failed {
            break;
        }
    }
    outcomes
}

/// Builds the structured argv for invoking `ebuild.sh` for a phase. This is the
/// only place the shell boundary is crossed, and it is a fixed argv vector:
/// `[ebuild_sh, phase_func]`. No untrusted value is ever concatenated into a
/// shell string.
pub fn phase_argv(ebuild_sh: &Path, phase: Phase) -> Vec<String> {
    vec![
        ebuild_sh.to_string_lossy().into_owned(),
        phase.func_name().to_string(),
    ]
}

/// A minimal, explicit base environment for spawning ebuild phases: just a
/// conservative `PATH`. Shared by the process spawners so the env-construction
/// boundary is defined in one place.
pub fn minimal_base_env() -> std::collections::BTreeMap<String, String> {
    let mut env = std::collections::BTreeMap::new();
    env.insert(
        "PATH".to_string(),
        "/usr/local/bin:/usr/bin:/bin".to_string(),
    );
    env
}
