# Code Style Rules

These rules always apply.

- Write idiomatic Rust first. Preserve emerge's observable behavior, not Portage's Python object layout or shell control flow.
- Keep modules small and named around behavior: CLI, atoms, versions, deps, USE, EAPI, config, repository, resolver, scheduler, executor, transactions, and reporting.
- Do not create god objects. Split config roots, repositories, package metadata, resolver state, scheduler plans, process execution, filesystem transactions, and output formatting into separate types/modules with explicit composition.
- Do not add feature-local parsing or state that bypasses the shared domain model.
- Prefer explicit data types over stringly typed internal protocols: atoms, package IDs, versions, slots, USE flags, EAPI values, dependency expressions, repository IDs, roots, and operation plans.
- Use `Result` and typed errors. Avoid panics in package-manager code except for impossible internal invariant violations that are documented and tested.
- Make filesystem ownership explicit. Types that can write to disk should carry or receive their root, config root, build directory, or transaction context explicitly.
- Keep public traits coherent and hard to misuse. Use sealed traits when external implementations would break resolver or transaction invariants.
- Avoid macros until normal Rust APIs prove too noisy. If a macro is added, keep the expanded model documented and test it with compile tests where practical.
- Make feature flags additive. Default features should be useful but should not pull in every backend or perform network/root operations.
- Keep examples short, compiling, and realistic.
- Use rustdoc on public APIs that define package identities, dependency expressions, resolver inputs/outputs, operation plans, and error behavior.
- Format with `cargo fmt --all`; do not hand-align code that rustfmt will rewrite.
