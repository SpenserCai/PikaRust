#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
DIST_DIR="$ROOT_DIR/pikarust-web/dist"
MODEL_FILE="${PIKARUST_NNUE_FILE:-$ROOT_DIR/models/pikafish.nnue}"
SOURCE_COMMIT="$(python3 "$SCRIPT_DIR/release.py" clean)"

# A runnable bundle must include the selected network and its terms.
python3 - "$MODEL_FILE" <<'PYMODEL'
import pathlib
import sys
p = pathlib.Path(sys.argv[1])
if not p.is_file() or p.stat().st_size < 1024:
    sys.exit(f"Missing NNUE model (or Git LFS pointer): {p}; run git lfs pull first")
PYMODEL

echo "=== Building PikaRust Web ==="

# Clean
rm -rf "$DIST_DIR"
mkdir -p "$DIST_DIR/models"
cp "$MODEL_FILE" "$DIST_DIR/models/pikafish.nnue"
cp "$ROOT_DIR/models/LICENSE-NNUE" "$DIST_DIR/models/LICENSE-NNUE"
cp "$ROOT_DIR/LICENSE" "$DIST_DIR/LICENSE"

# 1. Build engine
echo "[1/3] Building pikarust engine..."
cargo build --locked --release -p pikarust-app --bin pikarust --manifest-path "$ROOT_DIR/Cargo.toml"
cp "$ROOT_DIR/target/release/pikarust" "$DIST_DIR/pikarust"

# 2. Build bridge server
echo "[2/3] Building bridge server..."
cargo build --locked --release -p pikarust-bridge --manifest-path "$ROOT_DIR/Cargo.toml"
cp "$ROOT_DIR/target/release/pikarust-bridge" "$DIST_DIR/pikarust-bridge"

# 3. Build frontend
echo "[3/3] Building frontend..."
cd "$ROOT_DIR/pikarust-web/frontend"
npm ci --silent
npm run build
cp -r dist/* "$DIST_DIR/"

# Ship the exact committed sources, locked Rust dependencies, and dependency notices.
python3 "$SCRIPT_DIR/release.py" web --sha "$SOURCE_COMMIT" --output "$DIST_DIR"

echo ""
echo "=== Build complete: $DIST_DIR ==="
echo "Run with:"
echo "  cd pikarust-web/dist && ./pikarust-bridge --engine-path ./pikarust --static-dir ."
