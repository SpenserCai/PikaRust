#!/usr/bin/env python3
"""Validate the version contract shared by CI and release automation."""

import argparse
import json
from pathlib import Path
import re
import sys
import tomllib


ROOT = Path(__file__).resolve().parent.parent
NUMBER = r"(?:0|[1-9][0-9]*)"
PRE_ID = rf"(?:{NUMBER}|[0-9]*[A-Za-z-][0-9A-Za-z-]*)"
SEMVER = re.compile(rf"{NUMBER}\.{NUMBER}\.{NUMBER}(?:-{PRE_ID}(?:\.{PRE_ID})*)?(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?")


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


def validate_workspace() -> dict:
    workspace = read_toml(ROOT / "Cargo.toml")["workspace"]
    package = workspace["package"]
    version = package["version"]
    if not SEMVER.fullmatch(version):
        raise ValueError(f"unsupported release version: {version}")
    channel = read_toml(ROOT / "rust-toolchain.toml")["toolchain"]["channel"]
    if not re.fullmatch(r"\d+\.\d+\.\d+", channel):
        raise ValueError("rust-toolchain.toml must pin an exact stable Rust version")
    members = []
    for member in workspace["members"]:
        manifest = read_toml(ROOT / member / "Cargo.toml")["package"]
        members.append(manifest["name"])
        for key in ("version", "edition", "rust-version"):
            if manifest.get(key) != {"workspace": True}:
                raise ValueError(f"{member}: package.{key} must inherit from workspace")
    for name, dependency in workspace["dependencies"].items():
        if isinstance(dependency, dict) and "path" in dependency:
            if dependency.get("version") != version:
                raise ValueError(f"{name}: path dependency version must equal {version}")
    if not (ROOT / "Cargo.lock").is_file():
        raise ValueError("the application workspace must commit Cargo.lock")
    locked = read_toml(ROOT / "Cargo.lock")["package"]
    for name in members:
        entries = [entry for entry in locked if entry["name"] == name and "source" not in entry]
        if len(entries) != 1 or entries[0]["version"] != version:
            raise ValueError(f"Cargo.lock: workspace package {name} must have version {version}")
    check_frontend_version(version)
    return {"version": version, "msrv": package["rust-version"], "toolchain": channel}


def main() -> None:
    global ROOT
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=ROOT, help="workspace to validate")
    parser.add_argument("--field", choices=("version", "msrv", "toolchain"))
    args = parser.parse_args()
    ROOT = args.root.resolve()
    values = validate_workspace()
    print(values[args.field] if args.field else
          f"Workspace version {values['version']}; MSRV {values['msrv']}; toolchain {values['toolchain']}")


if __name__ == "__main__":
    try:
        main()
    except (KeyError, OSError, ValueError) as error:
        sys.exit(str(error))
