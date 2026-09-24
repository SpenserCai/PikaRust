#!/usr/bin/env python3
"""Validate the version contract shared by CI and release automation."""

import argparse
import json
from pathlib import Path
import re
import sys
import tomllib


ROOT = Path(__file__).resolve().parent.parent


def read_toml(path: Path) -> dict:
    with path.open("rb") as stream:
        return tomllib.load(stream)


def check_frontend_version(version: str) -> None:
    frontend = ROOT / "pikarust-web/frontend"
    manifest = json.loads((frontend / "package.json").read_text(encoding="utf-8"))
    lock = json.loads((frontend / "package-lock.json").read_text(encoding="utf-8"))
    for label, package in (("package.json", manifest), ("package-lock.json", lock),
                           ("package-lock.json root package", lock["packages"][""])):
        if package.get("version") != version:
            raise ValueError(f"frontend {label}: version must equal workspace version {version}")
        if package.get("name") != manifest["name"]:
            raise ValueError(f"frontend {label}: package name must match package.json")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--field", choices=("version", "msrv", "toolchain"))
    args = parser.parse_args()
    workspace = read_toml(ROOT / "Cargo.toml")["workspace"]
    package = workspace["package"]
    version = package["version"]
    if not re.fullmatch(r"\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?", version):
        raise ValueError(f"unsupported release version: {version}")
    channel = read_toml(ROOT / "rust-toolchain.toml")["toolchain"]["channel"]
    if not re.fullmatch(r"\d+\.\d+\.\d+", channel):
        raise ValueError("rust-toolchain.toml must pin an exact stable Rust version")
    for member in workspace["members"]:
        manifest = read_toml(ROOT / member / "Cargo.toml")["package"]
        for key in ("version", "edition", "rust-version"):
            if manifest.get(key) != {"workspace": True}:
                raise ValueError(f"{member}: package.{key} must inherit from workspace")
    for name, dependency in workspace["dependencies"].items():
        if isinstance(dependency, dict) and "path" in dependency:
            if dependency.get("version") != version:
                raise ValueError(f"{name}: path dependency version must equal {version}")
    if not (ROOT / "Cargo.lock").is_file():
        raise ValueError("the application workspace must commit Cargo.lock")
    check_frontend_version(version)
    values = {"version": version, "msrv": package["rust-version"], "toolchain": channel}
    print(values[args.field] if args.field else f"Workspace version {version}; MSRV {package['rust-version']}; toolchain {channel}")


if __name__ == "__main__":
    try:
        main()
    except (KeyError, OSError, ValueError) as error:
        sys.exit(str(error))
