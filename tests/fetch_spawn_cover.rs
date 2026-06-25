//! Coverage for the fetch and spawn executor modules.
//!
//! Exercises the Fetcher/LocalFetcher flow, resume/skip, multi-URI fallback,
//! every FetchError variant, and the CommandSpawner spawn paths.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use diverge::executor::fetch::{FetchError, Fetcher, LocalFetcher, Source, fetch_all, fetch_one};
use diverge::executor::phase::{BuildDirs, Phase, PhaseContext, PhaseSpawner};
use diverge::executor::spawn::{CommandSpawner, is_executable};
use diverge::manifest::{Manifest, checksum_str};

fn write(path: &Path, content: &[u8]) {
    if let Some(p) = path.parent() {
        fs::create_dir_all(p).unwrap();
    }
    fs::write(path, content).unwrap();
}

fn manifest_for(name: &str, data: &[u8]) -> Manifest {
    let line = format!(
        "DIST {name} {} BLAKE2B {} SHA512 {}",
        data.len(),
        checksum_str(data, "BLAKE2B").unwrap(),
        checksum_str(data, "SHA512").unwrap()
    );
    Manifest::parse(&line).unwrap()
}

#[test]
fn local_fetcher_reads_file_and_strips_scheme() {
    let dir = tempfile::tempdir().unwrap();
    let f = dir.path().join("d.tar");
    write(&f, b"data");
    let mut fetcher = LocalFetcher;
    // bare path and file:// both work.
    assert_eq!(
        fetcher.retrieve(f.to_str().unwrap()).unwrap(),
        Some(b"data".to_vec())
    );
    assert_eq!(
        fetcher
            .retrieve(&format!("file://{}", f.display()))
            .unwrap(),
        Some(b"data".to_vec())
    );
    // missing -> Ok(None).
    assert_eq!(fetcher.retrieve("/no/such/file/xyz").unwrap(), None);
}

#[test]
fn fetch_one_tries_each_uri_then_writes() {
    let dir = tempfile::tempdir().unwrap();
    let mirror = dir.path().join("m/foo.tar");
    let data = b"the real payload";
    write(&mirror, data);
    let distdir = dir.path().join("dist");
    let manifest = manifest_for("foo.tar", data);
    let source = Source {
        filename: "foo.tar".to_string(),
        uris: vec![
            "file:///missing/one".to_string(),
            format!("file://{}", mirror.display()),
        ],
    };
    let mut fetcher = LocalFetcher;
    let r = fetch_one(&source, &distdir, &manifest, &mut fetcher).unwrap();
    assert!(!r.already_present);
    assert_eq!(fs::read(&r.path).unwrap(), data);
}

#[test]
fn fetch_one_resumes_present_verified() {
    let dir = tempfile::tempdir().unwrap();
    let distdir = dir.path().join("dist");
    let data = b"already";
    write(&distdir.join("foo.tar"), data);
    let manifest = manifest_for("foo.tar", data);
    let source = Source {
        filename: "foo.tar".to_string(),
        uris: vec!["file:///irrelevant".to_string()],
    };
    let mut fetcher = LocalFetcher;
    let r = fetch_one(&source, &distdir, &manifest, &mut fetcher).unwrap();
    assert!(r.already_present);
}

#[test]
fn fetch_one_rejects_bad_checksum() {
    let dir = tempfile::tempdir().unwrap();
    let mirror = dir.path().join("m/foo.tar");
    write(&mirror, b"corrupt");
    let distdir = dir.path().join("dist");
    let manifest = manifest_for("foo.tar", b"the real one");
    let source = Source {
        filename: "foo.tar".to_string(),
        uris: vec![format!("file://{}", mirror.display())],
    };
    let mut fetcher = LocalFetcher;
    let err = fetch_one(&source, &distdir, &manifest, &mut fetcher).unwrap_err();
    assert!(matches!(err, FetchError::Verification(_)));
    assert!(!distdir.join("foo.tar").exists());
}

struct Dead;
impl Fetcher for Dead {
    fn retrieve(&mut self, _u: &str) -> Result<Option<Vec<u8>>, FetchError> {
        Ok(None)
    }
}

#[test]
fn fetch_all_propagates_first_failure() {
    let dir = tempfile::tempdir().unwrap();
    let ok = dir.path().join("a.tar");
    write(&ok, b"a");
    let m_ok = manifest_for("a.tar", b"a");
    // a.tar present+verified, b.tar all-fail -> fetch_all errors on b.
    let distdir = dir.path().join("dist");
    write(&distdir.join("a.tar"), b"a");
    let sources = vec![
        Source {
            filename: "a.tar".to_string(),
            uris: vec![format!("file://{}", ok.display())],
        },
        Source {
            filename: "b.tar".to_string(),
            uris: vec!["http://x/b".to_string()],
        },
    ];
    // Use a manifest that knows a.tar (b will fail before verify).
    let mut fetcher = Dead;
    let err = fetch_all(&sources, &distdir, &m_ok, &mut fetcher).unwrap_err();
    assert!(matches!(err, FetchError::AllSourcesFailed { .. }));
}

#[test]
fn fetch_error_display_strings() {
    let e = FetchError::AllSourcesFailed {
        filename: "f".into(),
        tried: vec!["u".into()],
    };
    assert!(format!("{e}").contains("all sources failed"));
    let e = FetchError::Io("boom".into());
    assert!(format!("{e}").contains("boom"));
}

#[cfg(unix)]
fn write_exec(path: &Path, body: &str) {
    use std::os::unix::fs::PermissionsExt;
    write(path, body.as_bytes());
    let mut perm = fs::metadata(path).unwrap().permissions();
    perm.set_mode(0o755);
    fs::set_permissions(path, perm).unwrap();
}

fn ctx(image: &Path) -> PhaseContext {
    PhaseContext {
        ebuild: image.join("a.ebuild"),
        cpv: "app/a-1".to_string(),
        eapi: "7".to_string(),
        root: image.join("root"),
        dirs: BuildDirs::new(image.to_path_buf(), image),
        use_flags: vec!["x".to_string()],
    }
}

#[cfg(unix)]
#[test]
fn command_spawner_runs_and_captures() {
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("ebuild.sh");
    let out = dir.path().join("o");
    write_exec(
        &script,
        &format!("#!/bin/sh\necho \"$1 $USE\" > {}\n", out.display()),
    );
    let sp = CommandSpawner::new(&script).with_base_env("FOO", "bar");
    let c = ctx(dir.path());
    let env = c.environment(Phase::SrcInstall);
    let res = sp.spawn_capture(Phase::SrcInstall, &env).unwrap();
    assert!(res.success);
    assert!(fs::read_to_string(&out).unwrap().contains("src_install x"));
    // run_phase wrapper returns success outcome.
    let mut sp2 = CommandSpawner::new(&script);
    assert!(sp2.run_phase(Phase::SrcCompile, &env).success);
}

#[cfg(unix)]
#[test]
fn command_spawner_failure_outcome_has_message() {
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("ebuild.sh");
    write_exec(&script, "#!/bin/sh\necho err >&2\nexit 3\n");
    let mut sp = CommandSpawner::new(&script);
    let c = ctx(dir.path());
    let env = c.environment(Phase::SrcCompile);
    let outcome = sp.run_phase(Phase::SrcCompile, &env);
    assert!(!outcome.success);
    assert!(outcome.message.unwrap().contains("err"));
}

#[test]
fn spawner_argv_and_is_executable() {
    let sp = CommandSpawner::new("/bin/ebuild.sh");
    let argv = sp.argv(Phase::PkgSetup);
    assert_eq!(
        argv,
        vec!["/bin/ebuild.sh".to_string(), "pkg_setup".to_string()]
    );
    // A non-existent path is not executable.
    assert!(!is_executable(Path::new("/no/such/bin/xyz")));
}

#[cfg(unix)]
#[test]
fn is_executable_true_for_exec_file() {
    let dir = tempfile::tempdir().unwrap();
    let f = dir.path().join("x.sh");
    write_exec(&f, "#!/bin/sh\n");
    assert!(is_executable(&f));
    // A plain file is not executable.
    let p = dir.path().join("plain");
    write(&p, b"hi");
    assert!(!is_executable(&p));
}

#[cfg(unix)]
#[test]
fn spawn_missing_program_is_failure() {
    let mut sp = CommandSpawner::new("/definitely/not/here/ebuild.sh");
    let dir = tempfile::tempdir().unwrap();
    let c = ctx(dir.path());
    let env: BTreeMap<String, String> = c.environment(Phase::PkgSetup);
    let outcome = sp.run_phase(Phase::PkgSetup, &env);
    assert!(!outcome.success);
    assert!(outcome.message.unwrap().contains("failed to spawn"));
}
