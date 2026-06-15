# Portage Parity Audit

Use this skill when auditing `diverge` behavior against the Gentoo Portage reference checkout in `research/portage/`.

Do not use this for normal formatting-only review.

## Inputs

Ask for or infer:

- The behavior area: CLI action, config/profile loading, atom/version parsing, dependency resolution, USE/EAPI handling, fetch/build/merge/unmerge, binary package behavior, world/set handling, output, or error reporting.
- The Rust modules or tests changed.
- The relevant Portage reference files under `research/portage/`.
- Whether the audit is correctness-focused, security-focused, compatibility-focused, or test-coverage-focused.

## Workflow

1. Identify the Rust code path and tests for the behavior.
2. Use Codegraph first for Rust and Portage Python/bash symbols; use focused reads for surrounding context that Codegraph does not include.
3. Trace the Portage behavior from the reference entrypoint when needed:
   - `research/portage/bin/emerge`
   - `research/portage/lib/_emerge/main.py`
   - `research/portage/lib/_emerge/actions.py`
   - `research/portage/lib/_emerge/create_depgraph_params.py`
   - `research/portage/lib/_emerge/depgraph.py`
   - `research/portage/lib/_emerge/Scheduler.py`
   - `research/portage/lib/portage/package/ebuild/config.py`
   - `research/portage/lib/portage/eapi.py`
   - `research/portage/bin/ebuild.sh`
4. Compare observable behavior: inputs, outputs, exit codes, selected packages, dependency graph, world/set changes, filesystem operations, logs, and failure modes.
5. Check for fixture coverage and integration tests, not only unit parser tests.
6. Check safety boundaries for filesystem writes, process execution, network access, archive extraction, permissions, and resume/rollback state.
7. Confirm `docs/portage-test-inventory.md` reflects the current Rust porting status for any upstream tests touched.
8. Run focused tests first, then `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace --all-targets`, and `cargo llvm-cov --workspace --all-targets --summary-only` when feasible.

## Output

Lead with findings ordered by severity. Include exact file paths and line references. Then list:

- Portage reference files inspected.
- Rust files inspected.
- Commands run and their results.
- Missing tests or unverified assumptions.

Do not modify files unless the user asks for fixes.
