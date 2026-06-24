//! Tests for the real process spawner and the distfile fetch loop.
//!
//! Reference:
//! - `research/portage/lib/portage/tests/ebuild/test_spawn.py`
//! - `research/portage/lib/portage/tests/ebuild/test_fetch.py`

use std::fs;
use std::path::{Path, PathBuf};

use diverge::executor::fetch::{FetchError, Fetcher, LocalFetcher, Source, fetch_one};
use diverge::executor::phase::{BuildDirs, Phase, PhaseContext, PhaseSpawner};
use diverge::executor::spawn::CommandSpawner;
use diverge::manifest::{Manifest, checksum_str};

use crate::fs_fixture::write;

#[cfg(unix)]
fn write_executable(path: &Path, content: &str) {
    use std::os::unix::fs::PermissionsExt;
    write(path, content);
    let mut perms = fs::metadata(path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms).unwrap();
}

fn phase_ctx(ebuild: PathBuf, image: PathBuf) -> PhaseContext {
    let build = image.parent().unwrap().to_path_buf();
    PhaseContext {
        ebuild,
        cpv: "dev-libs/A-1".to_string(),
        eapi: "7".to_string(),
        root: PathBuf::from("/test-root"),
        dirs: BuildDirs::new(build, PathBuf::from("/repo/dev-libs/A")),
        use_flags: vec!["foo".to_string()],
    }
}

#[cfg(unix)]
#[test]
fn command_spawner_runs_script_with_structured_env() {
    let dir = tempfile::tempdir().expect("tempdir");
    // A fake ebuild.sh that records its phase arg + a phase var to a file.
    let script = dir.path().join("ebuild.sh");
    let out = dir.path().join("out.txt");
    write_executable(
        &script,
        &format!(
            "#!/bin/sh\necho \"phase=$1 D=$D USE=$USE\" > {}\nexit 0\n",
            out.display()
        ),
    );

    let mut spawner = CommandSpawner::new(&script);
    let ctx = phase_ctx(dir.path().join("A-1.ebuild"), dir.path().join("image"));
    let env = ctx.environment(Phase::SrcInstall);
    let outcome = spawner.run_phase(Phase::SrcInstall, &env);

    assert!(outcome.success, "{:?}", outcome.message);
    let recorded = fs::read_to_string(&out).expect("script wrote output");
    assert!(recorded.contains("phase=src_install"), "{recorded}");
    assert!(recorded.contains("USE=foo"), "{recorded}");
    assert!(
        recorded.contains("/image"),
        "D should be the image dir: {recorded}"
    );
}

#[cfg(unix)]
#[test]
fn command_spawner_reports_failure_with_stderr() {
    let dir = tempfile::tempdir().expect("tempdir");
    let script = dir.path().join("ebuild.sh");
    write_executable(&script, "#!/bin/sh\necho boom >&2\nexit 1\n");

    let mut spawner = CommandSpawner::new(&script);
    let ctx = phase_ctx(dir.path().join("A-1.ebuild"), dir.path().join("image"));
    let env = ctx.environment(Phase::SrcCompile);
    let outcome = spawner.run_phase(Phase::SrcCompile, &env);

    assert!(!outcome.success);
    let msg = outcome.message.unwrap();
    assert!(msg.contains("boom"), "stderr surfaced: {msg}");
}

#[test]
fn command_spawner_argv_is_fixed_two_elements() {
    let spawner = CommandSpawner::new("/usr/lib/portage/bin/ebuild.sh");
    assert_eq!(
        spawner.argv(Phase::SrcUnpack),
        vec![
            "/usr/lib/portage/bin/ebuild.sh".to_string(),
            "src_unpack".to_string()
        ]
    );
}

/// Builds a single-DIST Manifest for `data` named `file`.
fn manifest_for(file: &str, data: &[u8]) -> Manifest {
    let line = format!(
        "DIST {file} {} BLAKE2B {} SHA512 {}",
        data.len(),
        checksum_str(data, "BLAKE2B").unwrap(),
        checksum_str(data, "SHA512").unwrap()
    );
    Manifest::parse(&line).unwrap()
}

#[test]
fn fetch_retrieves_and_verifies_from_local_mirror() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mirror = dir.path().join("mirror");
    let distdir = dir.path().join("distdir");
    let data = b"distfile contents v1";
    write(
        &mirror.join("foo-1.tar.gz"),
        std::str::from_utf8(data).unwrap(),
    );

    let manifest = manifest_for("foo-1.tar.gz", data);
    let source = Source {
        filename: "foo-1.tar.gz".to_string(),
        uris: vec![
            // First URI is missing; second is the real local mirror.
            format!(
                "file://{}",
                dir.path().join("absent/foo-1.tar.gz").display()
            ),
            format!("file://{}", mirror.join("foo-1.tar.gz").display()),
        ],
    };

    let mut fetcher = LocalFetcher;
    let result = fetch_one(&source, &distdir, &manifest, &mut fetcher).expect("fetch");
    assert!(!result.already_present);
    assert!(distdir.join("foo-1.tar.gz").exists());
    assert_eq!(fs::read(&result.path).unwrap(), data);
}

#[test]
fn fetch_skips_present_verified_distfile() {
    let dir = tempfile::tempdir().expect("tempdir");
    let distdir = dir.path().join("distdir");
    let data = b"already here";
    write(
        &distdir.join("foo-1.tar.gz"),
        std::str::from_utf8(data).unwrap(),
    );

    let manifest = manifest_for("foo-1.tar.gz", data);
    let source = Source {
        filename: "foo-1.tar.gz".to_string(),
        uris: vec!["file:///nonexistent".to_string()],
    };
    let mut fetcher = LocalFetcher;
    let result = fetch_one(&source, &distdir, &manifest, &mut fetcher).expect("fetch");
    assert!(result.already_present, "present + verified -> skip fetch");
}

#[test]
fn fetch_rejects_corrupt_distfile() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mirror = dir.path().join("mirror");
    let distdir = dir.path().join("distdir");
    write(&mirror.join("foo-1.tar.gz"), "corrupt bytes");

    // Manifest expects different content.
    let manifest = manifest_for("foo-1.tar.gz", b"the real contents");
    let source = Source {
        filename: "foo-1.tar.gz".to_string(),
        uris: vec![format!("file://{}", mirror.join("foo-1.tar.gz").display())],
    };
    let mut fetcher = LocalFetcher;
    let err = fetch_one(&source, &distdir, &manifest, &mut fetcher).expect_err("must reject");
    assert!(matches!(err, FetchError::Verification(_)), "got {err}");
    // The corrupt file is not committed to DISTDIR.
    assert!(!distdir.join("foo-1.tar.gz").exists());
}

/// A second Fetcher implementation: always reports the URI unavailable. Models
/// a network backend that cannot reach any mirror.
struct DeadFetcher;
impl Fetcher for DeadFetcher {
    fn retrieve(&mut self, _uri: &str) -> Result<Option<Vec<u8>>, FetchError> {
        Ok(None)
    }
}

#[test]
fn fetch_fails_when_all_sources_unavailable() {
    let dir = tempfile::tempdir().expect("tempdir");
    let manifest = manifest_for("foo-1.tar.gz", b"data");
    let source = Source {
        filename: "foo-1.tar.gz".to_string(),
        uris: vec!["http://a/foo".to_string(), "http://b/foo".to_string()],
    };
    let mut fetcher = DeadFetcher;
    let err = fetch_one(
        &source,
        &dir.path().join("distdir"),
        &manifest,
        &mut fetcher,
    )
    .expect_err("all fail");
    match err {
        FetchError::AllSourcesFailed { filename, tried } => {
            assert_eq!(filename, "foo-1.tar.gz");
            assert_eq!(tried.len(), 2);
        }
        other => panic!("expected AllSourcesFailed, got {other}"),
    }
}
