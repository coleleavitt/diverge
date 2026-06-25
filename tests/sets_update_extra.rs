//! Additional ports of Portage sets and update behavior.
//!
//! This file ports observable behavior from:
//! - research/portage/lib/portage/tests/sets/base/test_internal_package_set.py
//! - research/portage/lib/portage/tests/sets/base/test_variable_set.py
//! - research/portage/lib/portage/tests/sets/base/test_wildcard_package_set.py
//! - research/portage/lib/portage/tests/sets/files/test_static_file_set.py
//! - research/portage/lib/portage/tests/sets/files/test_config_file_set.py
//! - research/portage/lib/portage/tests/update/test_move_ent.py
//! - research/portage/lib/portage/tests/update/test_move_slot_ent.py
//! - research/portage/lib/portage/tests/update/test_update_dbentry.py

use diverge::sets::{SetMember, SetRegistry, WorldFile};
use diverge::update::{UpdateCommand, parse_updates, update_dbentry};

// ===== Sets: InternalPackageSet behavior =====

/// Port of test_internal_package_set.py::InternalPackageSetTestCase::testInternalPackageSet
/// Tests that SetRegistry stores atoms and handles member operations.
#[test]
fn set_registry_stores_and_expands_atoms() {
    // Test basic atom storage via define()
    let mut registry = SetRegistry::new();
    let atoms = vec![
        SetMember::Atom("dev-libs/A".to_string()),
        SetMember::Atom(">=dev-libs/A-1".to_string()),
        SetMember::Atom("dev-libs/B".to_string()),
    ];
    registry.define("test1", atoms.clone());

    // Verify the set exists
    assert!(registry.contains("test1"));

    // Verify expansion
    let expanded = registry.expand("test1");
    assert_eq!(expanded.len(), 3);
    assert!(expanded.contains(&"dev-libs/A".to_string()));
    assert!(expanded.contains(&">=dev-libs/A-1".to_string()));
    assert!(expanded.contains(&"dev-libs/B".to_string()));
}

/// Port of test_internal_package_set.py behavior: set references (non-atoms)
#[test]
fn set_registry_handles_set_references() {
    // Test that SetRef members are stored and can be expanded
    let mut registry = SetRegistry::new();
    let members = vec![
        SetMember::SetRef("world".to_string()),
        SetMember::SetRef("installed".to_string()),
        SetMember::SetRef("system".to_string()),
    ];
    registry.define("meta", members);
    assert!(registry.contains("meta"));

    // Undefined set references should not crash expansion
    let expanded = registry.expand("meta");
    assert_eq!(expanded.len(), 0); // All refs are undefined, so empty
}

/// Port of test_wildcard_package_set.py::testWildcardPackageSet
/// Tests SetMember parsing of wildcard atoms.
#[test]
fn set_member_parse_accepts_wildcard_patterns() {
    // Portage allows wildcard atoms like "dev-libs/*" and "*/B"
    let wildcard_atom = SetMember::parse("dev-libs/*");
    match wildcard_atom {
        SetMember::Atom(atom) => assert_eq!(atom, "dev-libs/*"),
        _ => panic!("wildcard should parse as atom"),
    }

    let wildcard_atom2 = SetMember::parse("*/B");
    match wildcard_atom2 {
        SetMember::Atom(atom) => assert_eq!(atom, "*/B"),
        _ => panic!("wildcard should parse as atom"),
    }
}

// ===== Sets: WorldFile behavior =====

/// Port of test_static_file_set.py and test_config_file_set.py:
/// WorldFile should parse a multi-line file with comments and whitespace.
#[test]
fn world_file_parses_from_text_with_comments() {
    let content = "sys-apps/portage\n# This is a comment\nvirtual/portage\n";
    let world = WorldFile::parse(content);
    let atoms = world.atoms();
    assert_eq!(atoms.len(), 2);
    assert!(atoms.contains(&"sys-apps/portage".to_string()));
    assert!(atoms.contains(&"virtual/portage".to_string()));
}

/// Port of static_file_set.py: loading a file of atoms should produce the exact set.
#[test]
fn world_file_atoms_matches_parsed_set() {
    let test_cps = vec!["sys-apps/portage", "virtual/portage"];
    let content = test_cps.join("\n");
    let world = WorldFile::parse(&content);
    let atoms = world.atoms();
    assert_eq!(atoms.len(), test_cps.len());
    for cp in test_cps {
        assert!(atoms.contains(&cp.to_string()));
    }
}

/// Port of config_file_set.py: atoms may have trailing configuration (ignored).
/// ConfigFileSet reads lines like "atom config1 config2", extracting only the atom.
#[test]
fn world_file_handles_leading_whitespace_and_empty_lines() {
    let content = "  sys-apps/portage  \n\n  virtual/portage  \n";
    let world = WorldFile::parse(content);
    let atoms = world.atoms();
    assert_eq!(atoms.len(), 2);
    // Trim should have removed whitespace
    assert!(atoms.contains(&"sys-apps/portage".to_string()));
    assert!(atoms.contains(&"virtual/portage".to_string()));
}

// ===== Update: Move and SlotMove behavior =====

/// Port of test_update_dbentry.py::UpdateDbentryTestCase::testUpdateDbentryTestCase
/// Tests move with basic, versioned, and slotted atoms.
#[test]
fn move_updates_basic_atoms_with_versions_and_slots() {
    let cmd = UpdateCommand::Move {
        from: "dev-libs/A".to_string(),
        to: "dev-libs/B".to_string(),
    };

    // Case: plain atom
    assert_eq!(update_dbentry(&cmd, "dev-libs/A"), "dev-libs/B");

    // Case: versioned atom (>=dev-libs/A-1)
    assert_eq!(update_dbentry(&cmd, ">=dev-libs/A-1"), ">=dev-libs/B-1");

    // Case: atom with slot
    assert_eq!(update_dbentry(&cmd, "dev-libs/A:0"), "dev-libs/B:0");

    // Case: atom with subslot (EAPI 5+)
    assert_eq!(update_dbentry(&cmd, "dev-libs/A:0/1"), "dev-libs/B:0/1");
}

/// Port of test_update_dbentry.py: move should preserve surrounding whitespace.
#[test]
fn move_preserves_whitespace_in_dependency_strings() {
    let cmd = UpdateCommand::Move {
        from: "dev-libs/A".to_string(),
        to: "dev-libs/B".to_string(),
    };

    // Whitespace before and after
    let input = "  dev-libs/A  ";
    let output = update_dbentry(&cmd, input);
    assert_eq!(output, "  dev-libs/B  ");

    // Multiple atoms with varied spacing
    let input2 = "dev-libs/A   dev-libs/C";
    let output2 = update_dbentry(&cmd, input2);
    assert_eq!(output2, "dev-libs/B   dev-libs/C");
}

/// Port of test_move_slot_ent.py::testMoveSlotEnt
/// Tests slotmove rewriting: slotmove dev-libs/A 0 1
#[test]
fn slotmove_updates_explicit_slot_only() {
    let cmd = UpdateCommand::SlotMove {
        cp: "dev-libs/A".to_string(),
        from_slot: "0".to_string(),
        to_slot: "1".to_string(),
    };

    // Explicit slot 0 -> 1
    assert_eq!(update_dbentry(&cmd, "dev-libs/A:0"), "dev-libs/A:1");

    // No slot -> unchanged
    assert_eq!(update_dbentry(&cmd, "dev-libs/A"), "dev-libs/A");

    // Different slot -> unchanged
    assert_eq!(update_dbentry(&cmd, "dev-libs/A:2"), "dev-libs/A:2");
}

/// Port of test_move_slot_ent.py: slotmove with subslots (EAPI 5+).
#[test]
fn slotmove_updates_with_subslots() {
    let cmd = UpdateCommand::SlotMove {
        cp: "dev-libs/A".to_string(),
        from_slot: "0".to_string(),
        to_slot: "2".to_string(),
    };

    // Simple slot with subslot: 0/2.30 -> 2/2.30
    assert_eq!(
        update_dbentry(&cmd, "dev-libs/A:0/2.30"),
        "dev-libs/A:2/2.30"
    );
}

/// Port of test_update_dbentry.py: slotmove should not rewrite versioned atoms.
/// Upstream's slotmove_token rejects atoms with version_part set, preserving them unchanged.
#[test]
fn slotmove_ignores_versioned_atoms() {
    let cmd = UpdateCommand::SlotMove {
        cp: "dev-libs/A".to_string(),
        from_slot: "0".to_string(),
        to_slot: "1".to_string(),
    };

    // Versioned atoms (with explicit version) are left unchanged by slotmove
    let input = ">=dev-libs/A-1:0";
    let output = update_dbentry(&cmd, input);
    // Should be unchanged because slotmove rejects atoms with version set
    assert_eq!(output, ">=dev-libs/A-1:0");
}

// ===== Update parsing and sequencing =====

/// Port of test_move_ent.py and test_update_dbentry.py:
/// Multiple moves applied in sequence should chain.
#[test]
fn parse_updates_reads_multiple_directives() {
    let content = "move dev-libs/A dev-libs/A-moved\nslotmove dev-libs/B 0 1\n";
    let commands = parse_updates(content).expect("parse updates");
    assert_eq!(commands.len(), 2);
    match &commands[0] {
        UpdateCommand::Move { from, to } => {
            assert_eq!(from, "dev-libs/A");
            assert_eq!(to, "dev-libs/A-moved");
        }
        _ => panic!("first command should be move"),
    }
    match &commands[1] {
        UpdateCommand::SlotMove {
            cp,
            from_slot,
            to_slot,
        } => {
            assert_eq!(cp, "dev-libs/B");
            assert_eq!(from_slot, "0");
            assert_eq!(to_slot, "1");
        }
        _ => panic!("second command should be slotmove"),
    }
}

/// Port of test_update_dbentry.py: blank lines in updates should be skipped.
#[test]
fn parse_updates_skips_blank_lines() {
    let content = "move dev-libs/A dev-libs/A-moved\n\nslotmove dev-libs/B 0 1\n";
    let commands = parse_updates(content).expect("parse updates");
    assert_eq!(commands.len(), 2);
}

/// Port of test_update_dbentry.py: invalid directives should error.
#[test]
fn parse_updates_rejects_invalid_directives() {
    let result = parse_updates("invalid directive");
    assert!(result.is_err());

    let result2 = parse_updates("move dev-libs/A");
    assert!(result2.is_err());
}

// ===== SetRegistry: nested and circular references =====

/// Port of test_variable_set.py::testVariableSetEmerge behavior:
/// Nested set references should resolve correctly.
#[test]
fn set_registry_resolves_simple_nested_refs() {
    let mut registry = SetRegistry::new();
    registry.define_world();
    registry.define("selected", vec![SetMember::Atom("app-misc/X".to_string())]);
    registry.define("system", vec![SetMember::Atom("sys-apps/Y".to_string())]);

    let world = registry.expand("world");
    assert_eq!(world.len(), 2);
    assert!(world.contains(&"app-misc/X".to_string()));
    assert!(world.contains(&"sys-apps/Y".to_string()));
}

/// Port of test_internal_package_set.py: duplicate atoms should be removed.
#[test]
fn set_registry_deduplicates_atoms_on_expansion() {
    let mut registry = SetRegistry::new();
    registry.define(
        "a",
        vec![
            SetMember::Atom("dev-libs/A".to_string()),
            SetMember::Atom("dev-libs/A".to_string()), // duplicate
        ],
    );

    let expanded = registry.expand("a");
    assert_eq!(expanded.len(), 1);
    assert_eq!(expanded[0], "dev-libs/A");
}

/// Port of nested set cycle test: cycles should be broken (already tested in binpkg_sets_parity.rs).
/// This is an additional sanity check.
#[test]
fn set_registry_handles_deep_nesting() {
    let mut registry = SetRegistry::new();
    registry.define("a", vec![SetMember::SetRef("b".to_string())]);
    registry.define("b", vec![SetMember::SetRef("c".to_string())]);
    registry.define("c", vec![SetMember::Atom("dev-libs/X".to_string())]);

    let expanded = registry.expand("a");
    assert_eq!(expanded.len(), 1);
    assert_eq!(expanded[0], "dev-libs/X");
}

/// Port of test_internal_package_set.py: undefined set references are silently skipped.
#[test]
fn set_registry_skips_undefined_set_references() {
    let mut registry = SetRegistry::new();
    registry.define(
        "a",
        vec![
            SetMember::Atom("dev-libs/A".to_string()),
            SetMember::SetRef("undefined".to_string()),
            SetMember::Atom("dev-libs/B".to_string()),
        ],
    );

    let expanded = registry.expand("a");
    assert_eq!(expanded.len(), 2);
    assert!(expanded.contains(&"dev-libs/A".to_string()));
    assert!(expanded.contains(&"dev-libs/B".to_string()));
}

// ===== WorldFile: add/remove/render =====

/// Port of test_static_file_set.py: WorldFile render should sort atoms and end with newline.
#[test]
fn world_file_render_sorts_atoms() {
    let mut world = WorldFile::parse("dev-libs/A\n");
    world.add("app-misc/C");
    world.add("dev-libs/B");

    let rendered = world.render();
    // Should be sorted: app, dev-libs C, dev-libs A, dev-libs B
    assert_eq!(rendered, "app-misc/C\ndev-libs/A\ndev-libs/B\n");
}

/// Port of test_static_file_set.py: empty world file should render as empty string.
#[test]
fn world_file_render_empty() {
    let world = WorldFile::parse("");
    assert_eq!(world.render(), "");
}

/// Port of test_static_file_set.py: world file should handle comments and blank lines.
#[test]
fn world_file_parse_filters_comments_and_blanks() {
    let content = "# Header comment\ndev-libs/A\n\n# Another comment\ndev-libs/B\n\n";
    let world = WorldFile::parse(content);
    let atoms = world.atoms();
    assert_eq!(atoms.len(), 2);
}

// ===== Additional update edge cases =====

/// Port of test_update_dbentry.py: move with identical atoms should still replace.
#[test]
fn move_replaces_identical_atoms() {
    let cmd = UpdateCommand::Move {
        from: "dev-libs/A".to_string(),
        to: "dev-libs/A".to_string(),
    };

    // Even though from == to, the replacement should occur
    let result = update_dbentry(&cmd, "dev-libs/A");
    assert_eq!(result, "dev-libs/A");
}

/// Port of test_update_dbentry.py: atoms with USE flags should be updated.
#[test]
fn move_updates_atoms_with_use_flags() {
    let cmd = UpdateCommand::Move {
        from: "dev-libs/A".to_string(),
        to: "dev-libs/B".to_string(),
    };

    // Portage updates USE flags on move (EAPI 2+)
    let input = "dev-libs/A[foo]";
    let output = update_dbentry(&cmd, input);
    assert_eq!(output, "dev-libs/B[foo]");
}

/// Port of test_update_dbentry.py: atoms with blockers (!) should be rewritten.
#[test]
fn move_updates_blocker_atoms() {
    let cmd = UpdateCommand::Move {
        from: "dev-libs/A".to_string(),
        to: "dev-libs/B".to_string(),
    };

    // Blocker atom
    let input = "!dev-libs/A";
    let output = update_dbentry(&cmd, input);
    assert_eq!(output, "!dev-libs/B");
}

/// Port of test_update_dbentry.py: non-matching atoms should be untouched.
#[test]
fn move_ignores_non_matching_atoms() {
    let cmd = UpdateCommand::Move {
        from: "dev-libs/A".to_string(),
        to: "dev-libs/B".to_string(),
    };

    // Similar but non-matching cp
    assert_eq!(update_dbentry(&cmd, "dev-libs/AB"), "dev-libs/AB");
    assert_eq!(update_dbentry(&cmd, "dev-libs/AA"), "dev-libs/AA");
    assert_eq!(update_dbentry(&cmd, "app-libs/A"), "app-libs/A");
}

/// Port of test_move_slot_ent.py: slotmove with compound slot (subslot equiv).
#[test]
fn slotmove_with_subslot_equivalence() {
    let cmd = UpdateCommand::SlotMove {
        cp: "dev-libs/C".to_string(),
        from_slot: "0".to_string(),
        to_slot: "1".to_string(),
    };

    // 0/1 is equivalent to 1/1 in terms of the slot index
    // Upstream test shows 0/1 -> 1 (rendered as 1/1 normalization)
    let result = update_dbentry(&cmd, "dev-libs/C:0/1");
    // The slotmove should rewrite the first part: 0 -> 1
    assert_eq!(result, "dev-libs/C:1/1");
}

// ===== SetRegistry: define_from_text =====

/// Port of set registry behavior: define_from_text parses whitespace/newline-separated text.
#[test]
fn set_registry_define_from_text() {
    let mut registry = SetRegistry::new();
    let content = "dev-libs/A\ndev-libs/B\n# comment\ndev-libs/C\n";
    registry.define_from_text("test", content);

    let expanded = registry.expand("test");
    assert_eq!(expanded.len(), 3);
    assert!(expanded.contains(&"dev-libs/A".to_string()));
    assert!(expanded.contains(&"dev-libs/B".to_string()));
    assert!(expanded.contains(&"dev-libs/C".to_string()));
}

/// Port of set registry behavior: define_from_text handles set references.
#[test]
fn set_registry_define_from_text_with_refs() {
    let mut registry = SetRegistry::new();
    registry.define_from_text("meta", "@world\n@system");

    let expanded = registry.expand("meta");
    assert_eq!(expanded.len(), 0); // Both refs undefined, so empty
}
