# diverge Documentation and Testing Specification

## Goal

`diverge` is a Rust rewrite of Gentoo Portage `emerge`. Documentation, implementation, and tests must advance together. A feature is not accepted because a parser works in isolation; it is accepted when the same typed Rust model supports the relevant CLI/request, resolver or executor behavior, user-visible reporting, and tests that cite Portage reference behavior.

## Reference Inputs

- Portage checkout: `research/portage/`.
- Test inventory: `docs/portage-test-inventory.md`.
- Primary entrypoints:
  - `research/portage/bin/emerge`
  - `research/portage/lib/_emerge/main.py`
  - `research/portage/lib/_emerge/actions.py`
  - `research/portage/lib/_emerge/create_depgraph_params.py`
  - `research/portage/lib/_emerge/depgraph.py`
  - `research/portage/lib/_emerge/Scheduler.py`
  - `research/portage/lib/portage/package/ebuild/config.py`
  - `research/portage/lib/portage/eapi.py`
  - `research/portage/bin/ebuild.sh`
  - `research/portage/lib/portage/tests/`

Treat `research/portage/` as read-only. Do not copy upstream GPL test bodies verbatim into Rust tests. Port observable cases and cite the reference file path.

## Layers and Required Tests

### Domain layer

Covers atoms, versions, package IDs, slots, USE flags, EAPI attributes, dependency expressions, masks, keywords, licenses, repositories, and installed package state.

Required tests:

- Atom parsing and validation against `dep/test_atom.py` and `dep/test_isvalidatom.py`.
- Version comparison against `versions/test_vercmp.py` and `versions/test_cpv_sort_key.py`.
- Dependency expression parsing/reduction against `dep/test_use_reduce.py`, `dep/test_paren_reduce.py`, `dep/test_dnf_convert.py`, and REQUIRED_USE tests.
- Property tests for parse/render round trips and normalized dependency expressions.

### Interpretation layer

Covers emerge CLI actions, resolver inputs/outputs, scheduler plans, reports, and world/set changes.

Required tests:

- CLI option/action parsing against `emerge/test_actions.py` and `research/portage/lib/_emerge/main.py` behavior.
- Resolver fixtures against `resolver/*.py`, starting with `resolver/test_simple.py` and expanding through blocker, slot-operator, autounmask, binary package, backtracking, and world warning cases.
- Golden tests for planned operation output and failure explanations.

### Runtime layer

Covers repository/config reads, process spawning, fetch/build directories, locks, merge/unmerge transactions, resume state, logging, and signal handling.

Required tests:

- Isolated temp-root tests only; never mutate `/`, `/etc/portage`, `/var/db/pkg`, a real distdir, or the user's world file.
- Fixture repositories for fetch/build/merge/unmerge flows.
- Transaction and interruption tests for rollback/resume boundaries.
- Process tests that use structured argv/env and never shell-concatenate untrusted input.

### Adapter layer

Covers terminal output, network fetch backends, binary package formats, sandbox/process adapters, and compatibility shims.

Required tests:

- Network tests are opt-in integration tests and use local fixtures unless explicitly marked external.
- Binary package metadata/integrity tests cite `gpkg/*.py` and `xpak/*.py`.
- Sync tests cite `sync/*.py` and use isolated repositories.

## Test-Porting Workflow

1. Add or update the inventory row in `docs/portage-test-inventory.md`.
2. Read the upstream test and the production Portage code path it exercises.
3. Rewrite the case in Rust under `tests/portage/` or a focused module test. Cite the upstream path in the test name or module docs when useful.
4. Add the minimum implementation needed in the shared domain/interpreter/runtime model.
5. Run the focused Rust test.
6. Run the full verification gate.
7. Mark the inventory row as `ported-representative` or `ported-complete` only after the Rust test exists and passes.

## Coverage Requirements

The project uses `cargo llvm-cov` for coverage. Coverage is a quality gate, not a substitute for parity review.

Required commands before handing back implementation work:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets
cargo llvm-cov --workspace --all-targets --summary-only
```

Long-term coverage target: every supported user-visible behavior has Rust tests and every non-trivial branch in package-manager code is exercised by unit, integration, or fixture tests. New code should either be covered or documented as unreachable/adapter-only with a follow-up test task.

## Current Rust Parity Scaffold

Implemented now:

- `src/atom.rs`: representative Portage atom parsing and validation behavior.
- `src/version.rs`: representative Portage `vercmp` behavior.
- `src/cli.rs`: initial emerge-style request parsing.
- `src/resolver.rs`: small fixture resolver for the cases in `resolver/test_simple.py`.
- `tests/portage.rs` plus `tests/portage/*.rs`: representative Rust ports.

This scaffold is intentionally small. It establishes the porting pattern and coverage gate; it is not a claim that all Portage tests are fully ported yet.
