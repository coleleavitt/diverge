# Audit diverge

Use this command to audit the Rust rewrite against the Gentoo Portage emerge reference.

User scope: `$ARGUMENTS`

## Procedure

1. Identify the Rust modules relevant to the requested scope.
2. Identify the matching Portage reference files under `research/portage/`.
3. Compare observable behavior, not Python object layout or shell control flow.
4. Check CLI parsing, config loading, atom/version parsing, USE flag evaluation, dependency expression handling, resolver/backtracking behavior, blockers, binary package selection, scheduler ordering, package operations, world/set updates, output, and tests as applicable.
5. Look for security issues, missing error cases, panics in package-manager code, shell injection, path traversal, unsafe filesystem mutation, missing rollback boundaries, secret leaks, and unstable public API choices.
6. Run the narrowest relevant tests, then broader workspace tests when feasible.

## Output

Lead with findings ordered by severity. Include file and line references. If no issues are found, say so and list any remaining test gaps or unverified assumptions.

Do not make edits unless the user explicitly asks for fixes.
