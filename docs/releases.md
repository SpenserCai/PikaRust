# Releases

The **Release** GitHub Action runs only by manual dispatch. It reads
`[workspace.package].version` from the selected source's `Cargo.toml`, derives
`v<version>`, and creates the tag after validating the build artifacts. A CI
completion does not trigger release preparation.

The engine and native applications are `GPL-3.0-or-later`; the independent MIT
components and NNUE model retain their separate terms. Follow the
[licensing policy](licensing.md) when distributing artifacts. Release checks
require the current component license declarations and notices; an older source
revision declaring the combined engine MIT is not eligible for publication.

## Prepare and run a release

1. Update the workspace version and workspace path-dependency versions together.
   Member crates inherit their version. Update the workspace `Cargo.lock`,
   frontend `package.json`, and frontend `package-lock.json` to the same version.
2. Run `python3 scripts/check-release.py`, complete the
   [validation gates](validation.md), and merge the reviewed version change.
   The CI **push run on main** for that exact commit must finish successfully.
3. Open **Actions → Release → Run workflow**, with the workflow branch set to
   `main`. Leave `commit` blank to select the main commit captured when the
   workflow is dispatched, or provide a full 40-character main commit SHA.
4. Choose `draft` to prepare a reviewable release, or `publish` to publish after
   all artifacts have been verified. The default is `draft`. There is no manual
   tag step and no CI run ID to copy.

The workflow finds the latest CI push run for the selected SHA and checks its
repository, workflow identity, branch, event, status, and conclusion. Missing,
queued, failed, unrelated, fork, and PR runs cannot authorize a release. The
commit must remain reachable from main. These checks run again before remote
release changes and publication.

The controller uses the workflow-dispatch revision. The selected source is
checked out separately, and builds use that source's lockfile and Rust toolchain.
Moving main during a build does not silently change the selected source.

Historical commits are accepted only if their `.github/workflows` tree matches
current main. GitHub requires additional workflow-write permissions to release
an older workflow definition; the repository's `GITHUB_TOKEN` cannot grant
those permissions. The action rejects that case before creating a tag or
uploading assets. Merge a new workspace version on current main instead.

## Artifacts and supported targets

| Target | Archive | Applications |
| --- | --- | --- |
| `x86_64-unknown-linux-gnu` | `.tar.gz` | UCI engine and HTTP server |
| `aarch64-apple-darwin` | `.tar.gz` | UCI engine and HTTP server |
| `x86_64-pc-windows-msvc` | `.zip` | UCI engine and HTTP server |

Builds use the committed dependency graph, pinned Rust toolchain, release
profile, and portable target configuration. GNU/Linux binaries are dynamically
linked and use the workflow's Ubuntu baseline; compatibility with every older
Linux distribution is not implied.

Application archives include executables, `LICENSE`, `LICENSE-MIT`, `NOTICE.md`,
upstream and dependency notices, model setup instructions, reference pins, and
documentation. Their `build.json` records the version, exact commit, target,
compiler, GPL code license, model exclusion, and the matching source archive.

Each release includes a corresponding source archive and SHA-256 checksum files.
The archive contains that commit's source, build scripts, lockfile, vendored
Cargo dependency sources and licenses, and an offline Cargo source configuration.
The Rust toolchain and native build tools must be installed separately. Archive
validation requires the complete expected set, correct checksums and embedded
source identity, and no model weights. A Git LFS pointer in the source archive
is not a bundled NNUE network. Preserve the matching source asset alongside
every native binary distribution.

To rebuild the native applications from a downloaded source archive, first
install the Rust toolchain named in its `rust-toolchain.toml` and the native
build tools for your platform. Set the version to match that archive:

```sh
release_version=0.1.0
tar -xzf "pikarust-${release_version}-source.tar.gz"
cd "pikarust-${release_version}-source"
cargo build --offline --locked --release -p pikarust-app --features server --bins
```

The included `vendor/` and `.cargo/config.toml` supply Cargo dependencies without
network access. NNUE weights are obtained separately for runtime use; follow the
[model setup instructions](../models/README.md).

NNUE weights have separate terms and are excluded from application archives.
GPL, MIT, dependency notices, and model terms retain their documented scopes.
Artifact checks verify package contents and source identity; they do not grant
additional rights. All crates remain `publish = false`; this action does not
run `cargo publish` or publish Python/npm bindings.

The frontend and web bridge are not packaged in these native archives. Build
the local web bundle from a clean committed checkout with `scripts/build-web.sh`.
It includes the GPL engine, MIT bridge and frontend, selected model and original
terms, license notices, and matching source archive and checksum. The builder
checks source cleanliness and commit stability before and after packaging.
For uncommitted development, use the
[bridge and frontend development commands](../CONTRIBUTING.md#local-web-development).

## Drafts, retries, and immutable versions

Both modes initially create a draft. `publish` changes it to a public release
only after the complete uploaded asset set matches the verified local files.
The action never moves an existing tag or overwrites a published release.
A version assigned to another commit requires a new workspace version.
An older draft cannot be repurposed to publish a different revision or bypass
the current source-license requirements.

A draft can be resumed at its original eligible commit. Existing assets with
identical SHA-256 digests are retained, and only missing assets are uploaded.
The action never replaces or deletes existing assets. A differing, incomplete,
or unexpected asset causes a failure for maintainer review; rebuilding the
same source is not sufficient proof that different bytes are interchangeable.
Archive timestamps and ownership are normalized, but different toolchains or
runner environments can still produce different binaries. Prefer GitHub's
**Re-run failed jobs** for an interrupted upload so successful build artifacts
remain available.

Review the source commit, validation evidence, artifacts, notices, and release
notes before choosing public distribution. Existing draft notes are preserved.
The `publish` choice is an explicit publication instruction, not an additional
license verification mechanism. GitHub repository rules may impose further
restrictions on tag creation or release publication.

## Local package validation

The packaging and verification commands do not require a GitHub token and do
not create tags or releases. Start with a clean committed checkout so that the
binary, notices, and corresponding source identify the same revision:

```sh
cargo build --locked --release -p pikarust-app --features server --bins \
  --target x86_64-unknown-linux-gnu
python3 scripts/release.py package --target x86_64-unknown-linux-gnu \
  --output target/release-smoke
python3 scripts/release.py source --output target/release-smoke
python3 scripts/release.py verify --target x86_64-unknown-linux-gnu \
  --output target/release-smoke
```

Use an empty output directory for the intended artifact set. Omitting
`verify --target` requires all three platform archives plus source and their
checksums, as used by the release action. `--root PATH` before the subcommand
selects a separate source checkout while retaining the current release tools.

CI runs a native package/source verification on Linux, macOS, and Windows,
alongside release failure-path tests. It also validates formatting, lints,
documentation, MSRV, engine tests, pinned-reference comparisons, and the
production browser application. Strength experiments remain a separate
scheduled/manual workflow; their results are not a claim of equal Elo.

Before registry publication is added, establish package ownership and applicable
distribution terms, define package verification, and review any change to
`publish = false`. Future bindings should share the release version while
retaining ecosystem-specific package validation.
