#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
DIST_DIR="$ROOT_DIR/pikarust-web/dist"

echo "=== Building PikaRust Web ==="

# Clean
rm -rf "$DIST_DIR"
mkdir -p "$DIST_DIR"

# 1. Build engine
echo "[1/3] Building pikarust engine..."
cargo build --release -p pikarust-app --bin pikarust --manifest-path "$ROOT_DIR/Cargo.toml"
cp "$ROOT_DIR/target/release/pikarust" "$DIST_DIR/pikarust"

# 2. Build bridge server
echo "[2/3] Building bridge server..."
cargo build --release -p pikarust-bridge --manifest-path "$ROOT_DIR/Cargo.toml"
cp "$ROOT_DIR/target/release/pikarust-bridge" "$DIST_DIR/pikarust-bridge"

# 3. Build frontend
echo "[3/3] Building frontend..."
cd "$ROOT_DIR/pikarust-web/frontend"
npm ci --silent
npm run build
cp -r dist/* "$DIST_DIR/"

echo ""
echo "=== Build complete: $DIST_DIR ==="
echo "Run with:"
echo "  cd pikarust-web/dist && ./pikarust-bridge --engine-path ./pikarust --static-dir ."
