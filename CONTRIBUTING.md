# Contributing

PikaRust develops a reusable Xiangqi engine, native applications, and repeatable
validation against a pinned Pikafish revision. Read [architecture](docs/architecture.md)
for component boundaries and [validation](docs/validation.md) for test semantics.
Automation agents also follow [AGENTS.md](AGENTS.md).

## Development environment

Use the Rust toolchain selected by `rust-toolchain.toml`, Git LFS, and the committed
lockfiles. C++ and `make` are needed only for building the upstream reference;
Node.js 24 LTS and npm are needed for the web frontend. `.node-version` selects
the supported Node.js release line for local development and CI. Reference setup
uses Python 3; release tooling requires Python 3.11 or newer.

```sh
git clone https://github.com/SpenserCai/PikaRust.git
cd PikaRust
git lfs install
git lfs pull --include="models/pikafish.nnue"
cargo build --workspace --locked
```

Run `scripts/setup-pikafish.sh --verify-model` before model-backed validation. A small text pointer
at `models/pikafish.nnue` is not the model. A missing or different network must not
be worked around by weakening tests.

## Making a change

1. Reproduce the behavior with the smallest useful FEN, move sequence, or protocol
   transcript. For algorithm comparisons record the pinned reference, model,
   thread count, hash size, search limits, and platform.
2. Add a regression at the layer that owns the invariant. Use end-to-end checks
   as well when the failure crosses parsing, process I/O, or engine lifecycle.
3. Keep application policy outside core and avoid unrelated refactoring in the
   same algorithm patch. Document public errors, units, and resource ownership.
4. Run the applicable checks below, then inspect the final diff for generated
   artifacts, unused code, and outdated documentation.
5. Open a PR explaining the problem, resulting behavior, validation performed,
   and any outstanding limitations. Include commands and failure details when
   an environment prevents a required check.

## Local checks

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-targets --features pikarust-app/server --locked
cargo test --workspace --doc --locked
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps --locked
cargo test -p pikarust-core --no-default-features --features simd-none --locked
```

For position, evaluation, search, or UCI changes, build the pinned reference and
run the reference E2E suite:

```sh
scripts/setup-pikafish.sh
scripts/run-e2e.sh --suite alignment
```

For search and evaluation changes, also run `scripts/run-bench.sh compare` to
check the standard continuous benchmark sequence. Preserve its report with the
PR evidence; do not compare its node total with the E2E suite, which resets
search state before each position and depth.

For HTTP/WebSocket changes, build `pikarust-server` with the `server` feature and
run `node scripts/check-server.mjs` with Node.js 24 LTS. See
[validation](docs/validation.md) for the full command and report location.

For frontend changes:

```sh
npm --prefix pikarust-web/frontend ci
npm --prefix pikarust-web/frontend run lint
npm --prefix pikarust-web/frontend run build
```

Also run the complete frontend, bridge, and native engine through the
[browser functional checks](docs/validation.md#browser-functional-verification).
The `scripts/check-web.mjs` entry point starts the production bundle and drives
Playwright Chromium. Install its browser dependencies and build the bundle as
described in that procedure. The separate HTTP server smoke test does not
exercise the board UI.

See [validation](docs/validation.md) for slow tests, strength experiments, and
interpreting differences. The workflow files specify the required platform and
toolchain matrix; a successful local run does not replace those jobs.

## Dependencies, versions, and documentation

Declare shared Rust dependencies at the workspace root. Keep the lockfile in the
same PR as dependency changes and preserve compatibility with the declared MSRV.
If a new dependency requires a newer compiler, update the manifest, toolchain,
CI, and documentation together.

Review npm peer dependencies together with release notes before changing major
versions. Keep related toolchains in the same update: React and its types, Vite
and its plugins, and ESLint with its parser and plugins. Use `npm ci` without
peer-resolution overrides, then run the frontend and browser checks. Dependabot
groups these updates; exclusions must identify a specific compatibility limit
and be reviewed when that limit changes.

JavaScript actions have an embedded Node.js runtime independent of the project's
Node.js version. Check each action's supported runtime when updating its pinned
commit, and keep the version comment accurate. Do not suppress runtime retirement
warnings in place of upgrading the action.

Use focused comments for implementation invariants, rustdoc for public API
contracts, and `docs/` for maintained architecture and operating instructions.
Keep investigations, benchmark logs, and one-off reports in issues, PRs, or CI
artifacts rather than committing temporary documents.

Release preparation follows [releases](docs/releases.md). Do not include model
weights in crate packages or alter existing license notices as routine cleanup.
Upstream source provenance and NNUE terms must be reviewed before enabling
registry publication or publishing a distribution.
