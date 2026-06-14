# diverge Claude Instructions

## Mission

Rewrite Gentoo Portage's `emerge` package manager into a Rust project named `diverge`.

The Portage reference checkout lives at `research/portage/`. Treat it as read-only research input and do not commit it. The Rust implementation should be idiomatic Rust that preserves emerge's observable behavior: command-line interface, package and set selection, dependency resolution, USE flag and EAPI semantics, fetch/build/merge/unmerge flows, binary package behavior, world file updates, configuration handling, logging, and failure reporting.

This project is not meant to become only an ebuild parser or a thin wrapper around existing Portage commands. The central value is a coherent Rust package manager model where the same parsed metadata and dependency model drive resolver decisions, scheduler tasks, package operations, CLI output, and tests where practical.

If the project needs to grow beyond the current single binary crate, prefer a workspace that can grow in layers:

- `diverge-core`: atoms, versions, package IDs, dependency expressions, USE flags, EAPI feature data, errors, and shared domain types.
- `diverge-ebuild`: ebuild metadata parsing, EAPI helpers, eclass/profile interaction, phase environment modeling, and shell-boundary adapters.
- `diverge-repository`: repository layout, metadata/cache reads, manifests, binrepos, installed package database views, and config/profile loading.
- `diverge-resolver`: package selection, depgraph construction, blocker handling, backtracking, slot/sub-slot semantics, rebuild decisions, and world/set expansion.
- `diverge-executor`: fetch, unpack, compile, test, install image, merge, unmerge, binary package, locks, sandbox/process execution, and transaction rollback boundaries.
- `diverge-cli`: emerge-compatible argument parsing, actions, output, prompts, logging, resume state, and command dispatch.

Keep this as an architectural direction, not a reason to split crates before tests prove the boundaries.

## Architecture First

Do not build isolated features that only work in their own examples. Every feature must fit the same shared architecture:

- Domain layer: atoms, versions, package identities, USE/IUSE, EAPI attributes, dependency expressions, masks, keywords, licenses, slots, repositories, and installed package state.
- Interpretation layer: CLI actions, dependency graph/resolver, scheduler plans, package operation plans, reports, and tests.
- Runtime layer: filesystem access, config roots, repository databases, process spawning, locks, fetchers, build directories, merge/unmerge transactions, logging, and signal handling.
- Adapter layer: platform-specific process/sandbox behavior, network fetch backends, terminal UI, binary package formats, and compatibility shims.

Before adding a feature, decide which layer it belongs to and which existing traits/types it composes with. If a feature needs mutable state or filesystem writes, define the ownership and rollback boundary before coding.

Avoid god objects. Do not let one `App`, `Config`, `Package`, `Resolver`, `Scheduler`, or `Transaction` struct accumulate CLI parsing, repository reads, dependency solving, task scheduling, process execution, filesystem mutation, and output formatting. Split responsibilities into focused modules with explicit data flow.

## Scope Boundaries

Build for Gentoo users and developers who need emerge-compatible package management semantics in Rust. Do not optimize the first version for:

- Replacing every Portage utility at once.
- A new package manager UX that ignores emerge compatibility.
- A macro-heavy DSL before the parser and resolver model are proven.
- A full shell interpreter for arbitrary ebuild logic beyond the boundaries needed for safe phase execution.
- Browser UI, dashboards, daemon mode, remote orchestration, or deployment tooling.
- Silent behavior changes from Portage without tests and documentation.

Initial priority is the smallest end-to-end slice: parse an emerge-style request, load enough config/repository metadata for a controlled test fixture, build a dependency plan, render the planned operations, and test that the result matches the Portage reference expectation.

## Reference Map

Start research from these files:

- `research/portage/bin/emerge`: executable entrypoint, signal handling, error handling, and dispatch into `_emerge.main.emerge_main`.
- `research/portage/lib/_emerge/main.py`: CLI option/action parsing and top-level emerge flow; `emerge_main` is indexed at line 1187.
- `research/portage/lib/_emerge/actions.py`: action dispatch for sync, search, depclean, info, list, config, and merge-oriented flows.
- `research/portage/lib/_emerge/create_depgraph_params.py`: conversion from command options/actions into dependency graph parameters.
- `research/portage/lib/_emerge/depgraph.py`: dependency graph, package selection, conflict/backtracking behavior, graph display, and resume helpers; `depgraph` is indexed at line 658.
- `research/portage/lib/_emerge/Scheduler.py`: merge/build scheduler and task orchestration; `Scheduler` is indexed at line 69 and `merge` at line 1135.
- `research/portage/lib/_emerge/Package.py`: package task model and package metadata view; `Package` is indexed at line 25.
- `research/portage/lib/_emerge/EbuildBuild.py`, `EbuildPhase.py`, `EbuildProcess.py`, `EbuildMerge.py`, `PackageMerge.py`, and `PackageUninstall.py`: ebuild build, phase execution, merge, and uninstall task flow.
- `research/portage/lib/_emerge/RootConfig.py`: per-root configuration model used by emerge.
- `research/portage/lib/portage/package/ebuild/config.py`: Portage config/profile/environment loading and package-specific settings.
- `research/portage/lib/portage/eapi.py` and `research/portage/bin/eapi.sh`: EAPI feature gates and shell-visible behavior.
- `research/portage/bin/ebuild.sh`, `phase-functions.sh`, `phase-helpers.sh`, `misc-functions.sh`, and `isolated-functions.sh`: shell phase helpers and ebuild execution contract.
- `research/portage/lib/portage/tests/`: reference test ideas for resolver, emerge actions, config, and ebuild behavior.

Codegraph has been initialized for this repo and indexes both the current Rust source and supported Python/bash files in `research/portage/`. Use Codegraph first for Portage symbols and Rust symbols, then focused reads for exact surrounding context.

## Design Principles

Do not port Python object structure or shell control flow mechanically. Preserve the developer- and user-facing guarantees in Rust terms:

- Make package identities, dependency atoms, version constraints, slots, USE flags, EAPI attributes, roots, repositories, and operation plans explicit typed values.
- Keep dependency resolution deterministic, explainable, and testable.
- Keep CLI parsing, resolver inputs, scheduler plans, package operation execution, and output tied to the same domain model instead of duplicating string parsing.
- Model parse failures, invalid configs, masked packages, unsatisfied dependencies, blockers, fetch failures, build failures, merge collisions, permission failures, and interrupted transactions as structured errors.
- Prefer standard ecosystem crates for shared formats and primitives where appropriate: `camino`/`std::path`, `clap`, `serde`, `toml`, `tracing`, `thiserror` or `snafu`, `tokio` where async is justified, and well-tested hash/graph crates only when they fit the resolver design.
- Avoid global mutable state. Pass roots, repositories, environment, and execution context explicitly.
- Keep public APIs small until tests prove the shape.
- Document each intentional semantic difference from Portage emerge.

## Working Rules

- Do not edit `research/portage/` except when deliberately refreshing the reference checkout.
- Before implementing a feature, identify the Portage reference module and record the intended Rust equivalent in tests, docs, or issue notes when useful.
- Add tests with every user-visible behavior change.
- Add integration tests that combine features. A feature is not done if it only works in isolation.
- Prefer property tests for atom/version parsing, dependency expression normalization, USE condition evaluation, path normalization, and resolver invariants.
- Keep examples and CLI help output current. They are part of the compatibility contract.
- Run formatting and tests before handing work back:
  - `cargo fmt --all`
  - `cargo clippy --workspace --all-targets -- -D warnings`
  - `cargo test --workspace --all-targets`

If the workspace has not been scaffolded beyond the initial binary yet, do the smallest useful scaffold first, then add tests around the first implemented slice.
