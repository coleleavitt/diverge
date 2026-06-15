# Portage Test Inventory and Rust Porting Plan

Source: `research/portage/lib/portage/tests` (read-only sparse Portage checkout).

This inventory records every upstream Python test file currently present so diverge can port behavior deliberately without copying GPL test code verbatim into Rust. Rust tests should be rewrites that cite the reference file and assert observable behavior.

## Summary

- Upstream test files inventoried: 239
- Representative Rust ports currently implemented: 4 files / 16 Rust tests
- Current Rust parity test entrypoint: `tests/portage.rs`
- Coverage command: `cargo llvm-cov --workspace --all-targets --summary-only`

## Test Areas

| Area | Count | Priority | Porting target |
| --- | ---: | --- | --- |
| `bin` | 5 | P2 helper command parity | `tests/portage/bin_*.rs` or module-specific fixture |
| `dbapi` | 5 | P2 repository/database views | `tests/portage/dbapi_*.rs` or module-specific fixture |
| `dep` | 21 | P0 domain semantics | `tests/portage/dep_*.rs` or module-specific fixture |
| `ebuild` | 8 | P1 ebuild/runtime boundary | `tests/portage/ebuild_*.rs` or module-specific fixture |
| `emaint` | 2 | P3 maintenance commands | `tests/portage/emaint_*.rs` or module-specific fixture |
| `emerge` | 8 | P0 CLI/action parity | `tests/portage/emerge_*.rs` or module-specific fixture |
| `env` | 4 | P1 config/profile parsing | `tests/portage/env_*.rs` or module-specific fixture |
| `glsa` | 1 | P3 security set behavior | `tests/portage/glsa_*.rs` or module-specific fixture |
| `gpkg` | 8 | P2 binary package integrity | `tests/portage/gpkg_*.rs` or module-specific fixture |
| `lafilefixer` | 1 | P3 legacy utility | `tests/portage/lafilefixer_*.rs` or module-specific fixture |
| `lazyimport` | 1 | P3 Python-only lint/baseline | `tests/portage/lazyimport_*.rs` or module-specific fixture |
| `lint` | 3 | P3 upstream lint only | `tests/portage/lint_*.rs` or module-specific fixture |
| `locks` | 2 | P2 locking | `tests/portage/locks_*.rs` or module-specific fixture |
| `news` | 1 | P3 news display | `tests/portage/news_*.rs` or module-specific fixture |
| `process` | 11 | P2 process/runtime adapters | `tests/portage/process_*.rs` or module-specific fixture |
| `resolver` | 111 | P0/P1 resolver parity | `tests/portage/resolver_*.rs` or module-specific fixture |
| `sets` | 6 | P1 package set semantics | `tests/portage/sets_*.rs` or module-specific fixture |
| `sync` | 2 | P2 sync adapters | `tests/portage/sync_*.rs` or module-specific fixture |
| `unicode` | 1 | P2 encoding behavior | `tests/portage/unicode_*.rs` or module-specific fixture |
| `update` | 3 | P2 global update transforms | `tests/portage/update_*.rs` or module-specific fixture |
| `util` | 32 | P2 shared utility behavior | `tests/portage/util_*.rs` or module-specific fixture |
| `versions` | 2 | P0 domain semantics | `tests/portage/versions_*.rs` or module-specific fixture |
| `xpak` | 1 | P2 binary package metadata | `tests/portage/xpak_*.rs` or module-specific fixture |

## Representative Ports Implemented Now

| Reference test | Rust port | Behavior covered |
| --- | --- | --- |
| `research/portage/lib/portage/tests/dep/test_atom.py` | `tests/portage/atom_parity.rs` | atom parsing, blockers, repo qualifiers, wildcard policy, USE deps, slots/sub-slots, intersects |
| `research/portage/lib/portage/tests/versions/test_vercmp.py` | `tests/portage/version_parity.rs` | Portage version ordering, suffix ordering, revisions, large numeric components |
| `research/portage/lib/portage/tests/resolver/test_simple.py` | `tests/portage/resolver_simple_parity.rs` | simple package selection, stable keyword filtering, --noreplace, --update, binary preference, OR dependency fallback |
| `research/portage/lib/portage/tests/emerge/test_actions.py` | `tests/portage/cli_request_parity.rs` | emerge-style option normalization and target validation |

## Full Upstream Inventory

| Status | Reference file | Rust target |
| --- | --- | --- |
| todo | `research/portage/lib/portage/tests/bin/test_dobin.py` | TBD |
| todo | `research/portage/lib/portage/tests/bin/test_dodir.py` | TBD |
| todo | `research/portage/lib/portage/tests/bin/test_doins.py` | TBD |
| todo | `research/portage/lib/portage/tests/bin/test_filter_bash_env.py` | TBD |
| todo | `research/portage/lib/portage/tests/bin/test_ver_funcs.py` | TBD |
| todo | `research/portage/lib/portage/tests/dbapi/test_auxdb.py` | TBD |
| todo | `research/portage/lib/portage/tests/dbapi/test_bintree.py` | TBD |
| todo | `research/portage/lib/portage/tests/dbapi/test_bintree_build_id.py` | TBD |
| todo | `research/portage/lib/portage/tests/dbapi/test_fakedbapi.py` | TBD |
| todo | `research/portage/lib/portage/tests/dbapi/test_portdb_cache.py` | TBD |
| ported-representative | `research/portage/lib/portage/tests/dep/test_atom.py` | `tests/portage/atom_parity.rs` |
| todo | `research/portage/lib/portage/tests/dep/test_best_match_to_list.py` | TBD |
| todo | `research/portage/lib/portage/tests/dep/test_check_required_use.py` | TBD |
| todo | `research/portage/lib/portage/tests/dep/test_dep_getcpv.py` | TBD |
| todo | `research/portage/lib/portage/tests/dep/test_dep_getrepo.py` | TBD |
| todo | `research/portage/lib/portage/tests/dep/test_dep_getslot.py` | TBD |
| todo | `research/portage/lib/portage/tests/dep/test_dep_getusedeps.py` | TBD |
| todo | `research/portage/lib/portage/tests/dep/test_dnf_convert.py` | TBD |
| todo | `research/portage/lib/portage/tests/dep/test_extended_atom_dict.py` | TBD |
| todo | `research/portage/lib/portage/tests/dep/test_extract_affecting_use.py` | TBD |
| todo | `research/portage/lib/portage/tests/dep/test_get_operator.py` | TBD |
| todo | `research/portage/lib/portage/tests/dep/test_get_required_use_flags.py` | TBD |
| todo | `research/portage/lib/portage/tests/dep/test_isjustname.py` | TBD |
| todo | `research/portage/lib/portage/tests/dep/test_isvalidatom.py` | TBD |
| todo | `research/portage/lib/portage/tests/dep/test_libc.py` | TBD |
| todo | `research/portage/lib/portage/tests/dep/test_match_from_list.py` | TBD |
| todo | `research/portage/lib/portage/tests/dep/test_overlap_dnf.py` | TBD |
| todo | `research/portage/lib/portage/tests/dep/test_paren_reduce.py` | TBD |
| todo | `research/portage/lib/portage/tests/dep/test_soname_atom_pickle.py` | TBD |
| todo | `research/portage/lib/portage/tests/dep/test_standalone.py` | TBD |
| todo | `research/portage/lib/portage/tests/dep/test_use_reduce.py` | TBD |
| todo | `research/portage/lib/portage/tests/ebuild/test_array_fromfile_eof.py` | TBD |
| todo | `research/portage/lib/portage/tests/ebuild/test_config.py` | TBD |
| todo | `research/portage/lib/portage/tests/ebuild/test_doebuild_fd_pipes.py` | TBD |
| todo | `research/portage/lib/portage/tests/ebuild/test_doebuild_spawn.py` | TBD |
| todo | `research/portage/lib/portage/tests/ebuild/test_fetch.py` | TBD |
| todo | `research/portage/lib/portage/tests/ebuild/test_ipc_daemon.py` | TBD |
| todo | `research/portage/lib/portage/tests/ebuild/test_spawn.py` | TBD |
| todo | `research/portage/lib/portage/tests/ebuild/test_use_expand_incremental.py` | TBD |
| todo | `research/portage/lib/portage/tests/emaint/test_emaint_binhost.py` | TBD |
| todo | `research/portage/lib/portage/tests/emaint/test_emaint_world.py` | TBD |
| ported-representative | `research/portage/lib/portage/tests/emerge/test_actions.py` | `tests/portage/cli_request_parity.rs` |
| todo | `research/portage/lib/portage/tests/emerge/test_baseline.py` | TBD |
| todo | `research/portage/lib/portage/tests/emerge/test_binpkg_fetch.py` | TBD |
| todo | `research/portage/lib/portage/tests/emerge/test_config_protect.py` | TBD |
| todo | `research/portage/lib/portage/tests/emerge/test_emerge_blocker_file_collision.py` | TBD |
| todo | `research/portage/lib/portage/tests/emerge/test_emerge_slot_abi.py` | TBD |
| todo | `research/portage/lib/portage/tests/emerge/test_global_updates.py` | TBD |
| todo | `research/portage/lib/portage/tests/emerge/test_libc_dep_inject.py` | TBD |
| todo | `research/portage/lib/portage/tests/env/config/test_PackageKeywordsFile.py` | TBD |
| todo | `research/portage/lib/portage/tests/env/config/test_PackageMaskFile.py` | TBD |
| todo | `research/portage/lib/portage/tests/env/config/test_PackageUseFile.py` | TBD |
| todo | `research/portage/lib/portage/tests/env/config/test_PortageModulesFile.py` | TBD |
| todo | `research/portage/lib/portage/tests/glsa/test_security_set.py` | TBD |
| todo | `research/portage/lib/portage/tests/gpkg/test_gpkg_checksum.py` | TBD |
| todo | `research/portage/lib/portage/tests/gpkg/test_gpkg_gpg.py` | TBD |
| todo | `research/portage/lib/portage/tests/gpkg/test_gpkg_gpg_emerge.py` | TBD |
| todo | `research/portage/lib/portage/tests/gpkg/test_gpkg_metadata_update.py` | TBD |
| todo | `research/portage/lib/portage/tests/gpkg/test_gpkg_metadata_url.py` | TBD |
| todo | `research/portage/lib/portage/tests/gpkg/test_gpkg_path.py` | TBD |
| todo | `research/portage/lib/portage/tests/gpkg/test_gpkg_size.py` | TBD |
| todo | `research/portage/lib/portage/tests/gpkg/test_gpkg_stream.py` | TBD |
| todo | `research/portage/lib/portage/tests/lafilefixer/test_lafilefixer.py` | TBD |
| todo | `research/portage/lib/portage/tests/lazyimport/test_lazy_import_portage_baseline.py` | TBD |
| todo | `research/portage/lib/portage/tests/lint/test_bash_syntax.py` | TBD |
| todo | `research/portage/lib/portage/tests/lint/test_compile_modules.py` | TBD |
| todo | `research/portage/lib/portage/tests/lint/test_import_modules.py` | TBD |
| todo | `research/portage/lib/portage/tests/locks/test_asynchronous_lock.py` | TBD |
| todo | `research/portage/lib/portage/tests/locks/test_lock_nonblock.py` | TBD |
| todo | `research/portage/lib/portage/tests/news/test_NewsItem.py` | TBD |
| todo | `research/portage/lib/portage/tests/process/test_AsyncFunction.py` | TBD |
| todo | `research/portage/lib/portage/tests/process/test_ForkProcess.py` | TBD |
| todo | `research/portage/lib/portage/tests/process/test_PipeLogger.py` | TBD |
| todo | `research/portage/lib/portage/tests/process/test_PopenProcess.py` | TBD |
| todo | `research/portage/lib/portage/tests/process/test_PopenProcessBlockingIO.py` | TBD |
| todo | `research/portage/lib/portage/tests/process/test_pickle.py` | TBD |
| todo | `research/portage/lib/portage/tests/process/test_poll.py` | TBD |
| todo | `research/portage/lib/portage/tests/process/test_spawn_fail_e2big.py` | TBD |
| todo | `research/portage/lib/portage/tests/process/test_spawn_returnproc.py` | TBD |
| todo | `research/portage/lib/portage/tests/process/test_spawn_warn_large_env.py` | TBD |
| todo | `research/portage/lib/portage/tests/process/test_unshare_net.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/binpkg_multi_instance/test_build_id_profile_format.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/binpkg_multi_instance/test_rebuilt_binaries.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/soname/test_autounmask.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/soname/test_depclean.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/soname/test_downgrade.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/soname/test_or_choices.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/soname/test_reinstall.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/soname/test_skip_update.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/soname/test_slot_conflict_reinstall.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/soname/test_slot_conflict_update.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/soname/test_soname_provided.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/soname/test_unsatisfiable.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/soname/test_unsatisfied.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_aggressive_backtrack_downgrade.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_alternatives_gzip.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_autounmask.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_autounmask_binpkg_use.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_autounmask_keep_keywords.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_autounmask_multilib_use.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_autounmask_parent.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_autounmask_use_backtrack.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_autounmask_use_breakage.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_autounmask_use_slot_conflict.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_backtracking.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_bdeps.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_binary_pkg_ebuild_visibility.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_binpackage_downgrades_slot_dep.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_binpackage_selection.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_blocker.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_bootstrap_deps.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_broken_deps.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_buildpkg.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_changed_deps.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_circular_choices.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_circular_choices_rust.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_circular_dependencies.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_complete_graph.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_complete_if_new_subslot_without_revbump.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_cross_dep_priority.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_depclean.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_depclean_order.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_depclean_slot_unavailable.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_depth.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_disjunctive_depend_order.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_eapi.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_emptytree_reinstall_unsatisfiability.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_features_test_use.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_imagemagick_graphicsmagick.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_installkernel.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_keywords.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_merge_order.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_missed_update.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_missing_iuse_and_evaluated_atoms.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_multirepo.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_multislot.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_old_dep_chain_display.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_onlydeps.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_onlydeps_circular.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_onlydeps_ideps.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_onlydeps_minimal.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_or_choices.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_or_downgrade_installed.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_or_upgrade_installed.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_output.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_package_tracker.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_perl_rebuild_bug.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_profile_default_eapi.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_profile_package_set.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_profile_use_stable.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_rebuild.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_rebuild_ghostscript.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_regular_slot_change_without_revbump.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_required_use.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_runtime_cycle_merge_order.py` | TBD |
| ported-representative | `research/portage/lib/portage/tests/resolver/test_simple.py` | `tests/portage/resolver_simple_parity.rs` |
| todo | `research/portage/lib/portage/tests/resolver/test_slot_abi.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_slot_abi_downgrade.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_slot_change_without_revbump.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_slot_collisions.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_slot_conflict_blocked_prune.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_slot_conflict_force_rebuild.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_slot_conflict_mask_update.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_slot_conflict_rebuild.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_slot_conflict_unsatisfied_deep_deps.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_slot_conflict_update.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_slot_conflict_update_virt.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_slot_operator_autounmask.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_slot_operator_bdeps.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_slot_operator_complete_graph.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_slot_operator_exclusive_slots.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_slot_operator_missed_update.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_slot_operator_rebuild.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_slot_operator_required_use.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_slot_operator_reverse_deps.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_slot_operator_runtime_pkg_mask.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_slot_operator_unsatisfied.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_slot_operator_unsolved.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_slot_operator_update_probe_parent_downgrade.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_solve_non_slot_operator_slot_conflicts.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_tar_merge_order.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_targetroot.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_unmerge_order.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_unnecessary_slot_upgrade.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_update.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_use_dep_defaults.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_useflags.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_virtual_cycle.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_virtual_minimize_children.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_virtual_slot.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_with_test_deps.py` | TBD |
| todo | `research/portage/lib/portage/tests/resolver/test_world_warning.py` | TBD |
| todo | `research/portage/lib/portage/tests/sets/base/test_internal_package_set.py` | TBD |
| todo | `research/portage/lib/portage/tests/sets/base/test_variable_set.py` | TBD |
| todo | `research/portage/lib/portage/tests/sets/base/test_wildcard_package_set.py` | TBD |
| todo | `research/portage/lib/portage/tests/sets/files/test_config_file_set.py` | TBD |
| todo | `research/portage/lib/portage/tests/sets/files/test_static_file_set.py` | TBD |
| todo | `research/portage/lib/portage/tests/sets/shell/test_shell.py` | TBD |
| todo | `research/portage/lib/portage/tests/sync/test_sync_local.py` | TBD |
| todo | `research/portage/lib/portage/tests/sync/test_sync_zipfile.py` | TBD |
| todo | `research/portage/lib/portage/tests/unicode/test_string_format.py` | TBD |
| todo | `research/portage/lib/portage/tests/update/test_move_ent.py` | TBD |
| todo | `research/portage/lib/portage/tests/update/test_move_slot_ent.py` | TBD |
| todo | `research/portage/lib/portage/tests/update/test_update_dbentry.py` | TBD |
| todo | `research/portage/lib/portage/tests/util/dyn_libs/test_installed_dynlibs.py` | TBD |
| todo | `research/portage/lib/portage/tests/util/dyn_libs/test_soname_deps.py` | TBD |
| todo | `research/portage/lib/portage/tests/util/eventloop/test_call_soon_fifo.py` | TBD |
| todo | `research/portage/lib/portage/tests/util/file_copy/test_copyfile.py` | TBD |
| todo | `research/portage/lib/portage/tests/util/futures/asyncio/test_event_loop_in_fork.py` | TBD |
| todo | `research/portage/lib/portage/tests/util/futures/asyncio/test_pipe_closed.py` | TBD |
| todo | `research/portage/lib/portage/tests/util/futures/asyncio/test_run_until_complete.py` | TBD |
| todo | `research/portage/lib/portage/tests/util/futures/asyncio/test_subprocess_exec.py` | TBD |
| todo | `research/portage/lib/portage/tests/util/futures/asyncio/test_wakeup_fd_sigchld.py` | TBD |
| todo | `research/portage/lib/portage/tests/util/futures/test_done_callback.py` | TBD |
| todo | `research/portage/lib/portage/tests/util/futures/test_done_callback_after_exit.py` | TBD |
| todo | `research/portage/lib/portage/tests/util/futures/test_iter_completed.py` | TBD |
| todo | `research/portage/lib/portage/tests/util/futures/test_retry.py` | TBD |
| todo | `research/portage/lib/portage/tests/util/test_atomic_ofstream.py` | TBD |
| todo | `research/portage/lib/portage/tests/util/test_checksum.py` | TBD |
| todo | `research/portage/lib/portage/tests/util/test_digraph.py` | TBD |
| todo | `research/portage/lib/portage/tests/util/test_file_copier.py` | TBD |
| todo | `research/portage/lib/portage/tests/util/test_getconfig.py` | TBD |
| todo | `research/portage/lib/portage/tests/util/test_grabdict.py` | TBD |
| todo | `research/portage/lib/portage/tests/util/test_install_mask.py` | TBD |
| todo | `research/portage/lib/portage/tests/util/test_manifest.py` | TBD |
| todo | `research/portage/lib/portage/tests/util/test_mtimedb.py` | TBD |
| todo | `research/portage/lib/portage/tests/util/test_normalizedPath.py` | TBD |
| todo | `research/portage/lib/portage/tests/util/test_shelve.py` | TBD |
| todo | `research/portage/lib/portage/tests/util/test_socks5.py` | TBD |
| todo | `research/portage/lib/portage/tests/util/test_stackDictList.py` | TBD |
| todo | `research/portage/lib/portage/tests/util/test_stackDicts.py` | TBD |
| todo | `research/portage/lib/portage/tests/util/test_stackLists.py` | TBD |
| todo | `research/portage/lib/portage/tests/util/test_uniqueArray.py` | TBD |
| todo | `research/portage/lib/portage/tests/util/test_varExpand.py` | TBD |
| todo | `research/portage/lib/portage/tests/util/test_whirlpool.py` | TBD |
| todo | `research/portage/lib/portage/tests/util/test_xattr.py` | TBD |
| todo | `research/portage/lib/portage/tests/versions/test_cpv_sort_key.py` | TBD |
| ported-representative | `research/portage/lib/portage/tests/versions/test_vercmp.py` | `tests/portage/version_parity.rs` |
| todo | `research/portage/lib/portage/tests/xpak/test_decodeint.py` | TBD |
