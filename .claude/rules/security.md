# Security Rules

These rules always apply.

- Treat ebuilds, eclasses, repository metadata, manifests, distfiles, binary packages, installed package database entries, config files, environment variables, package names, versions, paths, and command-line arguments as untrusted unless created by this process.
- Never construct shell commands with string concatenation. Use structured process spawning, explicit argument arrays, controlled environments, and documented shell-boundary adapters for ebuild phase execution.
- Keep build-time environment construction explicit. Redact secrets and host-specific credentials from logs, errors, debug output, and saved resume state.
- Do not allow repository paths, package metadata, archives, or merge operations to escape the configured root, config root, build root, image directory, or distdir. Normalize and validate paths at boundaries.
- Model filesystem mutations as plans or transactions with clear ownership, collision checks, rollback boundaries, and permission/error reporting.
- Do not perform root-level writes, unmerge operations, or host package database mutations in tests unless the test uses an isolated fixture root.
- Verify manifests, checksums, binary package signatures, and fetched artifacts according to the behavior being implemented; never silently skip integrity checks.
- Bound file reads, archive extraction, subprocess output, dependency expansion, and resolver backtracking. Avoid unbounded memory growth on malicious repository input.
- Preserve symlink, hardlink, directory, device node, and permission semantics intentionally. Add tests for traversal, collisions, protected paths, and non-UTF-8 paths where relevant.
- Keep network behavior explicit and configurable. Tests should not make external network calls unless they are explicitly marked integration tests and can be skipped.
- Treat interrupted transactions as first-class failure paths. Preserve enough state to report what happened and what is safe to resume or roll back.
