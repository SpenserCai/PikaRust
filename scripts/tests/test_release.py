"""Release safety contracts: verified commits, immutable versions, model-free bundles."""

import hashlib
import importlib.util
import json
import os
from pathlib import Path
import tarfile
import tempfile
import unittest
from unittest.mock import patch
import zipfile


SPEC = importlib.util.spec_from_file_location("release", Path(__file__).parents[1] / "release.py")
release = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(release)
VERSION_SPEC = importlib.util.spec_from_file_location("check_release", Path(__file__).parents[1] / "check-release.py")
check_release = importlib.util.module_from_spec(VERSION_SPEC)
VERSION_SPEC.loader.exec_module(check_release)


class ReleasePolicyTests(unittest.TestCase):
    def setUp(self):
        self.environment = patch.dict(os.environ, {"GITHUB_REPOSITORY": "owner/repo"})
        self.environment.start()
        self.addCleanup(self.environment.stop)

    def test_only_successful_main_push_from_this_repository_can_release(self):
        run = {"conclusion": "success", "event": "push", "head_branch": "main",
               "path": ".github/workflows/ci.yml", "head_repository": {"full_name": "owner/repo"},
               "head_sha": "a" * 40}
        bad_values = {"conclusion": "failure", "event": "pull_request", "head_branch": "feature",
                      "path": ".github/workflows/other.yml", "head_repository": {"full_name": "fork/repo"}}
        for key, value in bad_values.items():
            with self.subTest(key=key), patch.object(release, "github_get", return_value={**run, key: value}):
                with self.assertRaises(ValueError):
                    release.resolve(123)
        with patch.object(release, "github_get", return_value=run), patch.object(release, "write_outputs") as output:
            release.resolve(123)
            output.assert_called_once_with({"sha": "a" * 40})

    def plan(self, existing_release, tag=None):
        def api(path):
            return tag if path.startswith("git/ref/") else existing_release
        with patch.object(release, "github_get", side_effect=api), \
             patch.object(release, "version", return_value="0.1.0"), \
             patch.object(release.subprocess, "check_output", return_value="candidate\n"), \
             patch.object(release, "write_outputs") as output:
            release.plan()
            return output.call_args.args[0]["build"]

    def test_published_version_is_never_rebuilt_or_retargeted(self):
        self.assertEqual(self.plan({"draft": False, "target_commitish": "candidate"}), "false")

    def test_existing_tag_at_another_commit_is_not_retargeted(self):
        self.assertEqual(self.plan(None, {"object": {"type": "commit", "sha": "earlier"}}), "false")

    def test_draft_can_only_resume_at_its_original_commit(self):
        self.assertEqual(self.plan({"draft": True, "target_commitish": "earlier"}), "false")
        self.assertEqual(self.plan({"draft": True, "target_commitish": "candidate"}), "true")

    def test_application_archives_exclude_model_weights_and_include_terms(self):
        for target in ("x86_64-unknown-linux-gnu", "x86_64-pc-windows-msvc"):
            with self.subTest(target=target), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                for filename in ("LICENSE", "README.md", "models/LICENSE-NNUE", "models/README.md",
                                 "models/pikafish.nnue", "scripts/reference.lock", "docs/releases.md"):
                    path = root / filename
                    path.parent.mkdir(parents=True, exist_ok=True)
                    path.write_text("fixture")
                suffix = ".exe" if "windows" in target else ""
                for name in ("pikarust", "pikarust-server"):
                    path = root / "target" / target / "release" / f"{name}{suffix}"
                    path.parent.mkdir(parents=True, exist_ok=True)
                    path.write_text("application fixture")
                output = root / "artifacts"
                with patch.object(release, "ROOT", root), patch.object(release, "version", return_value="0.1.0"), \
                     patch.object(release.subprocess, "check_output", return_value="fixture\n"):
                    release.package(target, output)
                archive = next(path for path in output.iterdir() if not path.name.endswith(".sha256"))
                if suffix:
                    with zipfile.ZipFile(archive) as bundle:
                        names = bundle.namelist()
                else:
                    with tarfile.open(archive) as bundle:
                        names = bundle.getnames()
                self.assertFalse(any(name.endswith(".nnue") for name in names))
                self.assertTrue(any(name.endswith("models/LICENSE-NNUE") for name in names))
                self.assertTrue(any(name.endswith("MODEL-REQUIRED.txt") for name in names))
                self.assertTrue(any(name.endswith("build.json") for name in names))
                digest = hashlib.sha256(archive.read_bytes()).hexdigest()
                self.assertEqual(archive.with_name(archive.name + ".sha256").read_text(),
                                 f"{digest}  {archive.name}\n")

    def test_frontend_manifest_and_both_lock_versions_must_match_workspace(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            frontend = root / "pikarust-web/frontend"
            frontend.mkdir(parents=True)
            package = {"name": "pikarust-web", "version": "0.1.0"}
            lock = {**package, "packages": {"": dict(package)}}
            for stale_part in ("manifest", "lock", "lock_root"):
                manifest = dict(package)
                stale_lock = json.loads(json.dumps(lock))
                if stale_part == "manifest":
                    manifest["version"] = "0.0.1"
                elif stale_part == "lock":
                    stale_lock["version"] = "0.0.1"
                else:
                    stale_lock["packages"][""]["version"] = "0.0.1"
                (frontend / "package.json").write_text(json.dumps(manifest))
                (frontend / "package-lock.json").write_text(json.dumps(stale_lock))
                with self.subTest(stale_part=stale_part), patch.object(check_release, "ROOT", root):
                    with self.assertRaises(ValueError):
                        check_release.check_frontend_version("0.1.0")


if __name__ == "__main__":
    unittest.main()
