#!/bin/bash
set -euo pipefail

# ============================================================================
# setup-pikafish.sh
#
# Clones, checks out, and compiles a pinned version of Pikafish for use in
# PikaRust integration tests and benchmark comparisons.
#
# Output: tests/fixtures/pikafish/bin/pikafish
# ============================================================================

PIKAFISH_REPO="https://github.com/official-pikafish/Pikafish"
PIKAFISH_COMMIT="76239d0b06720bfa4588989fd4ac7573e9dbf887"

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
FIXTURE_DIR="$PROJECT_ROOT/tests/fixtures/pikafish"
BUILD_DIR="$FIXTURE_DIR/build"
BIN_DIR="$FIXTURE_DIR/bin"
MODEL_SRC="$PROJECT_ROOT/models/pikafish.nnue"

# --- Detect platform and architecture ---

detect_arch() {
    local kernel arch
    kernel="$(uname -s)"
    arch="$(uname -m)"

    case "$kernel" in
        Darwin)
            case "$arch" in
                arm64) echo "apple-silicon" ;;
                x86_64) echo "x86-64-avx2" ;;
                *) echo "x86-64" ;;
            esac
            ;;
        Linux)
            case "$arch" in
                x86_64) echo "x86-64-avx2" ;;
                aarch64) echo "armv8" ;;
                *) echo "x86-64" ;;
            esac
            ;;
        *)
            echo "x86-64"
            ;;
    esac
}

# --- Clone or update ---

setup_source() {
    if [ -d "$BUILD_DIR/.git" ]; then
        echo "[1/4] Pikafish source exists, verifying commit..."
        local current_commit
        current_commit="$(git -C "$BUILD_DIR" rev-parse HEAD)"
        if [ "$current_commit" = "$PIKAFISH_COMMIT" ]; then
            echo "  Already at $PIKAFISH_COMMIT"
            return 0
        fi
        echo "  Commit mismatch ($current_commit), resetting..."
        git -C "$BUILD_DIR" fetch origin
        git -C "$BUILD_DIR" checkout "$PIKAFISH_COMMIT"
    else
        echo "[1/4] Cloning Pikafish..."
        mkdir -p "$FIXTURE_DIR"
        git clone "$PIKAFISH_REPO" "$BUILD_DIR"
        git -C "$BUILD_DIR" checkout "$PIKAFISH_COMMIT"
    fi
    echo "  Pinned to commit: $PIKAFISH_COMMIT"
}

# --- Build ---

build_pikafish() {
    local arch
    arch="$(detect_arch)"
    echo "[2/4] Building Pikafish (ARCH=$arch)..."

    local nproc_cmd
    if command -v nproc >/dev/null 2>&1; then
        nproc_cmd="$(nproc)"
    else
        nproc_cmd="$(sysctl -n hw.ncpu 2>/dev/null || echo 4)"
    fi

    mkdir -p "$BIN_DIR"

    # Copy NNUE model to build dir so Pikafish can find it
    if [ -f "$MODEL_SRC" ]; then
        cp "$MODEL_SRC" "$BUILD_DIR/src/pikafish.nnue"
        echo "  Copied pikafish.nnue from models/"
    else
        echo "  WARNING: models/pikafish.nnue not found, Pikafish will try to download it"
    fi

    cd "$BUILD_DIR/src"
    make -j"$nproc_cmd" build ARCH="$arch"
    cp pikafish "$BIN_DIR/pikafish"
    echo "  Binary: $BIN_DIR/pikafish"
}

# --- Verify ---

verify_build() {
    echo "[3/4] Verifying build..."
    if [ ! -x "$BIN_DIR/pikafish" ]; then
        echo "  ERROR: pikafish binary not found or not executable"
        exit 1
    fi

    # Quick smoke test: run bench with depth 1
    local output
    output=$("$BIN_DIR/pikafish" bench 1 1 1 2>&1 | tail -5)
    echo "  Smoke test output:"
    echo "$output" | sed 's/^/    /'
    echo "  Build verified successfully"
}

# --- Summary ---

print_summary() {
    echo "[4/4] Setup complete"
    echo ""
    echo "  Pikafish commit: $PIKAFISH_COMMIT"
    echo "  Architecture:    $(detect_arch)"
    echo "  Binary:          $BIN_DIR/pikafish"
    echo "  NNUE model:      $MODEL_SRC"
    echo ""
    echo "Usage:"
    echo "  # Run bench (NPS baseline)"
    echo "  $BIN_DIR/pikafish bench"
    echo ""
    echo "  # Run perft"
    echo "  echo 'position startpos' | $BIN_DIR/pikafish"
    echo ""
    echo "  # UCI mode"
    echo "  $BIN_DIR/pikafish"
}

# --- Main ---

main() {
    echo "=== PikaRust: Pikafish Setup ==="
    echo ""
    setup_source
    build_pikafish
    verify_build
    print_summary
}

main "$@"
