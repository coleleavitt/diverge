//! Capstone integration: a single fixture exercises the advanced resolver
//! features (virtuals, slot-operator rebuild, backtracking) and then drives the
//! resulting plan through the scheduler and a gpkg round-trip — proving the
//! resolver, executor, and adapter layers compose on the shared model.

use std::collections::BTreeMap;
use std::path::PathBuf;

use diverge::dbapi::PackageDb;
use diverge::depgraph::{ResolveParams, Resolver};
use diverge::executor::phase::{BuildDirs, Phase, PhaseContext, PhaseOutcome, PhaseSpawner};
use diverge::executor::scheduler::{PackagePlan, RunMode, Scheduler, TaskStage};
use diverge::gpkg::Gpkg;

use crate::resolver_fixture::{db, pkg, pkg_slot};

/// A spawner that always succeeds, recording the phases it ran.
#[derive(Default)]
struct OkSpawner {
    ran: Vec<(String, Phase)>,
}
impl PhaseSpawner for OkSpawner {
    fn run_phase(&mut self, phase: Phase, env: &BTreeMap<String, String>) -> PhaseOutcome {
        self.ran
            .push((env.get("PF").cloned().unwrap_or_default(), phase));
        PhaseOutcome {
            phase,
            success: true,
            message: None,
        }
    }
}

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

#[test]
fn resolve_advanced_then_schedule_and_package() {
    // A fixture combining a virtual provider choice and a slot-operator rebuild.
    //
    // - app-misc/app depends on virtual/db and sys-libs/lib:= .
    // - virtual/db -> || ( sys-libs/sqlite sys-libs/bdb ).
    // - sys-libs/lib upgrades sub-slot 0/1 -> 0/2, forcing the installed
    //   app-misc/consumer (built against lib:0/1=) to rebuild.
    let available = db(&[
        (
            "app-misc/app-1",
            pkg(&[("RDEPEND", "virtual/db sys-libs/lib:=")]),
        ),
        (
            "virtual/db-1",
            pkg(&[("RDEPEND", "|| ( sys-libs/sqlite sys-libs/bdb )")]),
        ),
        ("sys-libs/sqlite-3", pkg(&[])),
        ("sys-libs/bdb-6", pkg(&[])),
        ("sys-libs/lib-1", pkg_slot("0/1", &[])),
        ("sys-libs/lib-2", pkg_slot("0/2", &[])),
        (
            "app-misc/consumer-1",
            pkg_slot("0", &[("RDEPEND", "sys-libs/lib:=")]),
        ),
    ]);
    let mut installed = PackageDb::new();
    installed.insert("sys-libs/lib-1", pkg_slot("0/1", &[]));
    installed.insert(
        "app-misc/consumer-1",
        pkg_slot("0", &[("RDEPEND", "sys-libs/lib:0/1=")]),
    );

    // --update --deep so the installed sys-libs/lib-1 upgrades to lib-2,
    // which (via the slot-operator dep) forces the consumer rebuild.
    let params = ResolveParams::default().with_update(true).with_deep(true);
    let resolver = Resolver::new(&available, &installed, params);
    let outcome = resolver.resolve(&["app-misc/app"]);
    assert!(outcome.is_success(), "resolve failed: {:?}", outcome.error);

    // Virtual provider (first branch, sqlite) and the app are present.
    assert!(outcome.mergelist.contains(&"virtual/db-1".to_string()));
    assert!(outcome.mergelist.contains(&"sys-libs/sqlite-3".to_string()));
    assert!(!outcome.mergelist.contains(&"sys-libs/bdb-6".to_string()));
    assert!(outcome.mergelist.contains(&"app-misc/app-1".to_string()));
    // lib upgrades to 0/2 and forces the consumer rebuild.
    assert!(outcome.mergelist.contains(&"sys-libs/lib-2".to_string()));
    assert!(
        outcome
            .mergelist
            .contains(&"app-misc/consumer-1".to_string())
    );

    // Dependencies precede dependents in the plan.
    let pos = |c: &str| outcome.mergelist.iter().position(|x| x == c).unwrap();
    assert!(pos("sys-libs/sqlite-3") < pos("virtual/db-1"));
    assert!(pos("virtual/db-1") < pos("app-misc/app-1"));
    assert!(pos("sys-libs/lib-2") < pos("app-misc/consumer-1"));

    // Drive the resolved plan through the scheduler (full build + merge).
    let mut spawner = OkSpawner::default();
    let mut scheduler = Scheduler::new(RunMode::BuildAndMerge, &mut spawner);
    let plan = FixturePlan;
    let schedule = scheduler.run(&outcome.mergelist, &plan);
    assert!(schedule.is_complete(), "schedule failed: {schedule:?}");
    assert!(
        schedule
            .records
            .iter()
            .all(|r| r.stage == TaskStage::Merged)
    );
    assert_eq!(schedule.records.len(), outcome.mergelist.len());

    // Package one of the merged packages into a gpkg and read it back.
    let mut meta = BTreeMap::new();
    meta.insert("SLOT".to_string(), b"0".to_vec());
    meta.insert("repository".to_string(), b"test_repo".to_vec());
    let binpkg = Gpkg::new(meta, b"app-1 install image".to_vec());
    let decoded = Gpkg::decode(&binpkg.encode()).expect("gpkg round-trip");
    assert_eq!(decoded.metadata_str("SLOT").as_deref(), Some("0"));
    assert_eq!(decoded.image, b"app-1 install image");
}
