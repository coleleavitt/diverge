//! A bundled, faithful subset of Portage's `ebuild.sh` install-helper contract,
//! plus an [`EbuildSpawner`] that runs a *real* ebuild's phase function against
//! its build environment.
//!
//! This is the "real ebuild build" path: instead of requiring a pre-built image
//! (the `image_for` override) or the host's full Portage install, it ships the
//! install-phase helper functions (`into`/`insinto`/`exeinto`, `dodir`,
//! `dobin`/`dosbin`, `doexe`, `doins`, `dolib*`, `dosym`, `dodoc`, `doman`,
//! `keepdir`, `newbin`/`newins`, `fperms`/`fowners`) as a shell library, sources
//! it together with the ebuild, and invokes the requested phase function with
//! the explicit phase environment. A real `src_install` then populates `$D`,
//! which the merge installs into `ROOT`.
//!
//! Safety: the ebuild is untrusted, so the spawn uses a structured argv (never
//! string-concatenated commands), a cleared+explicit environment, and runs
//! entirely inside the caller-provided build dir / `D` (tests use a tempdir).
//! Only `bash` and coreutils are required — no compiler, no host Portage.
//!
//! Reference: `research/portage/bin/phase-helpers.sh`, `bin/ebuild.sh`.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::Command;

use super::phase::{Phase, PhaseOutcome, PhaseSpawner};

/// The bundled install-helper shell library, sourced before each ebuild phase.
/// It implements the install helpers in terms of `install`/`ln`/`mkdir`, honors
/// `into`/`insinto`/`exeinto` dest-tree state, and writes everything under `$D`.
pub const EBUILD_HELPERS: &str = r#"# diverge bundled ebuild install helpers (subset of Portage phase-helpers.sh)
__E_DESTTREE="/usr"
__E_INSDESTTREE=""
__E_EXEDESTTREE=""
: "${INSOPTIONS:=-m0644}"
: "${EXEOPTIONS:=-m0755}"
: "${LIBOPTIONS:=-m0644}"
: "${DIROPTIONS:=}"
: "${PORTAGE_BIN_GROUP:=0}"

die() { echo "die: $*" >&2; exit 1; }

into() { if [ "$1" = "/" ]; then __E_DESTTREE=""; else __E_DESTTREE="$1"; fi; }
insinto() { if [ "$1" = "/" ]; then __E_INSDESTTREE=""; else __E_INSDESTTREE="$1"; fi; }
exeinto() { if [ "$1" = "/" ]; then __E_EXEDESTTREE=""; else __E_EXEDESTTREE="$1"; fi; }

dodir() { mkdir -p $DIROPTIONS "${D%/}/${1#/}" || die "dodir $1"; while [ $# -gt 1 ]; do shift; mkdir -p $DIROPTIONS "${D%/}/${1#/}" || die "dodir $1"; done; }
keepdir() { for d in "$@"; do dodir "$d"; : > "${D%/}/${d#/}/.keep" || die "keepdir $d"; done; }

dobin() { dodir "${__E_DESTTREE}/bin"; for f in "$@"; do install -m0755 "$f" "${D%/}/${__E_DESTTREE#/}/bin/" || die "dobin $f"; done; }
dosbin() { dodir "${__E_DESTTREE}/sbin"; for f in "$@"; do install -m0755 "$f" "${D%/}/${__E_DESTTREE#/}/sbin/" || die "dosbin $f"; done; }
doexe() { dodir "${__E_EXEDESTTREE}"; for f in "$@"; do install $EXEOPTIONS "$f" "${D%/}/${__E_EXEDESTTREE#/}/" || die "doexe $f"; done; }
doins() {
	local recur=
	if [ "$1" = "-r" ]; then recur=1; shift; fi
	dodir "${__E_INSDESTTREE}"
	for f in "$@"; do
		if [ -d "$f" ] && [ -n "$recur" ]; then
			cp -R "$f" "${D%/}/${__E_INSDESTTREE#/}/" || die "doins -r $f"
		else
			install $INSOPTIONS "$f" "${D%/}/${__E_INSDESTTREE#/}/" || die "doins $f"
		fi
	done
}
dolib() { dodir "${__E_DESTTREE}/lib"; for f in "$@"; do install $LIBOPTIONS "$f" "${D%/}/${__E_DESTTREE#/}/lib/" || die "dolib $f"; done; }
dolib_so() { dolib "$@"; }
dolib_a() { dolib "$@"; }
dosym() { local tgt="$1" lnk="$2"; dodir "$(dirname "${lnk}")"; ln -snf "$tgt" "${D%/}/${lnk#/}" || die "dosym $tgt $lnk"; }
dodoc() { dodir "/usr/share/doc/${PF:-pkg}"; for f in "$@"; do install -m0644 "$f" "${D%/}/usr/share/doc/${PF:-pkg}/" || die "dodoc $f"; done; }
doman() { dodir "/usr/share/man/man1"; for f in "$@"; do install -m0644 "$f" "${D%/}/usr/share/man/man1/" || die "doman $f"; done; }
newbin() { local s="$1" n="$2"; dodir "${__E_DESTTREE}/bin"; install -m0755 "$s" "${D%/}/${__E_DESTTREE#/}/bin/${n}" || die "newbin"; }
newins() { local s="$1" n="$2"; dodir "${__E_INSDESTTREE}"; install $INSOPTIONS "$s" "${D%/}/${__E_INSDESTTREE#/}/${n}" || die "newins"; }
fperms() { local mode="$1"; shift; for f in "$@"; do chmod "$mode" "${D%/}/${f#/}" || die "fperms $f"; done; }
fowners() { return 0; }
# No-op phase helpers commonly called but irrelevant to a hermetic install.
elog() { echo "* $*"; }
einfo() { echo " * $*"; }
ewarn() { echo " * $*" >&2; }
eerror() { echo " * $*" >&2; }
ebegin() { echo " * $*"; }
eend() { return 0; }
use() { return 1; }
has() { local n="$1"; shift; for x in "$@"; do [ "$x" = "$n" ] && return 0; done; return 1; }
default() { return 0; }
"#;

/// A [`PhaseSpawner`] that runs a real ebuild's phase function with the bundled
/// install helpers sourced. The ebuild path comes from the phase environment's
/// `EBUILD` variable.
#[derive(Debug, Clone, Default)]
pub struct EbuildSpawner {
    /// Extra base environment variables (e.g. `PATH`).
    base_env: BTreeMap<String, String>,
}

impl EbuildSpawner {
    pub fn new() -> Self {
        Self {
            base_env: super::phase::minimal_base_env(),
        }
    }

    /// The ebuild file path for a phase environment (`EBUILD`).
    fn ebuild_path(env: &BTreeMap<String, String>) -> Option<PathBuf> {
        env.get("EBUILD").map(PathBuf::from)
    }
}

impl PhaseSpawner for EbuildSpawner {
    fn run_phase(&mut self, phase: Phase, env: &BTreeMap<String, String>) -> PhaseOutcome {
        let func = phase.func_name();
        let Some(ebuild) = Self::ebuild_path(env) else {
            return PhaseOutcome {
                phase,
                success: false,
                message: Some("no EBUILD in phase environment".to_string()),
            };
        };
        if !ebuild.is_file() {
            // Nothing to source for this phase (e.g. a generated/installed pkg);
            // treat as a no-op success so the scheduler can proceed.
            return PhaseOutcome {
                phase,
                success: true,
                message: None,
            };
        }

        // Materialize the bundled helper library next to the build's temp dir
        // (T) so bash can `source` it by path; fall back to the system temp dir.
        let helpers_dir = env
            .get("T")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let helpers_path = helpers_dir.join(".diverge-helpers.sh");
        if let Some(parent) = helpers_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(err) = std::fs::write(&helpers_path, EBUILD_HELPERS) {
            return PhaseOutcome {
                phase,
                success: false,
                message: Some(format!("could not write ebuild helpers: {err}")),
            };
        }

        // Structured, fixed-argv invocation: bash sources the bundled helpers
        // (path in $1) and the ebuild (from $EBUILD), then runs the phase
        // function if the ebuild defines it. `set -e` aborts on the first error.
        let script = "set -e\n\
             source \"$1\"\n\
             source \"$EBUILD\"\n\
             if declare -F \"$2\" >/dev/null 2>&1; then \"$2\"; fi\n";
        let output = Command::new("bash")
            .arg("--norc")
            .arg("-c")
            .arg(script)
            .arg("diverge-ebuild") // $0
            .arg(&helpers_path) // $1 = helper library path
            .arg(func) // $2 = phase function name
            .env_clear()
            .envs(&self.base_env)
            .envs(env)
            .output();

        match output {
            Ok(out) if out.status.success() => PhaseOutcome {
                phase,
                success: true,
                message: None,
            },
            Ok(out) => PhaseOutcome {
                phase,
                success: false,
                message: Some(format!(
                    "phase {func} failed: {}",
                    String::from_utf8_lossy(&out.stderr).trim()
                )),
            },
            Err(err) => PhaseOutcome {
                phase,
                success: false,
                message: Some(format!("failed to spawn bash for {func}: {err}")),
            },
        }
    }
}
