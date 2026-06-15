# Project Memory

- Goal: rewrite Gentoo Portage `emerge` as idiomatic Rust in `diverge`.
- Reference checkout: `research/portage/`, ignored by git and treated as read-only research input.
- Primary reference entrypoint: `research/portage/bin/emerge`, which dispatches to `_emerge.main.emerge_main`.
- Core Portage behavior references: `research/portage/lib/_emerge/main.py`, `actions.py`, `create_depgraph_params.py`, `depgraph.py`, `Scheduler.py`, `Package.py`, `RootConfig.py`, `portage/package/ebuild/config.py`, `portage/eapi.py`, and `bin/ebuild.sh`.
- Upstream test inventory: `docs/portage-test-inventory.md` records all 239 Python test files from `research/portage/lib/portage/tests` and their Rust porting status.
- Testing spec: `docs/testing-spec.md` defines the Rust test-porting workflow, fixture rules, and coverage gate.
- Current representative Rust ports: `tests/portage/atom_parity.rs`, `version_parity.rs`, `version_sort_parity.rs`, `cli_request_parity.rs`, `resolver_simple_parity.rs`, `dep_accessors_parity.rs`, and `dep_reduce_parity.rs` (30 Rust tests; 14 upstream test files ported).
- Current Rust scaffold: `src/atom.rs`, `src/version.rs`, `src/cli.rs`, `src/resolver.rs`, and `src/dep.rs`. `dep.rs` ports the domain layer of `docs/testing-spec.md`: dep accessors (get_operator/dep_getcpv/dep_getkey/dep_getslot/dep_getrepo/dep_getusedeps/isjustname), `paren_reduce`, a core subset of `use_reduce` (uselist/masklist/matchall/matchnone/excludeall/subset/is_valid_flag), and boolean `check_required_use`. Not complete Portage parity; resolver/scheduler/executor/config layers remain unbuilt.
- Porting oracle: upstream Portage test files embed their own expected values, so ported Rust cases assert those literals (not invented ones). `use_reduce` does not yet port `opconvert`/`flat`/`is_src_uri`/`token_class` modes.
- Required verification gate: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace --all-targets`, and `cargo llvm-cov --workspace --all-targets --summary-only`.
- Coverage guardrail: new user-visible behavior should add or port tests first, then satisfy the llvm-cov gate before completion. Current total line coverage ~86%.
- Domain-layer EAPI gate detail: Portage's `_get_eapi_attrs(None)` sets `empty_groups_always_true=False` but `required_use_at_most_one_of=True`; `dep.rs` mirrors this so `|| ( )` is not implicitly satisfied under the permissive default.
