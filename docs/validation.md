# Validation and reference alignment

Validation separates rule correctness, numerical evaluation, search behavior,
protocol behavior, performance, and playing strength. Passing one layer is not
evidence that every other layer is aligned.

## Reproducible inputs

[`scripts/reference.lock`](../scripts/reference.lock) pins the official Pikafish
repository, complete commit ID, and NNUE SHA-256. The actual model is tracked by
Git LFS. Start with:

```sh
git lfs pull --include="models/pikafish.nnue"
scripts/setup-pikafish.sh --verify-model
scripts/setup-pikafish.sh
```

The reference script verifies the digest before building. It uses a clean,
detached checkout at the pinned commit and records the reference binary digest
and build architecture in `tests/fixtures/pikafish/reference.json`. Source and
build outputs under that directory are ignored by Git.

For source inspection without a C++ build:

```sh
scripts/setup-pikafish.sh --source-only
```

The default checkout is `tests/fixtures/pikafish/source`. Set
`PIKAFISH_SOURCE_DIR` to another disposable path, or an existing clean checkout at
the exact pin. The script refuses a mismatched or modified checkout rather than
resetting another developer's work. `BUILD_JOBS` controls build concurrency;
`PIKAFISH_ARCH` overrides the reference build architecture. Generic x86-64 is the
default on x86-64, so setup does not assume AVX2 support.

`PIKARUST_NNUE_MODEL` can select another location for the same pinned model in
reference tooling. This is distinct from the native CLI's
`PIKARUST_NNUE_FILE` runtime option. Changing the location does not relax digest
validation.

## Test layers

| Layer | Evidence | What it does not establish |
| --- | --- | --- |
| Rust unit and integration tests | Position invariants, parsing, search lifecycle, numerical regressions | Behavior for all legal positions |
| Perft reference comparison | Exact leaf totals and root move divisions on the fixture positions | Evaluation or search strength |
| NNUE reference comparison | Exact raw integer output on the comparison positions | Identity of the entire search tree |
| Fixed-node search diagnostics | Legal output, repeatability, and reported fixed-budget differences | Full upstream search alignment |
| Fixed-depth search reference comparison | Exact official best moves, scores, node counts, and complete PVs at depths 5, 8, and 13 | Identity at every depth or position |
| Candidate search snapshot | Reviewed fixed-depth moves, scores, node counts, and complete legal PVs | Independent correctness of the snapshot itself |
| UCI process tests | Handshake, limits, cancellation, and output framing | Hosting reliability under arbitrary load |
| Match tests | Complete legal games and recorded results | Statistically established Elo from a small sample |
| Benchmarks | Node counts and throughput for the recorded configuration | Correctness or equal playing strength |

## Commands and reports

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-targets --features pikarust-app/server --locked
cargo test --workspace --doc --locked
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps --locked
cargo test -p pikarust-core --no-default-features --features simd-none --locked

scripts/run-e2e.sh --list
scripts/run-e2e.sh --suite smoke
scripts/run-e2e.sh --suite alignment
scripts/run-e2e.sh --suite all
```

The smoke suite exercises the native UCI program. The alignment suite requires
the pinned official binary and includes perft, NNUE, fixed-node diagnostics,
strict fixed-depth search comparisons, and a reviewed candidate search snapshot.
The comparison positions are maintained in
[`e2e_platform/fixtures/positions.tsv`](../e2e_platform/fixtures/positions.tsv).
The default `all` suite covers non-slow cases; use an explicit case filter for
slow matches. `--filter NAME` selects a registered case, with exact names taking
priority over substring matches. Zero matching cases is an error.

Reports are written to `target/e2e/report.json`, including failed runs. Set
`PIKARUST_E2E_REPORT` to retain separate reports:

```sh
PIKARUST_E2E_REPORT=target/e2e/alignment.json \
  scripts/run-e2e.sh --suite alignment
```

The report contains the selected outcomes and preflight checks. Preserve it
together with the PikaRust revision, pinned-reference metadata, platform, and
command when reporting a regression. CI uploads reports as workflow artifacts.
Missing prerequisites and protocol errors fail the selected suite; an absent
reference is not represented as a successful comparison.

For the HTTP/WebSocket adapter, Node.js 22 or newer runs the process-level
request, search, cancellation, and session lifecycle checks:

```sh
cargo build --release --locked -p pikarust-app --features server --bin pikarust-server
node scripts/check-server.mjs
```

The server check writes `target/e2e/server.json`; set `PIKARUST_SERVER_REPORT` to
retain the report elsewhere.

For intentionally slow core cases, select a relevant ignored test by name after
listing it. Avoid running every ignored test simply to recheck a documentation
or packaging change:

```sh
cargo test -p pikarust-core --release --locked -- --list --ignored
```

## Search alignment procedure

The `search_regression` case compares every fixture position against the pinned
official engine at depths 5, 8, and 13, with one thread and 16 MiB hash. Best move,
score, node count, and complete PV must match exactly. It also checks candidate
output against the reviewed `e2e_platform/fixtures/search-baseline.json`
snapshot, including the complete PV and its legality. Both engines' PVs are
replayed move by move to verify legality before comparing the recorded output.

The snapshot provides a separate regression gate; its output alone does not
establish upstream correctness. An intentional search change can generate a
proposed replacement in an ignored output directory:

```sh
cargo build --release --locked -p pikarust-app --bin pikarust
cargo run --release --locked -p pikarust-e2e -- \
  record-search-baseline target/e2e/proposed-search-baseline.json
```

Review changed positions, moves, scores, node counts, and PVs against the intended
algorithm change, with reference and strength evidence. Only then replace the
tracked fixture in the same reviewed PR. Normal test execution does not update
expected output.

Reset engine state before each position, use one thread and the same model and
hash capacity, and reproduce the complete move history. Comparing identical FEN
strings alone can lose repetition and search-history context. Keep fixed-depth,
fixed-node, and fixed-time measurements distinct.

When a search differs, localize the earliest divergence in this order:

1. Legal moves, check state, repetition/rule status, and make/unmake state.
2. NNUE feature indices, accumulator refresh/update, and raw network output.
3. Static evaluation transformations, score units, and terminal handling.
4. Move ordering, transposition-table state, pruning and reduction conditions,
   histories, and principal-variation lifetime.

Add an independently justified expected result for the first incorrect state.
Do not fix a search discrepancy by raising a cp tolerance or replacing fixtures
with output from the same unverified engine. The former broad score-tolerance
comparison was only a catastrophic-regression smoke check and cannot establish
algorithm identity.

NNUE changes also require incremental-versus-refresh checks after captures,
king moves, feature-bucket changes, and make/unmake sequences. SIMD kernels must
match the scalar implementation on supported input ranges and run on a CPU that
actually supports the backend. An all-features build is a compilation gate, not
a substitute for exercising each backend separately.

## Playing strength and performance

Run the candidate-versus-baseline experiment with a separately built previous
PikaRust executable and a working directory containing the same pinned model:

```sh
PIKARUST_BASELINE_BIN=/absolute/path/to/previous/pikarust \
PIKARUST_BASELINE_CWD=/absolute/path/to/previous/checkout \
  scripts/run-e2e.sh --filter strength_regression
```

The default experiment uses 200 sampled openings, each played with both colors,
2,000 nodes per move, and seed `20260924`. It accepts the run only when the
one-sided 95% Hoeffding lower bound on the paired score reaches 0.40 and no more
than half of games reach the move cap. An inconclusive run does not pass. This
criterion concerns the specified opening generator and budget; it is not an Elo
guarantee or a general claim of equal strength.

| Variable | Default | Meaning |
| --- | --- | --- |
| `PIKARUST_STRENGTH_PAIRS` | `200` | Number of opening pairs |
| `PIKARUST_STRENGTH_NODES` | `2000` | Nodes per move |
| `PIKARUST_STRENGTH_MAX_MOVES` | `150` | Match move cap |
| `PIKARUST_STRENGTH_JOBS` | `4` | Concurrent opening pairs, from `1` to `8` |
| `PIKARUST_STRENGTH_MIN_SCORE` | `0.40` | Required lower bound on paired score |
| `PIKARUST_STRENGTH_SEED` | `20260924` | Replayable opening seed |

Choose parameters before examining outcomes. Lowering the threshold after a
failed run changes the experiment rather than resolving a regression. The JSON
report records binary and model digests, parameters, bounds, and each game.
The experiment requires distinct candidate and baseline binary digests. Before
play, it copies both executables and the pinned model to a checked snapshot under
the report directory, so another build cannot replace a participant during the
experiment. Each completed pair is also written to a JSONL checkpoint beside
the report (`report.games.jsonl` for the default report path). Retain this file
with the final report to preserve game-level evidence from interrupted runs.
The manual `Strength experiments` workflow accepts `suite=regression` with a
`baseline_ref` commit or tag; the baseline must contain a committed lockfile.

The small paired-opening gauntlet is useful for catching illegal moves,
termination failures, and large changes. Its sample size cannot support a strong
claim about Elo or equality with Pikafish. Do not treat a maximum-move adjudicated
draw as evidence of equal strength.

For a strength experiment, record both engine revisions, network digests,
compiler/build options, CPU, hash, threads, time control, adjudication, opening
set, and every game result. Swap colors for each opening. Compare the candidate
to a fixed previous PikaRust build to assess regression; compare separately to
Pikafish to characterize the remaining gap. Use a sufficient sample and a
predeclared statistical acceptance criterion, with paired-game uncertainty.

For performance, run `scripts/run-bench.sh` on equivalent release builds on the
same otherwise idle machine. Report elapsed time, node count, NPS, and the active
evaluation backend. A faster result with a changed search tree needs separate
algorithm and strength analysis.

Full search identity, broad rule coverage, sustained multithread behavior, and
statistically supported playing-strength parity remain distinct validation
targets. Describe the tested corpus and budget whenever reporting alignment.
