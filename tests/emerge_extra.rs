//! Extended integration tests porting observable CLI-parsing and config-protect
//! behavior from Portage upstream test_actions.py and test_config_protect.py.
//!
//! Reference:
//! - research/portage/lib/portage/tests/emerge/test_actions.py
//! - research/portage/lib/portage/tests/emerge/test_config_protect.py

use diverge::cli::{CliError, EmergeAction, EmergeRequest, YesNo};
use diverge::executor::config_protect::ConfigProtect;

// ============================================================================
// CLI Parsing: Extended flag clusters, action selections, and option forms
// ============================================================================

/// Tests more complex bundled short-flag clusters.
/// Derives from test_actions.py patterns for multi-flag emerge invocations.
#[test]
fn complex_bundled_short_flags() {
    // -pvuD1: pretend, verbose, update, deep, oneshot
    let request = EmergeRequest::parse(["-pvuD1", "dev-libs/A"]).unwrap();
    assert!(request.options.pretend);
    assert!(request.options.verbose);
    assert!(request.options.update);
    assert!(request.options.deep);
    assert!(request.options.oneshot);
    assert_eq!(request.action, EmergeAction::Merge);

    // -pq: pretend, quiet
    let request = EmergeRequest::parse(["-pq", "dev-libs/A"]).unwrap();
    assert!(request.options.pretend);
    assert_eq!(request.options.quiet, YesNo::Yes);
}

/// Tests that multiple boolean flags compose correctly.
#[test]
fn multiple_boolean_flags() {
    // --pretend --verbose --update --nodeps --usepkg --noreplace
    let request = EmergeRequest::parse([
        "--pretend",
        "--verbose",
        "--update",
        "--nodeps",
        "--usepkg",
        "--noreplace",
        "dev-libs/A",
    ])
    .unwrap();
    assert!(request.options.pretend);
    assert!(request.options.verbose);
    assert!(request.options.update);
    assert!(request.options.nodeps);
    assert!(request.options.usepkg);
    assert!(request.options.noreplace);
}

/// Tests action selection via short flags (e.g. -c, -C, -s, -P).
#[test]
fn short_action_flags_prune_clean_info() {
    // -P is --prune (action)
    let request = EmergeRequest::parse(["-P", "dev-libs/A"]).unwrap();
    assert_eq!(request.action, EmergeAction::Prune);

    // Info is --info (no targets unless sets provided)
    let request = EmergeRequest::parse(["--info"]).unwrap();
    assert_eq!(request.action, EmergeAction::Info);

    // Multiple package targets for unmerge
    let request = EmergeRequest::parse(["-C", "dev-libs/A", "dev-libs/B"]).unwrap();
    assert_eq!(request.action, EmergeAction::Unmerge);
    assert_eq!(request.targets.len(), 2);
}

/// Tests --opt=value forms for yes/no and integer options.
/// Ported from test_actions.py option parsing patterns.
#[test]
fn equals_form_yes_no_options() {
    // --ask=yes, --quiet=no
    let request =
        EmergeRequest::parse(["--ask=yes", "--quiet=no", "--autounmask=True", "dev-libs/A"])
            .unwrap();
    assert_eq!(request.options.ask, YesNo::Yes);
    assert_eq!(request.options.quiet, YesNo::No);
    assert_eq!(request.options.autounmask, YesNo::Yes);

    // --getbinpkg=false
    let request = EmergeRequest::parse(["--getbinpkg=false", "dev-libs/A"]).unwrap();
    assert_eq!(request.options.getbinpkg, YesNo::No);
}

/// Tests --opt=value forms for integer options.
#[test]
fn equals_form_integer_options() {
    // --jobs=4
    let request = EmergeRequest::parse(["--jobs=4", "dev-libs/A"]).unwrap();
    assert_eq!(request.options.jobs, Some(4));

    // --load-average=8
    let request = EmergeRequest::parse(["--load-average=8", "dev-libs/A"]).unwrap();
    assert_eq!(request.options.load_average, Some(8));

    // Both together
    let request = EmergeRequest::parse(["--jobs=16", "--load-average=12", "dev-libs/A"]).unwrap();
    assert_eq!(request.options.jobs, Some(16));
    assert_eq!(request.options.load_average, Some(12));
}

/// Tests that invalid integer values are properly rejected.
#[test]
fn invalid_integer_option_values() {
    assert!(matches!(
        EmergeRequest::parse(["--jobs=abc", "dev-libs/A"]),
        Err(CliError::InvalidOptionValue {
            option: ref o,
            value: ref v
        }) if o == "--jobs" && v == "abc"
    ));

    assert!(matches!(
        EmergeRequest::parse(["--load-average=", "dev-libs/A"]),
        Err(CliError::InvalidOptionValue { .. })
    ));
}

/// Tests --columns (and --cols alias) flag.
#[test]
fn columns_flag_variants() {
    let request = EmergeRequest::parse(["--columns", "dev-libs/A"]).unwrap();
    assert!(request.options.columns);

    let request = EmergeRequest::parse(["--cols", "dev-libs/A"]).unwrap();
    assert!(request.options.columns);

    // Short form -w
    let request = EmergeRequest::parse(["-w", "dev-libs/A"]).unwrap();
    assert!(request.options.columns);
}

/// Tests --skipfirst flag (including --skip-first variant).
#[test]
fn skipfirst_flag_variants() {
    let request = EmergeRequest::parse(["--skipfirst", "dev-libs/A"]).unwrap();
    assert!(request.options.skipfirst);

    let request = EmergeRequest::parse(["--skip-first", "dev-libs/A"]).unwrap();
    assert!(request.options.skipfirst);
}

/// Tests all short action flags map to their correct actions.
#[test]
fn all_short_action_mappings() {
    // -c => --depclean
    assert_eq!(
        EmergeRequest::parse(["-c"]).unwrap().action,
        EmergeAction::Depclean
    );
    // -C => --unmerge
    assert_eq!(
        EmergeRequest::parse(["-C", "dev-libs/A"]).unwrap().action,
        EmergeAction::Unmerge
    );
    // -s => --search
    assert_eq!(
        EmergeRequest::parse(["-s", "term"]).unwrap().action,
        EmergeAction::Search
    );
    // -P => --prune
    assert_eq!(
        EmergeRequest::parse(["-P", "dev-libs/A"]).unwrap().action,
        EmergeAction::Prune
    );
    // -h => --help
    assert_eq!(
        EmergeRequest::parse(["-h"]).unwrap().action,
        EmergeAction::Help
    );
    // -V => --version
    assert_eq!(
        EmergeRequest::parse(["-V"]).unwrap().action,
        EmergeAction::Version
    );
}

/// Tests buildpkg / buildpkgonly flags.
#[test]
fn buildpkg_variants() {
    // -b => --buildpkg
    let request = EmergeRequest::parse(["-b", "dev-libs/A"]).unwrap();
    assert!(request.options.buildpkg);

    // -B => --buildpkgonly
    let request = EmergeRequest::parse(["-B", "dev-libs/A"]).unwrap();
    assert!(request.options.buildpkgonly);

    // --buildpkg vs --buildpkgonly are distinct
    let request = EmergeRequest::parse(["--buildpkg", "dev-libs/A"]).unwrap();
    assert!(request.options.buildpkg);
    assert!(!request.options.buildpkgonly);

    let request = EmergeRequest::parse(["--buildpkgonly", "dev-libs/A"]).unwrap();
    assert!(request.options.buildpkgonly);
}

/// Tests use/package short flags (-g, -k, -G, -K).
#[test]
fn usepkg_short_flags() {
    // -g and -k both => --usepkg
    let request = EmergeRequest::parse(["-g", "dev-libs/A"]).unwrap();
    assert!(request.options.usepkg);

    let request = EmergeRequest::parse(["-k", "dev-libs/A"]).unwrap();
    assert!(request.options.usepkg);

    // -G and -K both => --usepkgonly (which implies --usepkg)
    let request = EmergeRequest::parse(["-G", "dev-libs/A"]).unwrap();
    assert!(request.options.usepkgonly);
    assert!(request.options.usepkg);

    let request = EmergeRequest::parse(["-K", "dev-libs/A"]).unwrap();
    assert!(request.options.usepkgonly);
    assert!(request.options.usepkg);
}

/// Tests remaining boolean flags: emptytree, fetchonly, debug, tree, resume.
#[test]
fn additional_boolean_flags() {
    let request = EmergeRequest::parse([
        "--emptytree",
        "--fetchonly",
        "--debug",
        "--tree",
        "--resume",
        "dev-libs/A",
    ])
    .unwrap();
    assert!(request.options.emptytree);
    assert!(request.options.fetchonly);
    assert!(request.options.debug);
    assert!(request.options.tree);
    assert!(request.options.resume);
}

/// Tests newuse / changed-use flags.
#[test]
fn use_tracking_flags() {
    // -N => --newuse
    let request = EmergeRequest::parse(["-N", "dev-libs/A"]).unwrap();
    assert!(request.options.newuse);

    // -U => --changed-use
    let request = EmergeRequest::parse(["-U", "dev-libs/A"]).unwrap();
    assert!(request.options.changed_use);

    // --newuse and --changed-use are distinct
    let request = EmergeRequest::parse(["--newuse", "dev-libs/A"]).unwrap();
    assert!(request.options.newuse);
    assert!(!request.options.changed_use);

    let request = EmergeRequest::parse(["--changed-use", "dev-libs/A"]).unwrap();
    assert!(request.options.changed_use);
    assert!(!request.options.newuse);
}

/// Tests onlydeps (single package dependency mode).
#[test]
fn onlydeps_option() {
    let request = EmergeRequest::parse(["--onlydeps", "dev-libs/A"]).unwrap();
    assert!(request.options.onlydeps);

    // -o => --onlydeps
    let request = EmergeRequest::parse(["-o", "dev-libs/A"]).unwrap();
    assert!(request.options.onlydeps);
}

/// Tests --nodeps (skip dependencies).
#[test]
fn nodeps_option() {
    let request = EmergeRequest::parse(["--nodeps", "dev-libs/A"]).unwrap();
    assert!(request.options.nodeps);

    // -O => --nodeps
    let request = EmergeRequest::parse(["-O", "dev-libs/A"]).unwrap();
    assert!(request.options.nodeps);
}

/// Tests multiple packages with mixed short and long options.
#[test]
fn mixed_flags_with_multiple_packages() {
    let request = EmergeRequest::parse([
        "-pv",
        "--update",
        "--jobs=2",
        "dev-libs/A",
        "=dev-libs/B-1.5",
        "sys-apps/C",
    ])
    .unwrap();
    assert!(request.options.pretend);
    assert!(request.options.verbose);
    assert!(request.options.update);
    assert_eq!(request.options.jobs, Some(2));
    assert_eq!(request.targets.len(), 3);
    assert_eq!(request.targets[0].cp(), "dev-libs/A");
    assert_eq!(request.targets[1].cpv(), "dev-libs/B-1.5");
    assert_eq!(request.targets[2].cp(), "sys-apps/C");
}

/// Tests package set targets with various actions.
#[test]
fn package_sets_with_actions() {
    // Merge with @world
    let request = EmergeRequest::parse(["-uD", "@world"]).unwrap();
    assert_eq!(request.action, EmergeAction::Merge);
    assert_eq!(request.sets, vec!["world"]);
    assert!(request.targets.is_empty());

    // Unmerge with @system
    let request = EmergeRequest::parse(["--unmerge", "@system"]).unwrap();
    assert_eq!(request.action, EmergeAction::Unmerge);
    assert_eq!(request.sets, vec!["system"]);

    // Config with @preserved-rebuild
    let request = EmergeRequest::parse(["--config", "@preserved-rebuild"]).unwrap();
    assert_eq!(request.action, EmergeAction::Config);
    assert_eq!(request.sets, vec!["preserved-rebuild"]);
}

/// Tests multiple package sets in one invocation.
#[test]
fn multiple_package_sets() {
    let request = EmergeRequest::parse(["@world", "@system"]).unwrap();
    assert_eq!(request.sets.len(), 2);
    assert_eq!(request.sets[0], "world");
    assert_eq!(request.sets[1], "system");
    assert!(request.targets.is_empty());
}

/// Tests mixed atoms and sets.
#[test]
fn mixed_atoms_and_sets() {
    let request = EmergeRequest::parse(["dev-libs/A", "@world", "sys-apps/B"]).unwrap();
    assert_eq!(request.targets.len(), 2);
    assert_eq!(request.sets.len(), 1);
    assert_eq!(request.sets[0], "world");
}

// ============================================================================
// CONFIG_PROTECT: Extended isprotected and protect_filename scenarios
// ============================================================================

/// Tests CONFIG_PROTECT with deeply nested directories.
/// Relates to test_config_protect.py path handling.
#[test]
fn config_protect_nested_paths() {
    let cp = ConfigProtect::new(&["/etc/app/config"], &[]);
    assert!(cp.is_protected("/etc/app/config/main.conf"));
    assert!(cp.is_protected("/etc/app/config/subdir/settings.conf"));
    assert!(!cp.is_protected("/etc/app/other.conf"));
    assert!(!cp.is_protected("/etc/other/file.conf"));
}

/// Tests CONFIG_PROTECT with multiple protect entries.
#[test]
fn config_protect_multiple_entries() {
    let cp = ConfigProtect::new(&["/etc", "/opt/app/conf"], &[]);
    assert!(cp.is_protected("/etc/app.conf"));
    assert!(cp.is_protected("/opt/app/conf/settings.conf"));
    assert!(!cp.is_protected("/usr/local/config"));
}

/// Tests CONFIG_PROTECT_MASK precedence: longer mask wins.
#[test]
fn config_protect_mask_longer_wins() {
    let cp = ConfigProtect::new(&["/etc"], &["/etc/app", "/etc/app/cache"]);
    // /etc/app is masked.
    assert!(!cp.is_protected("/etc/app/config.conf"));
    // But /etc/app/cache is even more specifically masked.
    assert!(!cp.is_protected("/etc/app/cache/data.db"));
    // Anything else under /etc is still protected.
    assert!(cp.is_protected("/etc/other.conf"));
}

/// Tests CONFIG_PROTECT with path normalization (leading slashes).
#[test]
fn config_protect_path_normalization() {
    // Entries provided without leading slashes should be normalized.
    let cp = ConfigProtect::new(&["etc", "/opt/app"], &[]);
    assert!(cp.is_protected("/etc/passwd.conf"));
    assert!(cp.is_protected("/opt/app/settings"));
}

/// Tests boundary checking: /etc/foobaz is under /etc but not under /etc/foo.
/// Reference: test_config_protect.py ConfigProtect path boundary checks.
#[test]
fn config_protect_path_boundary() {
    let cp = ConfigProtect::new(&["/etc"], &["/etc/foo"]);
    // /etc/bar is protected
    assert!(cp.is_protected("/etc/bar.conf"));
    // /etc/foo is masked (not protected)
    assert!(!cp.is_protected("/etc/foo/bar.conf"));
    // /etc/foobaz is protected by /etc (not masked by /etc/foo, boundary check).
    assert!(cp.is_protected("/etc/foobaz"));
}

/// Tests protect_filename with no existing destination.
#[test]
fn protect_filename_no_existing_dest() {
    // When dest does not exist, return the plain basename.
    let result = ConfigProtect::protect_filename("app.conf", &[], false);
    assert_eq!(result, "app.conf");
}

/// Tests protect_filename with existing dest, building a sequence.
#[test]
fn protect_filename_sequence_building() {
    // First protected version.
    let result = ConfigProtect::protect_filename("app.conf", &["app.conf".to_string()], true);
    assert_eq!(result, "._cfg0000_app.conf");

    // Existing ._cfg0000_; next should be 0001.
    let siblings = vec!["app.conf".to_string(), "._cfg0000_app.conf".to_string()];
    let result = ConfigProtect::protect_filename("app.conf", &siblings, true);
    assert_eq!(result, "._cfg0001_app.conf");

    // Sequence continues: 0000, 0001, 0002.
    let siblings = vec![
        "app.conf".to_string(),
        "._cfg0000_app.conf".to_string(),
        "._cfg0001_app.conf".to_string(),
    ];
    let result = ConfigProtect::protect_filename("app.conf", &siblings, true);
    assert_eq!(result, "._cfg0002_app.conf");
}

/// Tests protect_filename skips unrelated ._cfg files.
#[test]
fn protect_filename_skips_unrelated_cfg() {
    // ._cfg0000_other.conf is not related to app.conf
    let siblings = vec!["app.conf".to_string(), "._cfg0000_other.conf".to_string()];
    let result = ConfigProtect::protect_filename("app.conf", &siblings, true);
    // Should still be 0000 because the other file is for a different basename.
    assert_eq!(result, "._cfg0000_app.conf");
}

/// Tests protect_filename with gaps in the sequence.
#[test]
fn protect_filename_with_gaps() {
    // If 0001 exists but 0000 doesn't (unusual but possible), next is 0002.
    let siblings = vec![
        "app.conf".to_string(),
        "._cfg0001_app.conf".to_string(),
        "._cfg0003_app.conf".to_string(),
    ];
    let result = ConfigProtect::protect_filename("app.conf", &siblings, true);
    // Counter should increment from the highest found.
    assert_eq!(result, "._cfg0004_app.conf");
}

/// Tests protect_filename with malformed ._cfg entries.
#[test]
fn protect_filename_ignores_malformed_cfg() {
    let siblings = vec![
        "app.conf".to_string(),
        "._cfgABCD_app.conf".to_string(),   // Non-numeric
        "._cfg00_app.conf".to_string(),     // Too short (< 4 digits)
        "._cfg0000".to_string(),            // Missing underscore and basename
        "._cfg0000_other.conf".to_string(), // Wrong basename
    ];
    let result = ConfigProtect::protect_filename("app.conf", &siblings, true);
    // Should be 0000 since no valid ._cfgNNNN_app.conf was found.
    assert_eq!(result, "._cfg0000_app.conf");
}

/// Tests protect_filename with large counter numbers.
#[test]
fn protect_filename_large_counter() {
    let siblings = vec![
        "app.conf".to_string(),
        "._cfg9998_app.conf".to_string(),
        "._cfg9999_app.conf".to_string(),
    ];
    let result = ConfigProtect::protect_filename("app.conf", &siblings, true);
    // Counter increments: 9999 + 1 = 10000, formatted as 5 digits.
    assert_eq!(result, "._cfg10000_app.conf");
}

/// Tests protect_filename with dots in the basename.
#[test]
fn protect_filename_basename_with_dots() {
    let siblings = vec![
        "app.conf.local".to_string(),
        "._cfg0000_app.conf.local".to_string(),
    ];
    let result = ConfigProtect::protect_filename("app.conf.local", &siblings, true);
    assert_eq!(result, "._cfg0001_app.conf.local");
}

/// Tests protect_filename with underscores in the basename.
#[test]
fn protect_filename_basename_with_underscores() {
    let siblings = vec![
        "my_app_config.conf".to_string(),
        "._cfg0000_my_app_config.conf".to_string(),
    ];
    let result = ConfigProtect::protect_filename("my_app_config.conf", &siblings, true);
    assert_eq!(result, "._cfg0001_my_app_config.conf");
}

/// Tests protect_filename when dest_exists=false always returns plain name.
#[test]
fn protect_filename_dest_not_exists_always_plain() {
    // Even with many siblings, if dest_exists=false, return plain name.
    let siblings = vec![
        "._cfg0000_app.conf".to_string(),
        "._cfg0001_app.conf".to_string(),
    ];
    let result = ConfigProtect::protect_filename("app.conf", &siblings, false);
    assert_eq!(result, "app.conf");
}

/// Tests CONFIG_PROTECT with empty entries.
#[test]
fn config_protect_empty_entries() {
    // Empty entries should be filtered out.
    let cp = ConfigProtect::new(&["", "/etc", ""], &[]);
    assert!(cp.is_protected("/etc/app.conf"));
}

/// Tests CONFIG_PROTECT_MASK with empty entries.
#[test]
fn config_protect_mask_empty_entries() {
    let cp = ConfigProtect::new(&["/etc"], &["", "/etc/app", ""]);
    assert!(!cp.is_protected("/etc/app/config.conf"));
    assert!(cp.is_protected("/etc/other.conf"));
}

/// Tests complex scenario: multiple protect and mask entries.
#[test]
fn config_protect_complex_scenario() {
    let cp = ConfigProtect::new(
        &["/etc", "/opt/app/etc"],
        &["/etc/app/cache", "/opt/app/etc/temp"],
    );
    // /etc/passwd is protected
    assert!(cp.is_protected("/etc/passwd"));
    // /etc/app/real.conf is protected
    assert!(cp.is_protected("/etc/app/real.conf"));
    // /etc/app/cache/db is masked (not protected)
    assert!(!cp.is_protected("/etc/app/cache/db"));
    // /opt/app/etc/config is protected
    assert!(cp.is_protected("/opt/app/etc/config"));
    // /opt/app/etc/temp/data is masked (not protected)
    assert!(!cp.is_protected("/opt/app/etc/temp/data"));
    // /usr/local/config is not under any protect entry
    assert!(!cp.is_protected("/usr/local/config"));
}
