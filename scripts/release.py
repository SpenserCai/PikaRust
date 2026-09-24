#!/usr/bin/env python3
"""Prepare versioned, model-free application and source release artifacts."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile
import tempfile
import urllib.error
import urllib.request
import zipfile


ROOT = Path(__file__).resolve().parent.parent


def version() -> str:
    return subprocess.check_output(
        [sys.executable, str(ROOT / "scripts/check-release.py"), "--field", "version"],
        text=True,
    ).strip()


def github_get(path: str):
    request = urllib.request.Request(
        f"https://api.github.com/repos/{os.environ['GITHUB_REPOSITORY']}/{path}",
        headers={"Authorization": f"Bearer {os.environ['GH_TOKEN']}",
                 "Accept": "application/vnd.github+json", "User-Agent": "PikaRust-release"},
    )
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            return json.load(response)
    except urllib.error.HTTPError as error:
        if error.code == 404:
            return None
        raise


def write_outputs(values: dict) -> None:
    text = "".join(f"{key}={value}\n" for key, value in values.items())
    with open(os.environ["GITHUB_OUTPUT"], "a", encoding="utf-8") as stream:
        stream.write(text)
    print(text, end="")


def resolve(run_id: int) -> None:
    run = github_get(f"actions/runs/{run_id}")
    if not run or any((run.get("conclusion") != "success", run.get("event") != "push",
                       run.get("head_branch") != "main", run.get("path") != ".github/workflows/ci.yml",
                       run.get("head_repository", {}).get("full_name") != os.environ["GITHUB_REPOSITORY"])):
        raise ValueError("release requires a successful CI push run from this repository's main branch")
    write_outputs({"sha": run["head_sha"]})


def plan() -> None:
    release_version = version()
    tag = f"v{release_version}"
    sha = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    release = github_get(f"releases/tags/{tag}")
    ref = github_get(f"git/ref/tags/{tag}")
    same_tag = True
    if ref:
        obj = ref["object"]
        while obj["type"] == "tag":
            obj = github_get(f"git/tags/{obj['sha']}")["object"]
        same_tag = obj["sha"] == sha
    # Existing versions are immutable. A failed draft build may be retried only
    # at the same verified commit, without retargeting somebody else's release.
    ready = same_tag and (release is None or (
        release["draft"] and release["target_commitish"] == sha))
    write_outputs({"version": release_version, "tag": tag, "sha": sha,
                   "build": str(ready).lower()})


def checksum(path: Path) -> None:
    digest = hashlib.sha256(path.read_bytes()).hexdigest()
    path.with_name(path.name + ".sha256").write_text(f"{digest}  {path.name}\n", encoding="utf-8")


def package(target: str, output: Path) -> None:
    output.mkdir(parents=True, exist_ok=True)
    stem = f"pikarust-{version()}-{target}"
    windows = "windows" in target
    suffix = ".exe" if windows else ""
    with tempfile.TemporaryDirectory(prefix="pikarust-release-") as temporary:
        stage = Path(temporary) / stem
        stage.mkdir()
        for binary in ("pikarust", "pikarust-server"):
            shutil.copy2(ROOT / "target" / target / "release" / f"{binary}{suffix}", stage)
        for filename in ("LICENSE", "README.md"):
            shutil.copy2(ROOT / filename, stage)
        shutil.copytree(ROOT / "docs", stage / "docs")
        (stage / "models").mkdir()
        for filename in ("LICENSE-NNUE", "README.md"):
            shutil.copy2(ROOT / "models" / filename, stage / "models" / filename)
        (stage / "scripts").mkdir()
        shutil.copy2(ROOT / "scripts/reference.lock", stage / "scripts/reference.lock")
        (stage / "MODEL-REQUIRED.txt").write_text(
            "NNUE weights are not included. Obtain the compatible model as documented in\n"
            "models/README.md and review models/LICENSE-NNUE before use or redistribution.\n"
            "Run the engine from this directory with the model at models/pikafish.nnue,\n"
            "or pass --eval-file PATH to the CLI / set PIKARUST_NNUE_FILE.\n"
            "This is a development release; search parity and equal playing strength\n"
            "with official Pikafish are not established.\n", encoding="utf-8")
        metadata = {
            "version": version(), "target": target,
            "commit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
            "rustc": subprocess.check_output(["rustc", "--version"], text=True).strip(),
            "nnue_included": False,
        }
        (stage / "build.json").write_text(json.dumps(metadata, indent=2) + "\n", encoding="utf-8")
        archive = output / (stem + (".zip" if windows else ".tar.gz"))
        if windows:
            with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED) as bundle:
                for path in sorted(stage.rglob("*")):
                    if path.is_file():
                        bundle.write(path, path.relative_to(stage.parent))
        else:
            with tarfile.open(archive, "w:gz") as bundle:
                bundle.add(stage, arcname=stem)
        checksum(archive)
        print(archive)


def source(output: Path) -> None:
    output.mkdir(parents=True, exist_ok=True)
    stem = f"pikarust-{version()}-source"
    archive = output / f"{stem}.tar.gz"
    subprocess.run(["git", "archive", "--format=tar.gz", f"--prefix={stem}/",
                    f"--output={archive.resolve()}", "HEAD"], cwd=ROOT, check=True)
    checksum(archive)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    resolve_parser = commands.add_parser("resolve", help="verify a successful main-branch CI run")
    resolve_parser.add_argument("run_id", type=int)
    commands.add_parser("plan", help="check whether the current version needs a draft release")
    package_parser = commands.add_parser("package")
    package_parser.add_argument("--target", required=True)
    package_parser.add_argument("--output", type=Path, default=ROOT / "target/release-artifacts")
    source_parser = commands.add_parser("source")
    source_parser.add_argument("--output", type=Path, default=ROOT / "target/release-artifacts")
    args = parser.parse_args()
    if args.command == "resolve":
        resolve(args.run_id)
    elif args.command == "plan":
        plan()
    elif args.command == "package":
        package(args.target, args.output)
    else:
        source(args.output)


if __name__ == "__main__":
    main()
