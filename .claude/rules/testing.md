# Testing Rules

These rules always apply.

- Add tests for every supported user-visible emerge behavior and every parser/resolver/executor semantic added.
- Add integration tests that exercise multiple implemented features together. Avoid accepting isolated parser examples that break when combined.
- Keep small Portage-like fixture repositories for resolver, config, USE, EAPI, binary package, and merge/unmerge behavior. Do not depend on the user's host Gentoo state.
- Test atom parsing, version comparison, slot/sub-slot handling, blocker syntax, dependency expression normalization, USE condition evaluation, REQUIRED_USE, masks, keywords, licenses, and package set expansion.
- Test CLI actions and flags against expected emerge-compatible output, exit codes, prompts, and failure messages.
- Test resolver behavior for backtracking, conflicts, blockers, virtuals, rebuild decisions, binary/source selection, installed packages, world updates, and deterministic ordering.
- Test scheduler and executor behavior with isolated roots: fetch-only, build, test, install image, merge, unmerge, collision handling, protected files, interruption, and resume where implemented.
- Use property tests for atom/version parsing, dependency expression round trips, USE flag condition evaluation, path normalization, and resolver invariants.
- Use golden tests for CLI help, planned operation output, resolver explanations, and failure reports.
- Never let tests mutate `/`, `/etc/portage`, `/var/db/pkg`, real distdirs, or the user's world file. Use tempdirs and fixture roots.
- Keep `docs/portage-test-inventory.md` current when porting upstream Portage tests; mark rows as ported only after the Rust test exists and passes.
- Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace --all-targets`, and `cargo llvm-cov --workspace --all-targets --summary-only` before finalizing implementation work. If a command cannot run, record the reason.
