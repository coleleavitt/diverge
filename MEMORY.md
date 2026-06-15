# Project Memory

- Goal: rewrite Gentoo Portage `emerge` as idiomatic Rust in `diverge`.
- Reference checkout: `research/portage/`, ignored by git and treated as read-only research input.
- Primary reference entrypoint: `research/portage/bin/emerge`, which dispatches to `_emerge.main.emerge_main`.
- Core Portage behavior references: `research/portage/lib/_emerge/main.py`, `actions.py`, `create_depgraph_params.py`, `depgraph.py`, `Scheduler.py`, `Package.py`, `RootConfig.py`, `portage/package/ebuild/config.py`, `portage/eapi.py`, and `bin/ebuild.sh`.
- Upstream test inventory: `docs/portage-test-inventory.md` records all 239 Python test files from `research/portage/lib/portage/tests` and their Rust porting status.
- Testing spec: `docs/testing-spec.md` defines the Rust test-porting workflow, fixture rules, and coverage gate.
- Current representative Rust ports: `tests/portage/atom_parity.rs`, `version_parity.rs`, `cli_request_parity.rs`, and `resolver_simple_parity.rs`.
- Current Rust scaffold: `src/atom.rs`, `src/version.rs`, `src/cli.rs`, and `src/resolver.rs` implement the first representative parity slice; they are not complete Portage parity.
- Required verification gate: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace --all-targets`, and `cargo llvm-cov --workspace --all-targets --summary-only`.
- Coverage guardrail: new user-visible behavior should add or port tests first, then satisfy the llvm-cov gate before completion.
