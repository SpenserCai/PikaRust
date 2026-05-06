# PikaRust

A Chinese Chess (Xiangqi) engine written in Rust, reimplemented from [Pikafish](https://github.com/official-pikafish/Pikafish). PikaRust provides the engine as a **library crate**, making it easy to embed in your own applications.

> ⚠️ **This project is under active development.** Core functionality is aligned with Pikafish, but there is still room for optimization (NPS, multi-threading).

## Features

- **Library-first design** — use `pikarust-core` as a Rust dependency
- **UCI protocol** — standard UCI interface for GUI integration
- **NNUE evaluation** — Pikafish-compatible neural network inference
- **SIMD acceleration** — NEON (ARM64), AVX2 (x86_64), with scalar fallback
- **SaaS server** — WebSocket + REST API for cloud deployment (experimental)
- **Web UI** — standalone browser-based interface for local play and analysis

## Project Structure

```
crates/
├── pikarust-core     # Engine library (search, NNUE, move generation, position)
├── uci-rs            # UCI protocol parser/serializer
├── pikarust-app      # CLI binary + SaaS server binary
└── pikarust-bench    # Benchmarks and perft verification
pikarust-web/
├── bridge/           # WebSocket bridge server (spawns UCI engine process)
└── frontend/         # React + TypeScript + Tailwind CSS (Vite)
```

## Requirements

- Rust 1.85+ (edition 2024)
- Git LFS (for NNUE model file)

## Quick Start

```bash
# Clone (ensure Git LFS is installed)
git clone https://github.com/SpenserCai/PikaRust.git
cd PikaRust

# Build the UCI engine
cargo build --release -p pikarust-app --bin pikarust

# Run UCI mode
./target/release/pikarust
```

## Usage

### As a Library

Add to your `Cargo.toml`:

```toml
[dependencies]
pikarust-core = { git = "https://github.com/SpenserCai/PikaRust.git" }
```

### UCI Engine

```bash
cargo build --release -p pikarust-app --bin pikarust
./target/release/pikarust
```

The `pikarust` binary speaks standard UCI protocol, compatible with any Xiangqi GUI that supports UCI.

### SaaS Server (Experimental)

> ⚠️ The SaaS server has **not been thoroughly tested** and is provided as-is.

```bash
cargo build --release -p pikarust-app --bin pikarust-server
./target/release/pikarust-server
```

Environment variables:

| Variable | Default | Description |
|----------|---------|-------------|
| `PIKARUST_PORT` | `8080` | Server listen port |
| `PIKARUST_MAX_ENGINES` | `8` | Engine pool size |
| `PIKARUST_THREADS_PER_ENGINE` | `1` | Threads per engine instance |
| `PIKARUST_HASH_MB` | `16` | Hash table size per engine (MB) |
| `PIKARUST_MAX_SESSIONS` | `64` | Maximum concurrent sessions |
| `PIKARUST_IDLE_TIMEOUT` | `600` | Session idle timeout (seconds) |

### Web UI

A standalone web interface for local play and analysis:

```bash
# Build everything (engine + bridge + frontend)
# Requires Node.js for the frontend build
./scripts/build-web.sh

# Run
cd pikarust-web/dist
./pikarust-bridge --engine-path ./pikarust --static-dir .
# Open http://localhost:9000
```

## Benchmarks

Comparison against Pikafish (49 positions × depth 13, single-threaded):

| Metric | PikaRust | Pikafish |
|--------|----------|----------|
| Nodes | 3,175,304 | 3,410,768 |
| NPS | 332,736 | 911,482 |
| Time | 9,543 ms | 3,742 ms |

**Evaluation alignment** (E2E test, depth 8):
- Maximum centipawn difference: **77 cp** (tolerance: ±500 cp)
- Cross-engine match result: **draw**

Node counts are closely aligned (~93%), confirming search logic correctness. The NPS gap is a known area for optimization.

## SIMD Support

| Backend | Architecture | Status |
|---------|-------------|--------|
| NEON | ARM64 (aarch64) | ✅ Rigorously tested |
| AVX2 | x86_64 | ⚠️ Implemented, not tested |
| Scalar | All platforms | ✅ Fallback, always available |

The default feature `simd-auto` performs runtime detection and selects the best available backend.

NEON has been tested on macOS ARM (Apple Silicon), which validates all ARM64 platforms (Linux ARM64, etc.) since NEON is a mandatory part of the AArch64 specification.

To force a specific backend:

```bash
cargo build --release --no-default-features --features simd-neon
cargo build --release --no-default-features --features simd-avx2
cargo build --release --no-default-features --features simd-none
```

## Running Tests

```bash
# Bench (deterministic node count for regression testing)
./scripts/run-bench.sh

# E2E tests (requires Pikafish binary)
# First-time setup: compiles a pinned Pikafish commit (requires C++ toolchain + make)
./scripts/setup-pikafish.sh

# Run all E2E tests
./scripts/run-e2e.sh

# Run specific test
./scripts/run-e2e.sh --filter eval_equivalence

# List available tests
./scripts/run-e2e.sh --list
```

## NNUE Model

The NNUE model (`models/pikafish.nnue`) is tracked via Git LFS. Its license is governed by the [NNUE-License](models/LICENSE-NNUE) from the Pikafish project:

- **No commercial use without permission**
- Only for legal use

Please refer to the [original license terms](https://github.com/official-pikafish/Networks) for full details.

## License

This project is licensed under the **MIT License**.

Note: The NNUE model file (`models/pikafish.nnue`) is subject to its own [NNUE-License](models/LICENSE-NNUE) and is **not** covered by the MIT License of this project.

## Acknowledgments

PikaRust is a Rust reimplementation of [Pikafish](https://github.com/official-pikafish/Pikafish), a free and strong UCI Xiangqi engine derived from [Stockfish](https://github.com/official-stockfish/Stockfish). We are grateful to both projects for their foundational work in computer chess and Xiangqi.

- **[Pikafish](https://github.com/official-pikafish/Pikafish)** — The reference implementation that PikaRust is based on
- **[Stockfish](https://github.com/official-stockfish/Stockfish)** — The chess engine from which Pikafish is derived
