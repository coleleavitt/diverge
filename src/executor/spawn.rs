//! Real process spawner for ebuild phases.
//!
//! [`CommandSpawner`] implements [`super::phase::PhaseSpawner`] by spawning an
//! `ebuild.sh`-style script through [`std::process::Command`] with a fixed argv
//! (`[script, phase_func]`) and an explicit, fully-controlled environment.
//! Nothing is ever concatenated into a shell string — the script path and the
//! phase function name are passed as distinct argv entries, and the environment
//! is exactly the phase variables plus a minimal allowlist.
//!
//! Reference:
//! - `research/portage/lib/_emerge/EbuildProcess.py`, `AbstractEbuildProcess.py`
//! - `research/portage/lib/portage/tests/ebuild/test_spawn.py`

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::phase::{Phase, PhaseOutcome, PhaseSpawner, phase_argv};

/// Spawns ebuild phases by executing a script with a structured argv and the
/// phase environment. The script receives the phase function name as its only
/// argument.
#[derive(Debug, Clone)]
pub struct CommandSpawner {
    /// Path to the `ebuild.sh`-style entry script.
    ebuild_sh: PathBuf,
    /// Extra environment variables always present (e.g. `PATH`), kept minimal.
    base_env: BTreeMap<String, String>,
}

impl CommandSpawner {
    /// Creates a spawner for the given entry script. The base environment
    /// contains only a conservative `PATH` unless extended via
    /// [`Self::with_base_env`].
    pub fn new(ebuild_sh: impl Into<PathBuf>) -> Self {
        let mut base_env = BTreeMap::new();
        base_env.insert(
            "PATH".to_string(),
            "/usr/local/bin:/usr/bin:/bin".to_string(),
        );
        Self {
            ebuild_sh: ebuild_sh.into(),
            base_env,
        }
    }

    /// Adds or overrides a base environment variable.
    pub fn with_base_env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.base_env.insert(key.into(), value.into());
        self
    }

    /// The argv that would be used for `phase` (exposed for inspection/tests).
    pub fn argv(&self, phase: Phase) -> Vec<String> {
        phase_argv(&self.ebuild_sh, phase)
    }

    /// Spawns `phase`, returning the captured stdout/stderr on completion.
    /// This is the lower-level entry point [`PhaseSpawner::run_phase`] wraps.
    pub fn spawn_capture(
        &self,
        phase: Phase,
        env: &BTreeMap<String, String>,
    ) -> std::io::Result<SpawnResult> {
        let argv = self.argv(phase);
        let program = &argv[0];
        let args = &argv[1..];

        let output = Command::new(program)
            .args(args)
            .env_clear()
            .envs(&self.base_env)
            .envs(env)
            .output()?;

        Ok(SpawnResult {
            success: output.status.success(),
            code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }
}

/// The captured result of a real phase spawn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpawnResult {
    pub success: bool,
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

impl PhaseSpawner for CommandSpawner {
    fn run_phase(&mut self, phase: Phase, env: &BTreeMap<String, String>) -> PhaseOutcome {
        match self.spawn_capture(phase, env) {
            Ok(result) => PhaseOutcome {
                phase,
                success: result.success,
                message: if result.success {
                    None
                } else {
                    Some(format!(
                        "phase {} failed (code {:?}): {}",
                        phase.func_name(),
                        result.code,
                        result.stderr.trim()
                    ))
                },
            },
            Err(err) => PhaseOutcome {
                phase,
                success: false,
                message: Some(format!("failed to spawn {}: {err}", phase.func_name())),
            },
        }
    }
}

/// Returns true when `path` is an executable file (used to validate an ebuild
/// entry script before spawning).
pub fn is_executable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path)
            .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        path.is_file()
    }
}
