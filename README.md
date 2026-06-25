# diverge

A from-scratch **Rust rewrite of Gentoo Portage's `emerge`** package manager.

`diverge` re-implements emerge's *observable behavior* — the command-line
interface, atom/version semantics, USE flags, EAPI gates, dependency
resolution, and the fetch/build/merge/unmerge flows — in idiomatic, typed Rust.
It is **not** a thin wrapper around Portage or a bare ebuild parser: one
coherent domain model drives the resolver, scheduler, package operations, CLI
output, and tests.

> Status: early but functional. The CLI surface and every action *execute*
> (resolve, sync, regen, config, merge, unmerge, prune, clean, news), validated
> against emerge's own behavior. Real package *compilation* on a live system is
> the remaining frontier — see [Status & scope](#status--scope).

## About

Portage is Gentoo's source-based package manager; `emerge` is its CLI. The
[Package Manager Specification (PMS)](https://dev.gentoo.org/~ulm/pms/head/pms.html)
standardizes the ebuild behavior `diverge` targets.

This project preserves emerge's *guarantees* in Rust terms rather than
transliterating Portage's Python classes or shell scripts:

- package identities, atoms, version constraints, slots/sub-slots, USE/IUSE,
  EAPI attributes, repositories, and installed state are explicit typed values;
- dependency resolution is deterministic, explainable, and testable;
- CLI parsing, resolver inputs, scheduler plans, and filesystem transactions all
  share the same domain model instead of duplicating string parsing;
- parse failures, masks, unsatisfied deps, blockers, collisions, and interrupted
  transactions are modeled as structured errors.

## Quick start

```sh
git clone https://github.com/coleleavitt/diverge.git
cd diverge
cargo build --release

# emerge-compatible usage banner (byte-identical to `emerge`), exit 1
./target/release/diverge

# pretend-plan a package against the host config (read-only)
./target/release/diverge -p www-client/firefox

# search, info, news
./target/release/diverge -s firefox
./target/release/diverge --info
./target/release/diverge --check-news
```

`diverge` reads `make.conf`, `repos.conf`, the active `make.profile`, and the
installed package database from the roots given by `PORTAGE_CONFIGROOT` and
`ROOT` (defaulting to `/`). Point them at an isolated tree to experiment safely:

```sh
PORTAGE_CONFIGROOT=/path/to/fixture ROOT=/path/to/fixture \
  ./target/release/diverge -p app-misc/hello
```

**Safety:** every mutating action (real merge, `--unmerge`, `--prune`,
`--clean`) refuses to modify `ROOT=/` unless `DIVERGE_ALLOW_ROOT` is set, so a
run can never accidentally touch your live system.

## CLI / action support

| Area | Status |
| --- | --- |
| Usage banner + color (`--color y\|n`), exit codes, full option/short-flag table | ✅ matches emerge (banner byte-identical) |
| `--pretend`/`-p`, `--search`/`-s`/`-S`, `--info`, `--list-sets`, `--version`, `--moo` | ✅ |
| `--sync` (repos.conf + injectable backend), `--metadata`/`--regen` (md5-cache) | ✅ |
| `--config` (`pkg_config` phase), `--depclean` (preview), `--prune`, `--clean`/`--rage-clean`, `--check-news` | ✅ |
| real merge + `--unmerge`/`-C` (build → image → merge → VDB → world), host-root gated | ✅ |

Every dispatched action executes — there is no "not yet implemented" arm.

## Architecture

Single crate, layered into focused modules (the layering mirrors the eventual
`diverge-core` / `-repository` / `-resolver` / `-executor` / `-cli` workspace
split, kept single-crate until the boundaries are proven by tests):

- **Domain** — `atom`, `version`, `dep`, `matching` (atoms, `vercmp`,
  `use_reduce`, `match_from_list`/`best_match_to_list`, `REQUIRED_USE`).
- **Config** — `config` (`getconfig`/`varexpand` shell parser), `util`,
  `profile` (parent-chain stacking).
- **Repository** — `dbapi` (`fakedbapi`-style `PackageDb`), `repository`
  (ebuild-tree loader, **prefers eclass-resolved `md5-cache`**), `manifest`
  (BLAKE2B/SHA512/SHA256/SHA1/MD5 verification), `vardb` (installed DB).
- **Resolver** — `depgraph` (recursive deps, `\|\| ()` choice minimization,
  virtuals, slot-operator rebuilds, backtracking, autounmask, depclean,
  `--update/--deep/--newuse`).
- **Executor** — `executor::{merge, unmerge, config_protect, phase, spawn,
  fetch, scheduler, ebuild_sh}` (CONFIG_PROTECT, image→root merge with collision
  detection, fixed-argv phase spawning, distfile fetch, build scheduling, and a
  bundled `ebuild.sh` install-helper library that runs a real `src_install`).
- **Interpretation / adapters** — `cli`, `color`, `session` (end-to-end glue),
  `sets`, `update`, `sync`, `news`, `xpak`, `gpkg`.

## Status & scope

What works end-to-end (all tested against isolated temp roots — never the host):
CLI parsing, config/profile/md5-cache loading, dependency resolution, sync,
cache regen, `pkg_config`, prune/clean, news relevance, and a real
install-phase build → merge → VDB → world update.

What still genuinely requires a live Gentoo build environment and is **not**
done:

- **Real `src_compile`/`src_configure`** of upstream software — needs the system
  compiler, autotools/make, the full upstream `ebuild.sh`/eclass shell library,
  and a sandbox. (The *install*-phase helper contract is shipped and runs.)
- **eclass sourcing for uncached overlays** (cached repos work via md5-cache).
- **Network sync clients** (rsync/git/webrsync) and binhost/GPG — modeled via an
  injectable `SyncBackend`; no real network client is bundled.
- Full masks/licenses/USE_EXPAND in the resolver and the remaining upstream
  resolver test cases.

These are tracked candidly in
[`docs/portage-test-inventory.md`](docs/portage-test-inventory.md), and the
architecture (injectable spawner/fetcher/sync, explicit roots) is built so they
drop in.

## Testing

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets        # 764 tests across 52 suites
cargo llvm-cov --workspace --summary-only    # ~95% line coverage
```

Tests assert the literal expected values embedded in upstream Portage's own test
files. An **interop differential oracle** (`tests/interop/portage_oracle.py` +
`tests/interop_differential.rs`) imports the *real* upstream Portage functions
and diffs their output against the Rust implementations — it already caught a
real `vercmp` suffix-ordering bug. See
[`docs/testing-spec.md`](docs/testing-spec.md) for the test-porting workflow.

## Relationship to Portage

The Gentoo Portage source is used only as **read-only research input** (kept
under `research/portage/`, git-ignored). `diverge` is an independent
re-implementation; intentional behavioral differences from emerge are documented
in tests and docs.

## Licensing

`diverge` is distributed under the terms of the GNU General Public License,
version 2 (see [`LICENSE`](LICENSE)) — the same license as Portage, whose
observable behavior it re-implements.

This program is distributed in the hope that it will be useful, but WITHOUT ANY
WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A
PARTICULAR PURPOSE.

## Links

- Gentoo Portage project: <https://wiki.gentoo.org/wiki/Project:Portage>
- Package Manager Specification (PMS): <https://dev.gentoo.org/~ulm/pms/head/pms.html>
- Upstream Portage: <https://github.com/gentoo/portage>
