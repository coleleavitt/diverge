//! Ports of XPAK, global-update, and package-set behavior.
//!
//! Reference:
//! - `research/portage/lib/portage/tests/xpak/test_decodeint.py`
//! - `research/portage/lib/portage/tests/update/test_move_ent.py`,
//!   `test_update_dbentry.py`
//! - `research/portage/lib/portage/tests/sets/**`

use std::collections::BTreeMap;

use diverge::sets::{SetMember, SetRegistry, WorldFile};
use diverge::update::{UpdateCommand, parse_updates, update_dbentries, update_dbentry};
use diverge::xpak::{decodeint, encodeint, xpak_mem, xpak_parse};

#[test]
fn xpak_int_roundtrips_like_portage() {
    // test_decodeint.py: decodeint(encodeint(n)) == n for 0..1000 and 2^32-1.
    for n in 0..1000u32 {
        assert_eq!(decodeint(&encodeint(n)), Some(n));
    }
    let big = u32::MAX;
    assert_eq!(decodeint(&encodeint(big)), Some(big));
}

#[test]
fn xpak_segment_roundtrips() {
    let mut data = BTreeMap::new();
    data.insert("SLOT".to_string(), b"0".to_vec());
    data.insert("USE".to_string(), b"foo bar".to_vec());
    data.insert("DEPEND".to_string(), b"dev-libs/B".to_vec());

    let segment = xpak_mem(&data);
    assert!(segment.starts_with(b"XPAKPACK"));
    assert!(segment.ends_with(b"XPAKSTOP"));

    let parsed = xpak_parse(&segment).expect("parse xpak");
    assert_eq!(parsed, data);
}

#[test]
fn xpak_rejects_bad_magic() {
    assert!(xpak_parse(b"not an xpak segment at all!").is_err());
}

#[test]
fn parse_updates_reads_move_and_slotmove() {
    let content = "\
move dev-libs/A dev-libs/A-moved
slotmove dev-libs/B 0 1
";
    let commands = parse_updates(content).expect("parse updates");
    assert_eq!(
        commands,
        vec![
            UpdateCommand::Move {
                from: "dev-libs/A".to_string(),
                to: "dev-libs/A-moved".to_string()
            },
            UpdateCommand::SlotMove {
                cp: "dev-libs/B".to_string(),
                from_slot: "0".to_string(),
                to_slot: "1".to_string()
            },
        ]
    );
    assert!(parse_updates("bogus directive here").is_err());
}

#[test]
fn move_rewrites_matching_dependency_atoms() {
    let cmd = UpdateCommand::Move {
        from: "dev-libs/A".to_string(),
        to: "dev-libs/A-moved".to_string(),
    };
    // Plain dep.
    assert_eq!(update_dbentry(&cmd, "dev-libs/A"), "dev-libs/A-moved");
    // Versioned/operator atom keeps its operator + version.
    assert_eq!(
        update_dbentry(&cmd, ">=dev-libs/A-1.2"),
        ">=dev-libs/A-moved-1.2"
    );
    // Whitespace is preserved; only the matching token changes.
    assert_eq!(
        update_dbentry(&cmd, "dev-libs/A   dev-libs/C"),
        "dev-libs/A-moved   dev-libs/C"
    );
    // Non-matching cp untouched.
    assert_eq!(update_dbentry(&cmd, "dev-libs/AB"), "dev-libs/AB");
}

#[test]
fn slotmove_rewrites_explicit_slot() {
    let cmd = UpdateCommand::SlotMove {
        cp: "dev-libs/B".to_string(),
        from_slot: "0".to_string(),
        to_slot: "1".to_string(),
    };
    assert_eq!(update_dbentry(&cmd, "dev-libs/B:0"), "dev-libs/B:1");
    // No slot reference -> unchanged.
    assert_eq!(update_dbentry(&cmd, "dev-libs/B"), "dev-libs/B");
    // Different slot -> unchanged.
    assert_eq!(update_dbentry(&cmd, "dev-libs/B:2"), "dev-libs/B:2");
}

#[test]
fn update_dbentries_applies_in_sequence() {
    let commands = vec![
        UpdateCommand::Move {
            from: "dev-libs/A".to_string(),
            to: "dev-libs/A2".to_string(),
        },
        UpdateCommand::Move {
            from: "dev-libs/A2".to_string(),
            to: "dev-libs/A3".to_string(),
        },
    ];
    assert_eq!(update_dbentries(&commands, "dev-libs/A"), "dev-libs/A3");
}

#[test]
fn world_set_expands_selected_and_system() {
    let mut registry = SetRegistry::new();
    registry.define_world();
    registry.define("selected", vec![SetMember::Atom("dev-libs/A".to_string())]);
    registry.define(
        "system",
        vec![SetMember::Atom("sys-apps/baselayout".to_string())],
    );

    let world = registry.expand("world");
    assert!(world.contains(&"dev-libs/A".to_string()));
    assert!(world.contains(&"sys-apps/baselayout".to_string()));
}

#[test]
fn nested_set_refs_resolve_with_cycle_protection() {
    let mut registry = SetRegistry::new();
    // a -> b -> a (cycle) plus distinct atoms.
    registry.define(
        "a",
        vec![
            SetMember::Atom("cat/a-pkg".to_string()),
            SetMember::SetRef("b".to_string()),
        ],
    );
    registry.define(
        "b",
        vec![
            SetMember::Atom("cat/b-pkg".to_string()),
            SetMember::SetRef("a".to_string()),
        ],
    );
    let expanded = registry.expand("a");
    assert_eq!(
        expanded,
        vec!["cat/a-pkg".to_string(), "cat/b-pkg".to_string()]
    );
}

#[test]
fn world_file_add_remove_and_render() {
    let mut world = WorldFile::parse("dev-libs/A\n# a comment\nsys-apps/portage\n");
    assert_eq!(world.atoms().len(), 2);
    assert!(world.add("app-misc/new"));
    assert!(!world.add("dev-libs/A")); // already present
    assert!(world.remove("sys-apps/portage"));
    assert!(!world.remove("not-there/x"));
    // render is sorted, newline-terminated.
    assert_eq!(world.render(), "app-misc/new\ndev-libs/A\n");
}

#[test]
fn set_member_parse_distinguishes_refs() {
    assert_eq!(
        SetMember::parse("@world"),
        SetMember::SetRef("world".to_string())
    );
    assert_eq!(
        SetMember::parse("dev-libs/A"),
        SetMember::Atom("dev-libs/A".to_string())
    );
}
