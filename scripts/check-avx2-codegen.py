#!/usr/bin/env python3
"""Check portable x86-64 release assembly for out-of-line AVX2 intrinsics."""

import argparse
import os
from pathlib import Path
import platform
import re
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]


def check_assembly(assembly):
    # Both ordinary calls and tail calls can hide the per-instruction overhead.
    calls = re.findall(r"^\s*(?:callq?|jmpq?)\s+[^\n]*_mm256_[^\n]*", assembly, re.M)
    if calls:
        raise ValueError("Out-of-line AVX2 intrinsic calls remain:\n" + "\n".join(calls[:8]))
    if not re.search(r"^\s*v\w+\s+[^\n]*%ymm\d+", assembly, re.M):
        raise ValueError("No 256-bit vector instructions found; the check would be vacuous")


def build_assembly():
    if platform.system() != "Linux" or platform.machine() != "x86_64":
        raise ValueError("The assembly check requires native x86-64 Linux")
    (ROOT / "target").mkdir(exist_ok=True)
    target = Path(tempfile.mkdtemp(prefix="avx2-codegen-", dir=ROOT / "target"))
    env = os.environ.copy()
    # This diagnostic must not inherit global +avx2/native flags that conceal a
    # missing kernel annotation. It does not change normal release build flags.
    env.pop("CARGO_ENCODED_RUSTFLAGS", None)
    env.pop("CARGO_BUILD_RUSTFLAGS", None)
    env["RUSTFLAGS"] = "-C target-cpu=x86-64"
    env["CARGO_TARGET_DIR"] = str(target)
    subprocess.run([
        "cargo", "rustc", "--locked", "--release", "--target", "x86_64-unknown-linux-gnu",
        "-p", "pikarust-app", "--bin", "pikarust", "--", "--emit=asm",
    ], cwd=ROOT, env=env, check=True)
    assemblies = list((target / "x86_64-unknown-linux-gnu/release/deps").glob("pikarust-*.s"))
    if len(assemblies) != 1:
        raise ValueError(f"Expected one fresh engine assembly file, found {assemblies}")
    return assemblies[0]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--assembly", type=Path, help="inspect existing GNU-style assembly instead of building")
    args = parser.parse_args()
    try:
        path = args.assembly if args.assembly is not None else build_assembly()
        check_assembly(path.read_text())
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        print(f"AVX2 code-generation check failed: {error}", file=sys.stderr)
        return 1
    print(f"AVX2 kernels contain vector instructions without intrinsic wrapper calls: {path}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
