# Reproduce A diverge Issue

Use this command to turn a bug report or suspected Portage parity gap into a minimal reproduction.

User report: `$ARGUMENTS`

## Procedure

1. Restate the observed behavior and expected emerge-compatible behavior in concrete terms.
2. Locate the Rust code path and the matching Portage reference path under `research/portage/`.
3. Create or identify the smallest failing test or fixture.
4. Run the failing test and capture the important output.
5. Fix only the behavior needed for the reproduction.
6. Re-run the focused test and any nearby tests.

## Output

Report the failing case, the fix, and the verification commands. Include remaining uncertainty if the behavior intentionally differs from Portage emerge.
