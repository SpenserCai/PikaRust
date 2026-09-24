#!/usr/bin/env bash
# All suites fail closed; missing prerequisites are never silently skipped.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$PROJECT_ROOT"
export PIKARUST_ROOT="$PROJECT_ROOT"
args=()
case "${1:-run}" in
    --list) args=(list) ;;
    --suite) [[ $# == 2 ]] || { echo "Expected --suite smoke|alignment|all" >&2; exit 2; }; args=(run --suite "$2") ;;
    --filter) [[ $# == 2 ]] || { echo "Expected --filter NAME" >&2; exit 2; }; args=(run "$2") ;;
    run|--run|--strict) [[ $# -le 1 ]] || exit 2; args=(run) ;;
    *) echo "Usage: $0 [--suite smoke|alignment|all | --filter NAME | --list]" >&2; exit 2 ;;
esac
if [[ "${args[0]}" != list ]]; then
    "$SCRIPT_DIR/setup-pikafish.sh" --verify-model
    cargo build --locked --release -p pikarust-app -p pikarust-e2e
else
    cargo build --locked --release -p pikarust-e2e
fi
exec "$PROJECT_ROOT/target/release/pikarust-e2e" "${args[@]}"
