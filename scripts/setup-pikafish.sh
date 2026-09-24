#!/usr/bin/env bash
# Build the pinned official engine in an ignored, disposable reference directory.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
# shellcheck source=scripts/reference.lock
source "$SCRIPT_DIR/reference.lock"
FIXTURE_DIR="$PROJECT_ROOT/tests/fixtures/pikafish"
SOURCE_DIR="${PIKAFISH_SOURCE_DIR:-$FIXTURE_DIR/source}"
MODEL="${PIKARUST_NNUE_MODEL:-$PROJECT_ROOT/models/pikafish.nnue}"
OUTPUT_DIR="${PIKAFISH_OUTPUT_DIR:-$FIXTURE_DIR}"
if [[ "$OUTPUT_DIR" != /* ]]; then OUTPUT_DIR="$PROJECT_ROOT/$OUTPUT_DIR"; fi

verify_model() {
    [[ -f "$MODEL" ]] || { echo "Missing NNUE: $MODEL (run git lfs pull)" >&2; exit 1; }
    local actual
    if command -v sha256sum >/dev/null; then
        actual="$(sha256sum "$MODEL" | awk '{print $1}')"
    else
        actual="$(shasum -a 256 "$MODEL" | awk '{print $1}')"
    fi
    [[ "$actual" == "$PIKAFISH_NNUE_SHA256" ]] || {
        echo "NNUE hash mismatch: expected $PIKAFISH_NNUE_SHA256, got $actual" >&2; exit 1;
    }
    echo "NNUE SHA-256 verified: $actual"
}

setup_source() {
    if [[ ! -d "$SOURCE_DIR/.git" ]]; then
        [[ ! -e "$SOURCE_DIR" ]] || { echo "Refusing non-git directory: $SOURCE_DIR" >&2; exit 1; }
        git init "$SOURCE_DIR"
        git -C "$SOURCE_DIR" remote add origin "$PIKAFISH_REPOSITORY"
        git -C "$SOURCE_DIR" fetch --depth 1 origin "$PIKAFISH_COMMIT"
        git -C "$SOURCE_DIR" checkout --detach FETCH_HEAD
    fi
    [[ "$(git -C "$SOURCE_DIR" rev-parse HEAD)" == "$PIKAFISH_COMMIT" ]] || {
        echo "Reference checkout has another revision; use a new PIKAFISH_SOURCE_DIR" >&2; exit 1;
    }
    [[ -z "$(git -C "$SOURCE_DIR" status --porcelain --untracked-files=no)" ]] || {
        echo "Reference has tracked modifications; refusing to build an unverified source" >&2; exit 1;
    }
    echo "Reference: $PIKAFISH_REPOSITORY @ $PIKAFISH_COMMIT ($SOURCE_DIR)"
}

case "${1:-build}" in
    --verify-model) verify_model; exit ;;
    --source-only) setup_source; exit ;;
    build) ;;
    *) echo "Usage: $0 [build | --source-only | --verify-model]" >&2; exit 2 ;;
esac
verify_model
setup_source
case "$(uname -m)" in
    x86_64) build_arch=x86-64 ;;
    arm64|aarch64) build_arch=armv8 ;;
    *) build_arch="${PIKAFISH_ARCH:-}"
       [[ -n "$build_arch" ]] || { echo "Set PIKAFISH_ARCH for this unsupported CPU" >&2; exit 1; } ;;
esac
build_arch="${PIKAFISH_ARCH:-$build_arch}"
if command -v nproc >/dev/null; then build_jobs="$(nproc)"; else build_jobs="$(sysctl -n hw.ncpu)"; fi
mkdir -p "$OUTPUT_DIR/bin"
OUTPUT_DIR="$(cd "$OUTPUT_DIR" && pwd)"
SOURCE_DIR="$(cd "$SOURCE_DIR" && pwd)"
# Analysis checkouts may contain objects built with different flags. Clone only
# committed files into a fresh directory; never clean or build in that checkout.
# --local avoids a network fetch and --no-hardlinks keeps the snapshot independent.
BUILD_DIR="$(mktemp -d "$OUTPUT_DIR/build.XXXXXX")"
trap 'rm -rf -- "$BUILD_DIR"' EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
git clone --quiet --local --no-hardlinks --no-checkout -- "$SOURCE_DIR" "$BUILD_DIR"
git -C "$BUILD_DIR" checkout --quiet --detach "$PIKAFISH_COMMIT"
cp "$MODEL" "$BUILD_DIR/src/pikafish.nnue"
make -C "$BUILD_DIR/src" -j"${BUILD_JOBS:-$build_jobs}" build ARCH="$build_arch"
cp "$BUILD_DIR/src/pikafish" "$OUTPUT_DIR/bin/pikafish"
cp "$MODEL" "$OUTPUT_DIR/bin/pikafish.nnue"
# Record exactly what was built, including the binary hash, for CI artifacts.
python3 - "$OUTPUT_DIR" "$PIKAFISH_COMMIT" "$PIKAFISH_NNUE_SHA256" "$build_arch" <<'PY'
import hashlib, json, pathlib, sys
root, commit, model, arch = sys.argv[1:]
root = pathlib.Path(root)
binary = root / "bin/pikafish"
metadata = dict(commit=commit, nnue_sha256=model, architecture=arch,
                binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest())
(root / "reference.json").write_text(json.dumps(metadata, indent=2) + "\n")
PY
echo "Reference ready: $OUTPUT_DIR/bin/pikafish"
