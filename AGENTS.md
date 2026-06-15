# diverge Agent Instructions

This file exists for compatibility with agents that read `AGENTS.md`.

For the primary project instructions, read `CLAUDE.md` first. The goal is to rewrite Gentoo Portage's `emerge` package manager into idiomatic Rust in this repository.

## Project Goal

- Build `diverge`, a Rust rewrite of the `emerge` package manager and the Portage behavior it depends on.
- Preserve emerge's user-facing semantics in Rust terms: CLI compatibility, dependency resolution, USE flag handling, package selection, fetch/build/merge/unmerge flows, binary package support, world/set handling, configuration parsing, logging, and safe filesystem transactions.
- Do not mechanically transliterate Portage's Python classes or shell scripts when an idiomatic Rust design gives the same observable behavior.
- Do not reduce this project to a metadata parser. One coherent package-management model should drive resolution, scheduling, package operations, CLI behavior, and tests where practical.
- Avoid god objects. Keep CLI parsing, configuration, ebuild/EAPI semantics, dependency atoms, repository metadata, resolver state, scheduler tasks, filesystem mutations, and reporting in focused modules.

## Reference Material

- `research/portage/` contains the Gentoo Portage reference checkout.
- `research/portage/` is ignored by git and should be treated as read-only research input.
- Start with:
  - `research/portage/bin/emerge`
  - `research/portage/lib/_emerge/main.py`
  - `research/portage/lib/_emerge/actions.py`
  - `research/portage/lib/_emerge/depgraph.py`
  - `research/portage/lib/_emerge/Scheduler.py`
  - `research/portage/lib/_emerge/Package.py`
  - `research/portage/lib/portage/package/ebuild/config.py`
  - `research/portage/lib/portage/eapi.py`
  - `research/portage/bin/ebuild.sh`
  - `research/portage/bin/phase-functions.sh`

## Working Rules

- Do not edit `research/portage/` unless explicitly asked to refresh the reference checkout.
- Before implementing behavior, identify the matching Portage reference file and the observable behavior to preserve.
- Add tests for CLI parsing, config loading, atom/version parsing, USE/dependency semantics, resolver decisions, scheduler ordering, fetch/build/merge/unmerge behavior, binary package behavior, and error reporting as those features are implemented.
- Maintain `docs/portage-test-inventory.md` as the durable map from upstream Portage Python tests to Rust ports.
- Add integration tests that combine features; isolated parser examples are not enough.
- Keep Rust APIs idiomatic, documented, and hard to misuse.
- Prefer structured parsing, typed IDs, typed errors, explicit transaction plans, and deterministic output over stringly typed internals.
- Treat build scripts, ebuild metadata, repository files, installed package databases, and filesystem paths as untrusted inputs unless created by this process.

## Verification

When Rust source exists, run:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets
cargo llvm-cov --workspace --all-targets --summary-only
```

If these commands cannot run because the workspace is not scaffolded yet, report that directly.
