# Engineering instructions

PikaRust is a library-first Xiangqi engine with native applications. Changes must
preserve legal play, reproducible evaluation, and a usable embedding API. Read
[the architecture](docs/architecture.md) and [validation](docs/validation.md)
before changing engine behavior.

## Ownership and boundaries

- `crates/pikarust-core` owns rules, positions, NNUE, search, and the `Engine`
  facade. Keep HTTP, UCI I/O, process management, frontend state, and language
  binding dependencies outside it.
- `crates/uci-rs` owns protocol types and parsing. Keep it independent of the
  search implementation; validate untrusted input without panicking.
- `crates/pikarust-app` adapts the engine to UCI and HTTP/WebSocket applications.
  Applications own paths, environment variables, logging, and deployment policy.
- `pikarust-web/bridge` adapts a UCI child process; `pikarust-web/frontend` owns
  presentation. Do not duplicate authoritative move legality in the frontend.
- `crates/pikarust-bench` and `e2e_platform` are validation tools, not runtime
  dependencies. Keep their commands compatible with CI.
- Future bindings belong in separate crates. Expose explicit ownership,
  cancellation, model loading, and errors through the engine facade; do not
  export search-stack memory layouts or introduce binding runtimes into core.

## Rust and change discipline

- Use the workspace edition, MSRV, version, dependency declarations, and lints.
  Keep `Cargo.lock` committed; use `--locked` for validation and release builds.
- Keep changes focused. Separate mechanical moves from algorithm changes when
  practical, and retain a failing regression before fixing a confirmed defect.
- Prefer typed errors at public boundaries. Do not turn malformed FEN, models,
  options, or protocol input into a panic, default success, or silent fallback.
- Document public API units, ownership, lifecycle, and failure behavior. Model
  and search score units are not interchangeable with UCI centipawns.
- Use narrow visibility for new implementation details. Avoid gratuitous public
  API churn while the existing low-level modules remain accessible.
- Keep hot-path allocation and synchronization deliberate. Require measured
  evidence for performance claims and compare equivalent release builds.
- Limit `unsafe` to justified implementation boundaries. Document each safety
  invariant, including alignment, lengths, CPU features, aliasing, and lifetime.
  A safe API must uphold these invariants for every accepted input.
- SIMD changes require scalar comparison and execution on the relevant CPU
  architecture. Cross-compilation alone does not validate intrinsics.
- Do not add broad lint allowances, ignored failures, larger tolerances, or
  weaker assertions to make a gate green. Fix the cause or report the blocker.

## Reference and algorithm work

1. Use the pinned Pikafish commit and NNUE digest from the reference tooling;
   never compare against a moving branch or rolling model without updating the
   pin explicitly.
2. Fetch the reference with `scripts/setup-pikafish.sh`. Keep downloaded source,
   binaries, models, and diagnostic output in ignored paths. Do not vendor a
   temporary reference checkout or modify its published repository.
3. Identify the first differing invariant: legal moves/perft, position and
   repetition state, NNUE features/accumulators, static evaluation, then search
   decisions. Preserve the FEN, move history, limits, options, model hash, and
   reference revision needed to reproduce it.
4. Add a targeted regression and run the appropriate reference comparison.
   For incremental NNUE changes, compare against full refresh across make/unmake,
   captures, king movement, and feature-bucket changes.
5. Keep node counts, speed, score proximity, search identity, and playing strength
   as separate claims. A legal move or a drawn game does not establish parity.
   Do not publish an Elo claim from the small CI gauntlet.
6. When a pin or model changes, update provenance and all dependent fixtures in
   the same change. Explain fixture differences; do not regenerate expected
   output solely from the implementation being tested.

## Validation gates

Run these from the repository root for Rust changes:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-targets --features pikarust-app/server --locked
cargo test --workspace --doc --locked
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps --locked
```

Run scalar core tests separately when changing engine code:

```sh
cargo test -p pikarust-core --no-default-features --features simd-none --locked
```

Model-backed tests require the actual pinned NNUE file, not a Git LFS pointer.
Follow `docs/validation.md` for reference E2E, feature combinations, frontend
checks, slow tests, and strength testing. Missing prerequisites or zero matched
tests are not a passing validation result. Never claim a remote workflow passed
until its actual result is available.

## Collaboration and repository hygiene

- For substantial work crossing engine, protocol, validation, and infrastructure,
  use parallel agents with disjoint file ownership. Share interface changes
  early; one integrator owns manifests and the final combined validation.
- Read existing changes before editing and preserve others' work. Do not use
  broad resets or overwrite unrelated files to resolve an integration issue.
- Keep durable documentation in `README.md`, `CONTRIBUTING.md`, and `docs/`.
  Record task-specific analysis in PRs/issues; do not commit session transcripts,
  temporary audit reports, scratch plans, generated benchmarks, or model copies.
- Update user documentation when commands, defaults, errors, or API contracts
  change. Prefer a small maintained example over pseudocode that cannot compile.
- PR descriptions must state the problem, behavior change, validation actually
  performed, and remaining limitations. Include known failing cases explicitly.

## Versions and distribution

- Follow `docs/releases.md`. The workspace version drives releases; synchronize
  internal dependency versions and applicable frontend package metadata.
- Keep release builds portable; do not publish binaries built with
  `target-cpu=native` as generic architecture downloads.
- Release automation creates reviewable drafts from a successful CI revision.
  Do not publish packages or a public release merely to test the workflow.
- Preserve all existing license notices. The repository declaration, upstream
  Pikafish/Stockfish licenses, and NNUE weight terms are distinct. Do not assert
  that the repository declaration resolves upstream provenance, or enable
  registry publishing before maintainers resolve the distribution requirements.
