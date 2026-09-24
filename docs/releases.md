# Releases

Release preparation is driven by `[workspace.package].version` in `Cargo.toml`.
The workflow derives `v<version>` from that value; maintainers do not need to
create a tag manually.

## Distribution status

All crates currently set `publish = false`. The workspace provides usable Rust
library sources and native applications, but automated registry publication is
not enabled. Python and npm binding packages are not part of the release.

The repository declares MIT while its upstream engine reference uses GPLv3.
Source provenance and the resulting distribution requirements need maintainer
review before public publication. This workflow prepares a draft without
changing any existing license declaration. NNUE weights have separate terms
and are not bundled in release application archives.

## Prepare a version

1. Update the workspace version and the versions of workspace path dependencies
   together. Member crates inherit their package version. Update the root
   `Cargo.lock` and any frontend version metadata affected by the release.
2. Run `python3 scripts/check-release.py`. It checks inherited package metadata,
   internal dependency versions, the exact Rust toolchain pin, and the presence
   of the workspace lockfile.
3. Complete the [validation gates](validation.md), including reference alignment
   for engine changes. Record remaining search and strength limitations in the
   release notes; do not label an observation as complete upstream parity.
4. Merge the reviewed version change through a PR. The main-branch `CI` push run
   must succeed before release artifacts can be prepared.

`Prepare release` runs after successful main-branch CI. It verifies the workflow
identity, event, repository, branch, and conclusion, then checks out the exact
commit validated by that run. A later change on main is not substituted for the
validated revision.

## Artifacts and supported targets

| Target | Archive | Applications |
| --- | --- | --- |
| `x86_64-unknown-linux-gnu` | `.tar.gz` | UCI engine and HTTP server |
| `aarch64-apple-darwin` | `.tar.gz` | UCI engine and HTTP server |
| `x86_64-pc-windows-msvc` | `.zip` | UCI engine and HTTP server |

Builds use the committed dependency graph, pinned Rust toolchain, release
profile, and a portable target configuration. Linux binaries are built on the
workflow's Ubuntu runner and are dynamically linked GNU/Linux artifacts; they
are not a promise of compatibility with every older Linux distribution.

Each application archive contains executable files, source and model license
notices, model setup instructions, the reference pins, and `build.json` recording
the version, commit, target, compiler, and model exclusion. The draft also
includes a source-tree archive and SHA-256 checksum files. The source archive
contains Git-tracked contents; a Git LFS pointer is not a bundled NNUE network.
It is not a vendored, dependency-complete offline build environment.

The frontend and web bridge are not currently packaged in these application
archives. Build them from source with `scripts/build-web.sh`.

## Review, retry, and publication

Inspect the draft's commit, artifacts, checksums, source provenance, compatible
model information, and validation evidence. Resolve distribution requirements
before publishing it. Public release publication is a separate maintainer
action; the workflow does not execute `cargo publish` or publish model weights.

To retry an interrupted preparation, manually run `Prepare release` with
`ci_run_id` set to the successful **CI push run on main** for the intended commit.
A PR workflow run or a failed run is not accepted.

Existing version tags are immutable. A draft may be resumed at its original
verified commit. The workflow does not retarget an existing version to newer
code or overwrite a published release. Use a new workspace version for new
release contents.

Before adding registry publication, establish package ownership, confirm source
and dependency distribution terms, define a package verification gate, and
remove `publish = false` in an explicitly reviewed change. Future bindings should
share the release version while retaining ecosystem-specific package validation.

## Ongoing checks

`CI` checks formatting, lints, documentation, the declared MSRV, native platform
tests, pinned-reference comparisons, and the production browser application
with its native engine. `Strength experiments` runs separately
on a weekly schedule or by manual dispatch. It preserves experiment provenance
and results; a small successful gauntlet is not a release claim of equal Elo.
Dependency updates are proposed through Dependabot and pass the same PR gates.

Workflow files define checks, but do not themselves configure repository branch
protection. Maintainers should require the relevant CI jobs before merging.
