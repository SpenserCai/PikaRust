#!/usr/bin/env python3
"""Validate and package releases; remote changes require an explicit main dispatch."""

import argparse
from datetime import datetime, timezone
import gzip
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import stat
import subprocess
import sys
import tarfile
import tempfile
import tomllib
import urllib.error
import urllib.parse
import urllib.request
import zipfile


CONTROLLER = Path(__file__).resolve().parent
ROOT = CONTROLLER.parent
TARGETS = ("x86_64-unknown-linux-gnu", "aarch64-apple-darwin", "x86_64-pc-windows-msvc")


def version() -> str:
    return subprocess.check_output(
        [sys.executable, str(CONTROLLER / "check-release.py"), "--root", str(ROOT), "--field", "version"],
        text=True,
    ).strip()


def git(*args: str) -> str:
    return subprocess.check_output(["git", *args], cwd=ROOT, text=True).strip()


def checked_sha(value: str) -> str:
    if not re.fullmatch(r"[0-9a-f]{40}", value):
        raise ValueError("select a complete lowercase 40-character commit SHA")
    return value


def github_request(path: str, method: str = "GET", data=None):
    request = urllib.request.Request(
        f"https://api.github.com/repos/{os.environ['GITHUB_REPOSITORY']}/{path}",
        data=None if data is None else json.dumps(data).encode(), method=method,
        headers={"Authorization": f"Bearer {os.environ['GH_TOKEN']}",
                 "Accept": "application/vnd.github+json", "Content-Type": "application/json",
                 "X-GitHub-Api-Version": "2026-03-10", "User-Agent": "PikaRust-release"},
    )
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            return json.load(response)
    except urllib.error.HTTPError as error:
        if method == "GET" and error.code == 404:
            return None
        raise


def github_get(path: str):
    return github_request(path)


def write_outputs(values: dict) -> None:
    text = "".join(f"{key}={value}\n" for key, value in values.items())
    if os.environ.get("GITHUB_OUTPUT"):
        with open(os.environ["GITHUB_OUTPUT"], "a", encoding="utf-8") as stream:
            stream.write(text)
    print(text, end="")


def require_dispatch() -> None:
    if (os.environ.get("GITHUB_EVENT_NAME") != "workflow_dispatch"
            or os.environ.get("GITHUB_REF") != "refs/heads/main"):
        raise ValueError("Release must be dispatched explicitly from the main branch")


def workflow_tree(commit: str) -> str:
    """Read tree identities without a truncated recursive-tree comparison."""
    obj = github_get(f"git/commits/{commit}")
    if not obj:
        raise ValueError("selected Git commit is unavailable")
    tree_sha = obj["tree"]["sha"]
    for component in (".github", "workflows"):
        tree = github_get(f"git/trees/{tree_sha}")
        if not tree or tree.get("truncated"):
            raise ValueError("could not verify the workflow tree")
        entry = next((item for item in tree["tree"] if item["path"] == component), None)
        if not entry or entry["type"] != "tree":
            raise ValueError("the source is missing its workflow directory")
        tree_sha = entry["sha"]
    return tree_sha


def successful_ci(sha: str) -> dict:
    """Require the latest CI push run for this exact main-branch commit."""
    checked_sha(sha)
    main = github_get("git/ref/heads/main")
    if not main or main["object"]["type"] != "commit":
        raise ValueError("could not resolve the main branch")
    main_sha = checked_sha(main["object"]["sha"])
    comparison = github_get(f"compare/{sha}...{main_sha}?per_page=1")
    if (not comparison or comparison.get("status") not in ("ahead", "identical")
            or comparison.get("base_commit", {}).get("sha") != sha):
        raise ValueError("the selected commit must be reachable from main")
    if sha != main_sha and workflow_tree(sha) != workflow_tree(main_sha):
        raise ValueError("selected workflows differ from main; GITHUB_TOKEN cannot publish that historical revision; merge a new workspace version instead")
    workflow = github_get("actions/workflows/ci.yml")
    if not workflow or workflow.get("path") != ".github/workflows/ci.yml":
        raise ValueError("the repository CI workflow is missing")
    runs = []
    for page in range(1, 11):
        query = urllib.parse.urlencode({"branch": "main", "event": "push", "head_sha": sha,
                                        "per_page": 100, "page": page})
        response = github_get(f"actions/workflows/{workflow['id']}/runs?{query}")
        if not response:
            raise ValueError("could not obtain CI runs for the selected commit")
        runs.extend(response["workflow_runs"])
        if len(runs) >= response["total_count"]:
            break
    else:
        raise ValueError("too many CI runs to establish the latest result")
    if not runs:
        raise ValueError("the selected commit has no CI push run on main")
    run = max(runs, key=lambda item: item["id"])
    repository = os.environ["GITHUB_REPOSITORY"].casefold()
    if any((run.get("status") != "completed", run.get("conclusion") != "success",
            run.get("event") != "push", run.get("head_branch") != "main", run.get("head_sha") != sha,
            run.get("path", "").split("@", 1)[0] != ".github/workflows/ci.yml",
            run.get("workflow_id") != workflow["id"],
            run.get("head_repository", {}).get("full_name", "").casefold() != repository,
            run.get("repository", {}).get("full_name", "").casefold() != repository)):
        raise ValueError("the latest exact-commit CI run must be completed and successful on this repository's main push")
    return run


def resolve(commit: str) -> None:
    require_dispatch()
    sha = checked_sha(commit or os.environ["GITHUB_SHA"])
    run = successful_ci(sha)
    write_outputs({"sha": sha, "ci_run_id": run["id"]})


def tag_commit(tag: str):
    ref = github_get(f"git/ref/tags/{urllib.parse.quote(tag, safe='')}")
    if ref is None:
        return None
    if ref.get("ref") != f"refs/tags/{tag}":
        raise ValueError("unexpected tag reference returned by GitHub")
    obj = ref["object"]
    visited = set()
    while obj["type"] == "tag":
        sha = checked_sha(obj["sha"])
        if sha in visited or len(visited) >= 16:
            raise ValueError("invalid annotated tag chain")
        visited.add(sha)
        annotated = github_get(f"git/tags/{sha}")
        if not annotated:
            raise ValueError("annotated tag target is missing")
        obj = annotated["object"]
    if obj["type"] != "commit":
        raise ValueError("release tags must point to commits")
    return checked_sha(obj["sha"])


def find_release(tag: str):
    # The tag endpoint is documented for published releases. Listing with push
    # access also returns drafts, including a draft that has no Git tag yet.
    matches = []
    for page in range(1, 101):
        releases = github_get(f"releases?per_page=100&page={page}")
        if releases is None:
            raise ValueError("could not list releases, including drafts")
        matches.extend(item for item in releases if item["tag_name"] == tag)
        if len(releases) < 100:
            if len(matches) > 1:
                raise ValueError(f"multiple releases use {tag}; inspect the existing drafts")
            return matches[0] if matches else None
    raise ValueError("too many releases to establish the version's current state")


def release_state(tag: str, sha: str):
    existing_tag = tag_commit(tag)
    release = find_release(tag)
    if existing_tag is not None and existing_tag != sha:
        raise ValueError(f"{tag} already belongs to another commit; update the workspace version")
    if release and (release.get("tag_name") != tag or release.get("target_commitish") != sha
                    or not release.get("draft") or release.get("immutable")):
        raise ValueError(f"{tag} is published or belongs to another commit; update the workspace version")
    return existing_tag, release


def plan() -> dict:
    release_version = version()
    sha = checked_sha(git("rev-parse", "HEAD"))
    tag = f"v{release_version}"
    release_state(tag, sha)
    values = {"version": release_version, "tag": tag, "sha": sha}
    write_outputs(values)
    return values


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def checksum(path: Path) -> None:
    path.with_name(path.name + ".sha256").write_text(f"{digest(path)}  {path.name}\n", encoding="utf-8", newline="\n")


def archive_name(release_version: str, target: str) -> str:
    return f"pikarust-{release_version}-{target}" + (".zip" if "windows" in target else ".tar.gz")


def compiler_version() -> str:
    return subprocess.check_output(["rustc", "--version"], cwd=ROOT, text=True).strip()


def validate_archive(path: Path, release_version: str, sha: str, target: str) -> None:
    stem = f"pikarust-{release_version}-{target}"
    source_archive = target == "source"
    with zipfile.ZipFile(path) if path.suffix == ".zip" else tarfile.open(path) as archive:
        if isinstance(archive, zipfile.ZipFile):
            entries = archive.infolist()
            names = [entry.filename for entry in entries]
            if any(stat.S_ISLNK(entry.external_attr >> 16) for entry in entries):
                raise ValueError(f"{path.name}: unexpected archive symlink")
            read = archive.read
        else:
            entries = archive.getmembers()
            names = [entry.name for entry in entries]
            if any(not (entry.isdir() or entry.isfile()) for entry in entries):
                raise ValueError(f"{path.name}: unexpected archive link or special file")
            if source_archive and archive.pax_headers.get("comment") != sha:
                raise ValueError("source archive does not identify the selected Git commit")
            def read(name):
                stream = archive.extractfile(name)
                if stream is None:
                    raise ValueError(f"{name} is not an archived file")
                return stream.read()
        for name in names:
            parts = PurePosixPath(name).parts
            if not parts or parts[0] != stem or ".." in parts or "\\" in name:
                raise ValueError(f"{path.name}: invalid archive path {name}")
            if name.endswith(".nnue"):
                content = read(name)
                if not source_archive or len(content) > 1024 or not content.startswith(b"version https://git-lfs.github.com/spec/v1\n"):
                    raise ValueError("release archives must not contain NNUE weights")
        if source_archive:
            manifest = tomllib.loads(read(f"{stem}/Cargo.toml").decode())
            if manifest["workspace"]["package"]["version"] != release_version:
                raise ValueError("source archive has a different workspace version")
        else:
            metadata = json.loads(read(f"{stem}/build.json"))
            if any((metadata.get("version") != release_version, metadata.get("commit") != sha,
                    metadata.get("target") != target, metadata.get("nnue_included") is not False)):
                raise ValueError(f"{path.name}: build metadata does not match the selected source")
            suffix = ".exe" if "windows" in target else ""
            required = (f"pikarust{suffix}", f"pikarust-server{suffix}", "LICENSE",
                        "models/LICENSE-NNUE", "models/README.md", "scripts/reference.lock", "MODEL-REQUIRED.txt")
            for name in required:
                if not read(f"{stem}/{name}"):
                    raise ValueError(f"{path.name}: missing or empty {name}")


def package(target: str, output: Path) -> None:
    if target not in TARGETS:
        raise ValueError(f"unsupported release target: {target}")
    output.mkdir(parents=True, exist_ok=True)
    release_version = version()
    sha = checked_sha(git("rev-parse", "HEAD"))
    timestamp = int(git("show", "-s", "--format=%ct", "HEAD"))
    stem = f"pikarust-{release_version}-{target}"
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
            "or pass --eval-file PATH to the CLI / set PIKARUST_NNUE_FILE.\n", encoding="utf-8", newline="\n")
        metadata = {"version": release_version, "target": target, "commit": sha,
                    "rustc": compiler_version(),
                    "nnue_included": False}
        (stage / "build.json").write_text(json.dumps(metadata, indent=2) + "\n", encoding="utf-8", newline="\n")
        archive = output / archive_name(release_version, target)
        if windows:
            date = datetime.fromtimestamp(max(timestamp, 315532800), timezone.utc).timetuple()[:6]
            with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED) as bundle:
                for path in sorted(stage.rglob("*")):
                    if path.is_file():
                        info = zipfile.ZipInfo(path.relative_to(stage.parent).as_posix(), date_time=date)
                        info.compress_type = zipfile.ZIP_DEFLATED
                        info.external_attr = path.stat().st_mode << 16
                        bundle.writestr(info, path.read_bytes())
        else:
            def normalize(info):
                info.uid = info.gid = 0
                info.uname = info.gname = ""
                info.mtime = timestamp
                return info
            with archive.open("wb") as raw, gzip.GzipFile(filename="", mode="wb", fileobj=raw, mtime=timestamp) as compressed:
                with tarfile.open(fileobj=compressed, mode="w") as bundle:
                    bundle.add(stage, arcname=stem, filter=normalize)
        validate_archive(archive, release_version, sha, target)
        checksum(archive)
        print(archive)


def source(output: Path) -> None:
    output.mkdir(parents=True, exist_ok=True)
    release_version = version()
    stem = f"pikarust-{release_version}-source"
    archive = output / f"{stem}.tar.gz"
    # Archive committed LFS pointers even when the checkout has hydrated weights.
    subprocess.run(["git", "-c", "filter.lfs.process=", "-c", "filter.lfs.smudge=",
                    "-c", "filter.lfs.required=false", "archive", "--format=tar.gz", f"--prefix={stem}/",
                    f"--output={archive.resolve()}", "HEAD"], cwd=ROOT, check=True)
    validate_archive(archive, release_version, checked_sha(git("rev-parse", "HEAD")), "source")
    checksum(archive)
    print(archive)


def verify_artifacts(output: Path, sha: str, targets=TARGETS) -> list[Path]:
    release_version = version()
    archives = [(archive_name(release_version, target), target) for target in (*targets, "source")]
    expected = {name for filename, _ in archives for name in (filename, filename + ".sha256")}
    actual = {path.name for path in output.iterdir()}
    if actual != expected:
        raise ValueError(f"release artifact set differs: missing={sorted(expected - actual)}, unexpected={sorted(actual - expected)}")
    for filename, target in archives:
        path = output / filename
        if (output / (filename + ".sha256")).read_bytes() != f"{digest(path)}  {filename}\n".encode():
            raise ValueError(f"checksum mismatch: {filename}")
        validate_archive(path, release_version, sha, target)
    return sorted(output / name for name in expected)


def matching_assets(existing: list, artifacts: list[Path]) -> list[Path]:
    """Preserve uploaded bytes; never clobber even an existing draft asset."""
    expected = {path.name: path for path in artifacts}
    seen = set()
    for asset in existing:
        name = asset["name"]
        if name not in expected or name in seen:
            raise ValueError(f"unexpected existing release asset: {name}")
        if asset.get("state") != "uploaded" or asset.get("digest") != f"sha256:{digest(expected[name])}":
            raise ValueError(f"existing asset differs or is incomplete: {name}; inspect the draft before retrying")
        seen.add(name)
    return [path for name, path in expected.items() if name not in seen]


def finalize(mode: str, expected_sha: str, output: Path) -> None:
    require_dispatch()
    sha = checked_sha(expected_sha)
    if git("rev-parse", "HEAD") != sha:
        raise ValueError("source checkout differs from the selected commit")
    run = successful_ci(sha)
    release_version = version()
    tag = f"v{release_version}"
    artifacts = verify_artifacts(output, sha)
    existing_tag, existing_release = release_state(tag, sha)
    if existing_release:
        matching_assets(existing_release.get("assets", []), artifacts)
    if existing_tag is None:
        github_request("git/refs", "POST", {"ref": f"refs/tags/{tag}", "sha": sha})
    # Recheck after creating a tag: another actor may have published a draft.
    _, release = release_state(tag, sha)
    if release is None:
        release = github_request("releases", "POST", {
            "tag_name": tag, "target_commitish": sha, "name": tag, "draft": True,
            "prerelease": "-" in release_version.split("+", 1)[0],
            "body": f"PikaRust native applications and Rust library sources.\n\nCommit: `{sha}`\n"
                    f"Validated by [CI](https://github.com/{os.environ['GITHUB_REPOSITORY']}/actions/runs/{run['id']}).\n\n"
                    "NNUE weights are not bundled. See models/README.md, models/LICENSE-NNUE, "
                    "and scripts/reference.lock for model setup and terms. Source notices and model terms "
                    "have separate scopes; see the README license section. Cargo registry publication is disabled.\n",
        })
    missing = matching_assets(release.get("assets", []), artifacts)
    if missing:
        subprocess.run(["gh", "release", "upload", tag, *map(str, missing),
                        "--repo", os.environ["GITHUB_REPOSITORY"]], check=True)
    if mode == "publish":
        successful_ci(sha)
    # Keep the mutable tag/draft/asset check immediately before publication.
    _, complete = release_state(tag, sha)
    if not complete or complete["id"] != release["id"] or matching_assets(complete.get("assets", []), artifacts):
        raise ValueError("release assets are incomplete or the draft changed while uploading")
    if mode == "publish":
        github_request(f"releases/{complete['id']}", "PATCH", {"draft": False, "make_latest": "legacy"})
    write_outputs({"tag": tag, "sha": sha, "mode": mode,
                   "url": complete["html_url"]})


def main() -> None:
    global ROOT
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=ROOT, help="selected source checkout; controller code stays unchanged")
    commands = parser.add_subparsers(dest="command", required=True)
    resolve_parser = commands.add_parser("resolve", help="find successful CI for an exact main commit")
    resolve_parser.add_argument("--commit", default="", help="default: the main dispatch commit")
    commands.add_parser("plan", help="validate the source version and existing tag/draft")
    for command in ("package", "source", "verify", "finalize"):
        command_parser = commands.add_parser(command)
        command_parser.add_argument("--output", type=Path, default=ROOT / "target/release-artifacts")
        if command == "package":
            command_parser.add_argument("--target", required=True, choices=TARGETS)
        if command == "verify":
            command_parser.add_argument("--target", choices=TARGETS, help="verify one platform plus source; default: all platforms")
        if command == "finalize":
            command_parser.add_argument("--mode", required=True, choices=("draft", "publish"))
            command_parser.add_argument("--sha", required=True)
    args = parser.parse_args()
    ROOT = args.root.resolve()
    if args.command == "resolve":
        resolve(args.commit)
    elif args.command == "plan":
        plan()
    elif args.command == "package":
        package(args.target, args.output)
    elif args.command == "source":
        source(args.output)
    elif args.command == "verify":
        artifacts = verify_artifacts(args.output, checked_sha(git("rev-parse", "HEAD")),
                                     (args.target,) if args.target else TARGETS)
        print(f"Verified {len(artifacts)} release files for {git('rev-parse', 'HEAD')}")
    else:
        finalize(args.mode, args.sha, args.output)


if __name__ == "__main__":
    try:
        main()
    except (KeyError, OSError, ValueError, subprocess.CalledProcessError) as error:
        sys.exit(str(error))
