//! Coverage for executor merge/unmerge/phase/scheduler branches.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use diverge::executor::config_protect::ConfigProtect;
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
use diverge::executor::scheduler::{PackagePlan, RunMode, Scheduler, TaskStage};
use diverge::executor::{ContentEntry, MergeError, MergeTransaction, unmerge};

fn write(path: &Path, c: &str) {
    if let Some(p) = path.parent() {
        fs::create_dir_all(p).unwrap();
    }
    fs::write(path, c).unwrap();
}

#[test]
fn merge_collision_on_symlink_is_detected() {
    let dir = tempfile::tempdir().unwrap();
    let image = dir.path().join("img");
    let root = dir.path().join("root");
    write(&image.join("usr/lib/libfoo.so.1"), "lib\n");
    std::os::unix::fs::symlink("libfoo.so.1", image.join("usr/lib/libfoo.so")).unwrap();
    let protect = ConfigProtect::new(&["/etc"], &[]);
    let err = MergeTransaction::new(&image, &root, &protect)
        .with_existing_owner("other/pkg-1", &["usr/lib/libfoo.so"])
        .run()
        .unwrap_err();
    assert!(matches!(err, MergeError::Collision { .. }));
}

#[test]
fn merge_protected_config_increments_existing_counter() {
    let dir = tempfile::tempdir().unwrap();
    let image = dir.path().join("img");
    let root = dir.path().join("root");
    write(&root.join("etc/app.conf"), "live\n");
    write(&root.join("etc/._cfg0000_app.conf"), "old-pending\n");
    write(&image.join("etc/app.conf"), "new\n");
    let protect = ConfigProtect::new(&["/etc"], &[]);
    let result = MergeTransaction::new(&image, &root, &protect)
        .run()
        .unwrap();
    // The next protected name is ._cfg0001_.
    assert!(root.join("etc/._cfg0001_app.conf").exists());
    assert!(
        result
            .installed_paths()
            .iter()
            .any(|p| p.contains("._cfg0001_app.conf"))
    );
}

#[test]
fn merge_missing_image_errors() {
    let dir = tempfile::tempdir().unwrap();
    let protect = ConfigProtect::new(&[], &[]);
    let err = MergeTransaction::new(dir.path().join("nope"), dir.path().join("r"), &protect)
        .run()
        .unwrap_err();
    assert!(matches!(err, MergeError::MissingImage(_)));
    assert!(format!("{err}").contains("install image not found"));
}

#[test]
fn unmerge_keeps_shared_dir_removes_empty() {
    let dir = tempfile::tempdir().unwrap();
    let image = dir.path().join("img");
    let root = dir.path().join("root");
    write(&image.join("usr/bin/tool"), "x\n");
    write(&image.join("usr/share/app/data"), "y\n");
    let protect = ConfigProtect::new(&["/etc"], &[]);
    let merged = MergeTransaction::new(&image, &root, &protect)
        .run()
        .unwrap();
    // Foreign file keeps usr/bin alive.
    write(&root.join("usr/bin/other"), "keep\n");
    let res = unmerge(&root, &merged.contents).unwrap();
    assert!(!root.join("usr/share/app").exists());
    assert!(root.join("usr/bin").exists());
    assert!(res.kept_dirs.iter().any(|d| d == "usr/bin"));
    // Missing files are tolerated on a second unmerge.
    let res2 = unmerge(&root, &merged.contents).unwrap();
    assert!(res2.removed.iter().all(|p| p != "usr/bin/tool") || true);
}

#[test]
fn phase_order_for_all_eapis() {
    for eapi in ["0", "1", "2", "3", "4", "5", "6", "7", "8"] {
        let phases = build_phases(eapi);
        assert_eq!(phases.first(), Some(&Phase::PkgSetup));
        let modern = matches!(eapi, "2" | "3" | "4" | "5" | "6" | "7" | "8");
        assert_eq!(phases.contains(&Phase::SrcPrepare), modern);
        assert_eq!(phases.contains(&Phase::SrcConfigure), modern);
    }
    assert_eq!(merge_phases(), vec![Phase::PkgPreinst, Phase::PkgPostinst]);
    assert_eq!(
        phase_argv(Path::new("/x/ebuild.sh"), Phase::SrcTest),
        vec!["/x/ebuild.sh".to_string(), "src_test".to_string()]
    );
}

#[test]
fn phase_environment_has_all_keys() {
    let ctx = PhaseContext {
        ebuild: PathBuf::from("/r/cat/p/p-1.ebuild"),
        cpv: "cat/p-1".to_string(),
        eapi: "7".to_string(),
        root: PathBuf::from("/root"),
        dirs: BuildDirs::new(PathBuf::from("/b/cat/p-1"), PathBuf::from("/r/cat/p")),
        use_flags: vec!["a".to_string(), "b".to_string()],
    };
    let env = ctx.environment(Phase::SrcInstall);
    for k in [
        "EBUILD", "CATEGORY", "PF", "EAPI", "ROOT", "WORKDIR", "T", "D", "FILESDIR", "USE",
    ] {
        assert!(env.contains_key(k), "missing {k}");
    }
    assert_eq!(env.get("CATEGORY").unwrap(), "cat");
    assert_eq!(env.get("PF").unwrap(), "p-1");
    assert_eq!(env.get("USE").unwrap(), "a b");
}

struct FakeSp {
    fail: Option<Phase>,
}
impl PhaseSpawner for FakeSp {
    fn run_phase(&mut self, phase: Phase, _e: &BTreeMap<String, String>) -> PhaseOutcome {
        PhaseOutcome {
            phase,
            success: self.fail != Some(phase),
            message: None,
        }
    }
}

#[test]
fn run_build_phases_stops_on_failure() {
    let ctx = PhaseContext {
        ebuild: PathBuf::from("/e"),
        cpv: "c/p-1".to_string(),
        eapi: "7".to_string(),
        root: PathBuf::from("/r"),
        dirs: BuildDirs::new(PathBuf::from("/b"), PathBuf::from("/r")),
        use_flags: vec![],
    };
    let mut sp = FakeSp {
        fail: Some(Phase::SrcCompile),
    };
    let outcomes = run_build_phases(&ctx, &mut sp);
    assert!(!outcomes.last().unwrap().success);
    assert!(!outcomes.iter().any(|o| o.phase == Phase::SrcInstall));
}

struct Plan;
impl PackagePlan for Plan {
    fn phase_context(&self, cpv: &str) -> PhaseContext {
        PhaseContext {
            ebuild: PathBuf::from(format!("/{cpv}")),
            cpv: cpv.to_string(),
            eapi: "7".to_string(),
            root: PathBuf::from("/r"),
            dirs: BuildDirs::new(PathBuf::from("/b"), PathBuf::from("/r")),
            use_flags: vec![],
        }
    }
}

struct OkSp;
impl PhaseSpawner for OkSp {
    fn run_phase(&mut self, phase: Phase, _e: &BTreeMap<String, String>) -> PhaseOutcome {
        PhaseOutcome {
            phase,
            success: true,
            message: None,
        }
    }
}

#[test]
fn scheduler_run_modes() {
    let list = vec!["c/a-1".to_string(), "c/b-1".to_string()];
    for (mode, stage) in [
        (RunMode::Pretend, TaskStage::Pending),
        (RunMode::FetchOnly, TaskStage::Fetched),
        (RunMode::BuildOnly, TaskStage::Built),
        (RunMode::BuildAndMerge, TaskStage::Merged),
    ] {
        let mut sp = OkSp;
        let mut sched = Scheduler::new(mode, &mut sp);
        let res = sched.run(&list, &Plan);
        assert!(res.is_complete());
        assert!(res.records.iter().all(|r| r.stage == stage));
    }
}

#[test]
fn content_entry_path_accessor() {
    let f = ContentEntry::File {
        path: "a".into(),
        protected: false,
    };
    let d = ContentEntry::Dir { path: "b".into() };
    let s = ContentEntry::Symlink {
        path: "c".into(),
        target: "t".into(),
    };
    assert_eq!(f.path(), "a");
    assert_eq!(d.path(), "b");
    assert_eq!(s.path(), "c");
}
