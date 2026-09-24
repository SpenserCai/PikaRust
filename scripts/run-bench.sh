#!/usr/bin/env bash
# Standard continuous bench, or a reproducible comparison with pinned Pikafish.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$PROJECT_ROOT"
if [[ "${1:-}" == compare ]]; then
    shift
    exec python3 "$SCRIPT_DIR/bench_compare.py" "$@"
fi
if [[ "${1:-}" == --help || "${1:-}" == -h ]]; then
    cat <<'HELP'
Usage: scripts/run-bench.sh [--depth N] [--hash MB]
       scripts/run-bench.sh compare [--rounds 3] [--depth 13] [--hash 16]
                                    [--cpu N] [--output target/bench/RUN]

Without compare, build and run the single-engine continuous 49-position bench.
Both modes require the pinned model (PIKARUST_NNUE_MODEL overrides its path).
Use 'scripts/run-bench.sh compare --help' for comparison and build options.
HELP
    exit 0
fi
# Keep model selection in one place so the verified file is the one loaded.
MODEL="${PIKARUST_NNUE_MODEL:-$PROJECT_ROOT/models/pikafish.nnue}"
MODEL="$(python3 -c 'import pathlib,sys; print(pathlib.Path(sys.argv[1]).resolve())' "$MODEL")"
PIKARUST_NNUE_MODEL="$MODEL" "$SCRIPT_DIR/setup-pikafish.sh" --verify-model
for arg in "$@"; do
    [[ "$arg" != --eval-file ]] || { echo 'Use PIKARUST_NNUE_MODEL to select and verify the model' >&2; exit 2; }
done
cargo build --locked --release -p pikarust-bench
"${CARGO_TARGET_DIR:-target}/release/pikarust-bench" bench "$@" --eval-file "$MODEL"
