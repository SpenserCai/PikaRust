#!/bin/bash
set -euo pipefail

# ============================================================================
# run-bench.sh
#
# Build release and run Pikafish-aligned bench (49 positions × depth 13).
# Produces deterministic node count for regression testing.
#
# Usage:
#   scripts/run-bench.sh
# ============================================================================

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

cd "$PROJECT_ROOT"

echo "[INFO]  Building pikarust-bench (release)..."
cargo build --release -p pikarust-bench 2>&1 | tail -3

echo "[INFO]  Running bench (49 positions × depth 13)..."
echo
target/release/pikarust-bench bench
