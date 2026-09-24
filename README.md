# PikaRust

[![CI](https://github.com/SpenserCai/PikaRust/actions/workflows/ci.yml/badge.svg)](https://github.com/SpenserCai/PikaRust/actions/workflows/ci.yml)

A library-first Chinese chess (Xiangqi) engine in Rust, developed against a
pinned [Pikafish](https://github.com/official-pikafish/Pikafish) reference.

PikaRust provides legal move generation, NNUE evaluation, search, a native UCI
application, and experimental HTTP/WebSocket and web applications. Full search
identity and playing-strength parity with Pikafish remain development goals.
See [validation](docs/validation.md) for what each test actually establishes.

## Components

| Component | Purpose |
| --- | --- |
| `pikarust-core` | Embeddable engine, rules, position, NNUE, and search |
| `pikarust-uci` | UCI parser and response types (`uci_rs` Rust import) |
| `pikarust-app` | `pikarust` UCI executable and `pikarust-server` |
| `pikarust-bench` | Perft and search benchmarks |
| `pikarust-e2e` | Process-level validation and pinned-reference comparisons |
| `pikarust-web` | Native engine bridge and React frontend |

The [architecture](docs/architecture.md) describes module boundaries and the
extension path for future language bindings. Python, Node.js, and browser-native
WASM bindings are not currently provided.

## Build and run

Install Rust through rustup, Git LFS, and a native build toolchain. The repository
selects its Rust toolchain in `rust-toolchain.toml`; `Cargo.toml` declares the MSRV.

```sh
git clone https://github.com/SpenserCai/PikaRust.git
cd PikaRust
git lfs install
git lfs pull --include="models/pikafish.nnue"
scripts/setup-pikafish.sh --verify-model
cargo build --release --locked -p pikarust-app --bin pikarust
./target/release/pikarust --eval-file models/pikafish.nnue
```

The engine communicates through UCI on stdin/stdout. A Xiangqi GUI that supports
UCI can launch it. Use `--eval-file PATH` or `PIKARUST_NNUE_FILE` to select a model
explicitly; an invalid explicit model produces an error. `--material-only` is
available for model-independent development and is not representative of NNUE
playing strength.

The current engine supports one principal variation (`MultiPV=1`); larger values
are rejected.

The [model documentation](models/README.md) records the fixed network digest and
its separate usage terms. Release application archives do not contain weights.

## Embed the Rust library

Until registry publication is enabled, use a Git dependency and pin the revision
you have validated:

```toml
[dependencies]
pikarust-core = { git = "https://github.com/SpenserCai/PikaRust.git" }
```

```rust,no_run
use pikarust_core::engine::{Engine, SearchLimits};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut engine = Engine::with_nnue_file("models/pikafish.nnue")?;
    let limits = SearchLimits {
        depth: Some(8),
        ..SearchLimits::default()
    };
    let handle = engine.try_go(&limits)?;
    let result = handle.wait_result()?;
    println!("best move: {:?}, nodes: {}", result.best_move, result.nodes);
    Ok(())
}
```

`Engine::with_network` shares immutable weights between engine instances through
`Arc<Network>`. `Engine::without_nnue` selects material-only evaluation.
`Engine::new` retains local model discovery and can fall back to material-only
evaluation; use an explicit constructor for deployments and comparisons.

## Validation

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-targets --features pikarust-app/server --locked
cargo test --workspace --doc --locked

# Download and build the pinned official reference; requires C++ and make.
scripts/setup-pikafish.sh
scripts/run-e2e.sh --suite smoke
scripts/run-e2e.sh --suite alignment

# Inspect upstream code without building it or downloading another model.
scripts/setup-pikafish.sh --source-only

# Standard 49-position benchmark, then comparison with pinned Pikafish.
scripts/run-bench.sh
scripts/run-bench.sh compare
```

Reference source and build output are disposable and ignored by Git. Missing
models, digest mismatches, unavailable required engines, and unmatched test
filters fail validation. See [validation](docs/validation.md) for suite coverage,
reports, slow tests, and strength experiments.

The alignment suite requires exact official best moves, scores, node counts, and
complete PVs at depths 5, 8, and 13 across the maintained position corpus, plus a
reviewed candidate snapshot. These gates describe that corpus and configuration;
they do not establish identity for every position or search budget.

The standard benchmark retains search state across its 49-position sequence;
the E2E oracle resets for every position and depth. See the
[benchmark procedure](docs/validation.md#standard-benchmark) for repeatable
timing, matching CPU backends, comparison reports, and the difference between
these checks.

Node counts are useful regression signals, and NPS measures throughput on a
particular machine. Neither proves search correctness or equivalent playing
strength. Strength claims require a controlled match experiment with sufficient
games and uncertainty estimates.

## SIMD

The core includes scalar, x86-64 AVX2, and AArch64 NEON implementations. The default
`simd-auto` selects an available backend at runtime. Test the scalar path with:

```sh
cargo test -p pikarust-core --locked --no-default-features --features simd-none
```

Platform support is established by the actual CI targets and executed numerical
comparisons. Successful testing on one operating system or CPU does not validate
every platform sharing its instruction set.

## Applications

### Local web interface

Install Node.js and npm, then build the native engine, bridge, and frontend:

```sh
scripts/build-web.sh
cd pikarust-web/dist
./pikarust-bridge --engine-path ./pikarust --static-dir .
```

The local bundle includes the selected model and its license terms. Set
`PIKARUST_NNUE_FILE` before building to select another model path. Open
<http://localhost:9000>. The browser talks to a native engine through the bridge;
it does not execute the engine as WASM. The bridge targets the bundled PikaRust
engine, including its position and game-status diagnostic extensions; generic
UCI support alone does not make another engine a compatible replacement.

For repeatable browser acceptance checks, use Node.js 22 or newer, install
Playwright Chromium, and run `node scripts/check-web.mjs` against the built
bundle. It starts its own local bridge and engine. The
[browser validation procedure](docs/validation.md#browser-functional-verification)
lists the setup commands, covered interactions, and retained reports.

### HTTP/WebSocket server

```sh
cargo build --release --locked -p pikarust-app --features server --bin pikarust-server
./target/release/pikarust-server
```

This adapter is experimental. Set `PIKARUST_NNUE_FILE` for explicit model loading,
or run it from the repository root for default model discovery. Deployment
authentication and resource isolation are not provided.

| Environment variable | Default | Meaning |
| --- | --- | --- |
| `PIKARUST_PORT` | `8080` | Listen port |
| `PIKARUST_MAX_ENGINES` | `8` | Engine pool capacity |
| `PIKARUST_THREADS_PER_ENGINE` | `1` | Search threads per engine |
| `PIKARUST_HASH_MB` | `16` | Hash size per engine in MiB |
| `PIKARUST_MAX_SESSIONS` | `64` | Session capacity |
| `PIKARUST_IDLE_TIMEOUT` | `600` | Idle timeout in seconds |
| `PIKARUST_NNUE_FILE` | Unset | Explicit network path shared by pooled engines |

## Development and releases

- [Contributing](CONTRIBUTING.md): setup, change process, and checks.
- [Architecture](docs/architecture.md): engine and application boundaries.
- [Validation](docs/validation.md): reference alignment and strength evidence.
- [Releases](docs/releases.md): version-driven draft releases and distribution.
- [AGENTS.md](AGENTS.md): persistent instructions for automated contributors.

CI validates Rust quality, platform builds, model-backed tests, and reference
comparisons. Release automation uses the workspace version and a successful
main-branch CI revision to prepare a draft; manual tagging is not required.

## Licenses and acknowledgments

The repository currently declares [MIT](LICENSE). The upstream
[Pikafish](https://github.com/official-pikafish/Pikafish) and
[Stockfish](https://github.com/official-stockfish/Stockfish) projects use GPLv3;
PikaRust is developed from their engine work. The repository's MIT declaration
does not by itself resolve licensing requirements for derived code. Source
provenance and distribution terms require maintainer review before publication.
Registry publishing is disabled.

The NNUE weights have separate [NNUE terms](models/LICENSE-NNUE), including a
restriction on commercial use without permission. They are not covered by the
repository's MIT declaration. We acknowledge Pikafish and Stockfish for their
foundational engine work.
