# PikaRust

[![CI](https://github.com/SpenserCai/PikaRust/actions/workflows/ci.yml/badge.svg)](https://github.com/SpenserCai/PikaRust/actions/workflows/ci.yml)

PikaRust is a Rust implementation of
[Pikafish](https://github.com/official-pikafish/Pikafish), a Chinese chess
(Xiangqi) engine. It provides a reusable Rust library and native applications
for UCI, HTTP/WebSocket, and browser play.

The engine implements Xiangqi rules, NNUE evaluation, and search, with scalar,
x86-64 AVX2, and AArch64 NEON evaluation backends. Development uses a pinned
Pikafish revision and model for reproducible algorithm comparisons.

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

Use a Git dependency to embed the library. Add a `rev` pin for the commit your
application has validated; registry publication is not enabled.
The library is `GPL-3.0-or-later`; see [licensing](docs/licensing.md) for embedding
and source-distribution requirements.

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

The alignment suite checks perft, raw NNUE evaluation, and exact search results
against the pinned reference, including positions with played move histories.
The standard benchmark retains search state across its 49-position sequence;
the FEN-based search suite resets for each position and depth. Missing models or
reference builds fail the checks.

See [validation](docs/validation.md) for fixtures, reports, browser tests,
benchmarks, and controlled strength experiments. Agreement on the tested corpus
does not establish identical play for every position or search budget.

## SIMD

The core includes scalar, x86-64 AVX2, and AArch64 NEON implementations. The default
`simd-auto` selects an available backend at runtime. Test the scalar path with:

```sh
cargo test -p pikarust-core --locked --no-default-features --features simd-none
```

CI exercises supported native targets and compares SIMD results with the scalar
implementation. See [validation](docs/validation.md) for backend-specific checks.

## Applications

### Local web interface

Use Node.js 24 LTS and npm to build the native engine, bridge, and frontend:

```sh
scripts/build-web.sh
cd pikarust-web/dist
./pikarust-bridge --engine-path ./pikarust --static-dir .
```

The distribution builder requires a clean committed checkout and includes source
matching the bundled binaries. For work with uncommitted changes, use the
[local development commands](CONTRIBUTING.md#local-web-development).

The local bundle includes the selected model and its license terms. Set
`PIKARUST_NNUE_FILE` before building to select another model path. Open
<http://localhost:9000>. The browser talks to a native engine through the bridge;
it does not execute the engine as WASM. The bridge targets the bundled PikaRust
engine, including its position and game-status diagnostic extensions; generic
UCI support alone does not make another engine a compatible replacement.

For repeatable browser acceptance checks, use Node.js 24 LTS, install
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
- [Releases](docs/releases.md): manual release workflow and distribution.
- [Licensing](docs/licensing.md): component licenses, provenance, and source distribution.
- [AGENTS.md](AGENTS.md): persistent instructions for automated contributors.

CI validates Rust quality, native platforms, the browser application, and
reference comparisons. Run the **Release** workflow manually to prepare a draft
or publish a release from a validated main-branch commit. It reads the workspace
version from `Cargo.toml` and creates the version tag automatically; no manual
tagging is required. See [releases](docs/releases.md) for inputs and checks.

## Licenses and acknowledgments

The engine library and the programs that link it use
[GPL-3.0-or-later](LICENSE). The independent UCI parser, web bridge, and frontend
retain [MIT](LICENSE-MIT). See [licensing](docs/licensing.md) for the component
map and distribution requirements, and [NOTICE.md](NOTICE.md) for Pikafish and
Stockfish provenance.

The model retains its original [NNUE terms](models/LICENSE-NNUE), including the
requirement for permission for commercial use. The code license does not cover
the weights. See [model documentation](models/README.md) for their source and
fixed digest.

PikaRust builds on the work of the
[Pikafish](https://github.com/official-pikafish/Pikafish) and
[Stockfish](https://github.com/official-stockfish/Stockfish) contributors.
