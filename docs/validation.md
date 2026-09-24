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

The reference script verifies the digest before building. Each invocation makes
a fresh local clone of the verified source, checks out the pinned commit, and
builds there. This local clone requires no network access and excludes old
objects and untracked analysis files. The temporary build directory is removed
on exit; the source checkout remains available for inspection without being
cleaned or built in. The script records the reference binary digest and build
architecture in `tests/fixtures/pikafish/reference.json`. Source and build
outputs under that directory are ignored by Git.

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

`PIKAFISH_OUTPUT_DIR` selects the directory for `bin/pikafish`, its model, and
`reference.json`; relative paths resolve from the repository root. The default
is `tests/fixtures/pikafish`. This does not change the default source checkout.
E2E uses the same variable to locate and verify an existing reference build.

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
| Played-history reference comparison | Exact depth-8 search output after real moves, plus expected native game status | Every repetition or chase history |
| Candidate search snapshot | Reviewed fixed-depth moves, scores, node counts, and complete legal PVs | Independent correctness of the snapshot itself |
| UCI process tests | Handshake, limits, cancellation, and output framing | Hosting reliability under arbitrary load |
| HTTP/WebSocket process tests | Server requests, sessions, search results, and cancellation | Browser rendering or board interaction |
| Browser functional checks | Board interaction through the bridge and native engine | Full rule coverage or hosted-service reliability |
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

For the HTTP/WebSocket adapter, Node.js 24 LTS runs the process-level
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
Each position and depth starts with `ucinewgame`, which resets the transposition
table and search histories. The current corpus has 52 positions, producing 156
independent searches per engine. This differs from the continuous benchmark
sequence described below; their total node counts are not interchangeable.

The separate `search_history_equivalence` case replays the initial FEN and
complete UCI move sequence from
[`e2e_platform/fixtures/search-history.tsv`](../e2e_platform/fixtures/search-history.tsv).
Its seven histories include opening play, twofold and threefold repetition,
perpetual check, and a moving chase target. After one reset before each history,
both engines search the resulting position at depth 8, one thread, and 16 MiB
hash. Best move, score type and value, node count, and complete legal PV must
match the pinned official engine exactly.

The case also checks the candidate's `d` game-status response against the
reviewed fixture expectation. This status check is separate from search parity:
the official engine can still search an already adjudicated root, so its search
score is not a game-result oracle. The case belongs to the alignment suite and
can be selected directly:

```sh
scripts/run-e2e.sh --filter search_history_equivalence
```

These histories complement the 156 FEN-based snapshot searches. Replacing a
played history with only its final FEN discards repetition context and cannot
detect state lost while copying a position into search workers. Reports retain
the initial FEN, moves, expected and actual status, and both search results.

The FEN-based snapshot provides a separate regression gate; its output alone
does not establish upstream correctness. An intentional search change can generate a
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
with output from the same unverified engine. Score proximity alone cannot
establish algorithm identity.

NNUE changes also require incremental-versus-refresh checks after captures,
king moves, feature-bucket changes, and make/unmake sequences. SIMD kernels must
match the scalar implementation on supported input ranges and run on a CPU that
actually supports the backend. An all-features build is a compilation gate, not
a substitute for exercising each backend separately.

## Browser functional verification

The React frontend uses `pikarust-bridge`, which launches the native UCI engine.
It does not use the separate `pikarust-server` HTTP API. Frontend lint/build and
the HTTP/WebSocket server smoke test therefore cover different paths from a
browser session.

Build and start the complete local application as shown in the
[README](../README.md#local-web-interface), or run the browser acceptance script
with Node.js 24 LTS (the release line in `.node-version`) and Playwright Chromium:

```sh
scripts/build-web.sh
(cd pikarust-web/frontend && npx --no-install playwright install --with-deps chromium)
node scripts/check-web.mjs
```

The build installs the frontend's locked dependencies. The acceptance script
starts the production bundle's bridge and native engine on a random local port,
using its bundled NNUE model; no separately running service is required. Reports,
WebSocket frames, bridge logs, and screenshots go to `target/e2e/web/`. Set
`PIKARUST_WEB_REPORT_DIR` to retain them elsewhere. CI preserves these outputs as
artifacts, including failures.

Retain `report.json` with `protocol.jsonl`, `bridge.log`, `provenance.json`, and
`board.png` when reporting a failure. The provenance records the revision and
binary, model, and frontend identifiers used by the test.

The browser acceptance scope includes the following interactions:

1. The page connects, displays the starting board, and has no browser console
   errors or failed application requests.
2. As red, make a legal move and wait for the engine reply. Check board state,
   move history, and analysis together. As black, confirm the engine moves first.
3. Switch between depth and time limits, including switching modes without
   changing the newly displayed default value. Flip the board and verify that
   piece selection and destination clicks still correspond to the right squares.
4. Start a new game and undo moves, both after a reply and during a search.
   A result from a cancelled search must not change the replacement position.
5. Reload or reconnect and confirm that connection state and game controls remain
   usable, without duplicated move events.

Record the tested revision, browser, selected model, and checks performed in the
PR or retained test artifacts. A successful frontend build alone is not a browser
functional test.

## Standard benchmark

The standard benchmark searches the 49 official benchmark positions in order,
at depth 13 with one thread and 16 MiB hash. It resets the engine once before the
sequence, then retains the transposition table and search histories between
positions. This tests a different state lifetime from the E2E oracle, which
resets before each position and depth.

| Procedure | Positions and depths | Search state |
| --- | --- | --- |
| Standard benchmark | 49 official positions, depth 13 | One reset before the complete sequence |
| `search_regression` E2E | 52 fixture positions, depths 5, 8, and 13 | Reset before each of the 156 searches |

```sh
# Build and run the candidate benchmark.
scripts/run-bench.sh

# Build both engines and compare the standard sequence over repeated runs.
scripts/run-bench.sh compare

# Explicit experiment settings and retained output directory.
scripts/run-bench.sh compare --rounds 3 --depth 13 --hash 16 \
  --output target/bench/review
```

Comparison mode fixes the thread count to one. By default it runs three rounds,
alternating which engine runs first, and starts a fresh process for each engine
and round. It requires exact per-position node counts, best moves, score types
and values, and complete PVs. Missing prerequisites or any mismatch fail the
command.

Both modes verify the pinned model before use. `PIKARUST_NNUE_MODEL` selects a
different path to that same model; a missing model or digest mismatch fails
instead of falling back to material evaluation.

The script matches the reference build architecture to the candidate's actual
SIMD backend: AVX2 uses `x86-64-avx2`, and NEON uses `armv8`. An explicitly selected
`--reference-arch` must match that backend. Use `--cpu N` to select CPU affinity
on Linux with `taskset`, and record the host configuration alongside the results.
Performance comparison fails on a backend without a matching reference build;
it does not silently compare different instruction sets.
Matching the NNUE instruction set does not make Rust and C++ compilers or their
optimization flags identical. Inspect the retained build commands and logs when
interpreting timing differences.

Outputs go to the chosen directory, or a timestamped directory under
`target/bench`. They include `report.json`, per-engine/per-round logs, build logs,
source-diff evidence, and frozen copies of the executables and model. The summary
reports median elapsed time and NPS, plus the candidate/reference NPS ratio.
Keep the report and its supporting files together when reviewing a performance
claim; they are generated artifacts and do not belong in tracked documentation.
An explicit output directory must be new; the script refuses to overwrite a
previous experiment.

Each comparison publishes its reference under its own output directory's
`reference/`, overriding an external `PIKAFISH_OUTPUT_DIR` for that build. To
reuse that exact binary and metadata in E2E:

```sh
PIKAFISH_OUTPUT_DIR=target/bench/review/reference \
  scripts/run-e2e.sh --suite alignment
```

Run benchmarks on equivalent release builds on the same otherwise idle machine.
Do not overlap benchmark runs with other builds or searches, and do not run
reference setup concurrently with a comparison that uses its output directory.
Record elapsed time, node count, NPS, and the active evaluation backend. A faster
result with a changed search tree needs separate algorithm and strength
analysis. Exact benchmark output only establishes agreement for the stated
sequence and configuration; it does not establish equivalent playing strength.

## Playing strength

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

Full search identity, broad rule coverage, sustained multithread behavior, and
statistically supported playing-strength parity remain distinct validation
targets. Describe the tested corpus and budget whenever reporting alignment.
