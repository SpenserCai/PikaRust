#!/bin/bash
set -euo pipefail

# ============================================================================
# run-e2e.sh
#
# One-click E2E test runner for PikaRust.
# Builds the release binary, verifies prerequisites, runs the E2E platform,
# and reports results.
#
# Usage:
#   scripts/run-e2e.sh              # run default tests (excludes slow gauntlets)
#   scripts/run-e2e.sh --filter X   # run tests matching X (e.g. strength_gauntlet)
#   scripts/run-e2e.sh --list       # list available tests
#   scripts/run-e2e.sh --clean      # remove e2e_platform build artifacts
#
# Slow tests (strength_gauntlet*) are excluded by default.
# To run them: scripts/run-e2e.sh --filter strength_gauntlet
# ============================================================================

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
E2E_DIR="$PROJECT_ROOT/e2e_platform"
PIKARUST_BIN="$PROJECT_ROOT/target/release/pikarust"
PIKAFISH_BIN="$PROJECT_ROOT/tests/fixtures/pikafish/bin/pikafish"
NNUE_MODEL="$PROJECT_ROOT/models/pikafish.nnue"

# --- Helpers ---

info()  { echo "[INFO]  $*"; }
warn()  { echo "[WARN]  $*"; }
error() { echo "[ERROR] $*" >&2; }

# --- Commands ---

do_clean() {
    info "Cleaning e2e_platform build artifacts..."
    if [ -d "$E2E_DIR/target" ]; then
        rm -rf "$E2E_DIR/target"
        info "Removed e2e_platform/target/"
    else
        info "Nothing to clean."
    fi
}

do_list() {
    build_e2e_platform
    cargo run --manifest-path "$E2E_DIR/Cargo.toml" --quiet -- list
}

do_run() {
    local filter="${1:-}"

    info "=== PikaRust E2E Test Runner ==="
    echo ""

    # Step 1: Build PikaRust release binary
    info "[1/4] Building PikaRust release binary..."
    cargo build --release -p pikarust-app --manifest-path "$PROJECT_ROOT/Cargo.toml"
    if [ ! -x "$PIKARUST_BIN" ]; then
        error "PikaRust binary not found at $PIKARUST_BIN"
        exit 1
    fi
    info "  Binary ready: $PIKARUST_BIN"

    # Step 2: Check Pikafish binary
    info "[2/4] Checking Pikafish binary..."
    if [ ! -x "$PIKAFISH_BIN" ]; then
        warn "Pikafish binary not found at $PIKAFISH_BIN"
        warn "Cross-engine tests will be skipped."
        warn "Run 'scripts/setup-pikafish.sh' to set up Pikafish."
    else
        info "  Pikafish ready: $PIKAFISH_BIN"
    fi

    # Step 3: Verify NNUE model
    info "[3/4] Verifying NNUE model..."
    if [ ! -f "$NNUE_MODEL" ]; then
        error "NNUE model not found at $NNUE_MODEL"
        error "Please ensure models/pikafish.nnue exists (Git LFS)."
        exit 1
    fi
    local model_size
    model_size=$(wc -c < "$NNUE_MODEL" | tr -d ' ')
    info "  Model ready: $NNUE_MODEL ($model_size bytes)"

    # Step 4: Build and run E2E platform
    info "[4/4] Running E2E tests..."
    echo ""

    build_e2e_platform

    local run_args=("run")
    if [ -n "$filter" ]; then
        run_args+=("$filter")
    fi

    export PIKARUST_ROOT="$PROJECT_ROOT"
    cargo run --manifest-path "$E2E_DIR/Cargo.toml" --quiet -- "${run_args[@]}"
}

build_e2e_platform() {
    cargo build --manifest-path "$E2E_DIR/Cargo.toml" --quiet
}

# --- Main ---

main() {
    cd "$PROJECT_ROOT"

    case "${1:-run}" in
        --clean)
            do_clean
            ;;
        --list)
            do_list
            ;;
        --filter)
            if [ -z "${2:-}" ]; then
                error "Usage: $0 --filter <pattern>"
                exit 1
            fi
            do_run "$2"
            ;;
        run|--run)
            do_run ""
            ;;
        *)
            echo "Usage: $0 [run | --filter <pattern> | --list | --clean]"
            exit 1
            ;;
    esac
}

main "$@"
