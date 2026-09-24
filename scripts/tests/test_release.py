"""Release boundaries: exact CI, immutable refs, historical sources, and artifacts."""

import importlib.util
import hashlib
import json
import os
from pathlib import Path
import shlex
import shutil
import subprocess
import sys
import tarfile
import tempfile
import unittest
import zipfile
from unittest.mock import patch


SPEC = importlib.util.spec_from_file_location("release", Path(__file__).parents[1] / "release.py")
release = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(release)
VERSION_SPEC = importlib.util.spec_from_file_location("check_release", Path(__file__).parents[1] / "check-release.py")
check_release = importlib.util.module_from_spec(VERSION_SPEC)
VERSION_SPEC.loader.exec_module(check_release)
SHA = "a" * 40
OTHER = "b" * 40


def ci_run(**changes):
    return {"id": 123, "status": "completed", "conclusion": "success", "event": "push",
            "head_branch": "main", "path": ".github/workflows/ci.yml", "workflow_id": 42,
            "head_repository": {"full_name": "owner/repo"}, "repository": {"full_name": "owner/repo"},
            "head_sha": SHA, **changes}


def ci_api(runs=None, main=SHA, status="identical"):
    def get(path):
        if path == "git/ref/heads/main":
            return {"object": {"type": "commit", "sha": main}}
        if path.startswith("compare/"):
            return {"status": status, "base_commit": {"sha": SHA}}
        if path == "actions/workflows/ci.yml":
            return {"id": 42, "path": ".github/workflows/ci.yml"}
        if path.startswith("actions/workflows/42/runs?"):
            values = [ci_run()] if runs is None else runs
            return {"workflow_runs": values, "total_count": len(values)}
        raise AssertionError(f"unexpected API read: {path}")
    return get


class ReleasePolicyTests(unittest.TestCase):
    def setUp(self):
        environment = patch.dict(os.environ, {"GITHUB_REPOSITORY": "owner/repo", "GITHUB_SHA": SHA,
                                               "GITHUB_EVENT_NAME": "workflow_dispatch", "GITHUB_REF": "refs/heads/main"})
        environment.start()
        self.addCleanup(environment.stop)

    def test_default_commit_is_fixed_at_main_dispatch(self):
        with patch.object(release, "github_get", side_effect=ci_api()), patch.object(release, "write_outputs") as output:
            release.resolve("")
        output.assert_called_once_with({"sha": SHA, "ci_run_id": 123})

    def test_only_an_explicit_main_dispatch_can_resolve_or_write(self):
        for changes in ({"GITHUB_EVENT_NAME": "workflow_run"}, {"GITHUB_REF": "refs/heads/feature"}):
            with self.subTest(changes=changes), patch.dict(os.environ, changes), patch.object(release, "github_request") as request:
                with self.assertRaises(ValueError):
                    release.resolve("")
                with self.assertRaises(ValueError):
                    release.finalize("publish", SHA, Path("unused"))
                request.assert_not_called()

    def test_only_complete_successful_ci_push_for_exact_source_can_release(self):
        bad_values = {"status": "in_progress", "conclusion": "failure", "event": "pull_request",
                      "head_branch": "feature", "path": ".github/workflows/other.yml", "workflow_id": 999,
                      "head_repository": {"full_name": "fork/repo"}, "repository": {"full_name": "fork/repo"},
                      "head_sha": OTHER}
        for key, value in bad_values.items():
            with self.subTest(key=key), patch.object(release, "github_get", side_effect=ci_api([ci_run(**{key: value})])):
                with self.assertRaises(ValueError):
                    release.successful_ci(SHA)

    def test_latest_pending_run_blocks_an_older_success(self):
        with patch.object(release, "github_get", side_effect=ci_api([ci_run(), ci_run(id=124, status="queued", conclusion=None)])):
            with self.assertRaises(ValueError):
                release.successful_ci(SHA)

    def test_missing_ci_and_commits_outside_main_fail_closed(self):
        for get in (ci_api([]), ci_api(status="behind"), ci_api(status="diverged")):
            with self.subTest(get=get), patch.object(release, "github_get", side_effect=get):
                with self.assertRaises(ValueError):
                    release.successful_ci(SHA)
        for sha in ("main", "a" * 7, SHA + "\nextra=value"):
            with self.subTest(sha=sha), patch.object(release, "github_get") as get:
                with self.assertRaises(ValueError):
                    release.successful_ci(sha)
                get.assert_not_called()

    def test_historical_main_requires_unchanged_workflow_tree(self):
        with patch.object(release, "github_get", side_effect=ci_api(main=OTHER, status="ahead")):
            with patch.object(release, "workflow_tree", side_effect=["first", "second"]):
                with self.assertRaisesRegex(ValueError, "GITHUB_TOKEN"):
                    release.successful_ci(SHA)
            with patch.object(release, "workflow_tree", return_value="same"):
                self.assertEqual(release.successful_ci(SHA)["id"], 123)

    def test_workflow_tree_uses_nonrecursive_subtrees(self):
        responses = [{"tree": {"sha": "root"}}, {"tree": [{"path": ".github", "type": "tree", "sha": "github"}]},
                     {"tree": [{"path": "workflows", "type": "tree", "sha": "workflows"}]}]
        with patch.object(release, "github_get", side_effect=responses) as get:
            self.assertEqual(release.workflow_tree(SHA), "workflows")
            self.assertEqual([call.args[0] for call in get.call_args_list],
                             [f"git/commits/{SHA}", "git/trees/root", "git/trees/github"])
        with patch.object(release, "github_get", side_effect=[responses[0], {"truncated": True}]):
            with self.assertRaises(ValueError):
                release.workflow_tree(SHA)

    def state(self, existing=None, tag_sha=None):
        with patch.object(release, "tag_commit", return_value=tag_sha), patch.object(release, "find_release", return_value=existing):
            return release.release_state("v0.1.0", SHA)

    def test_draft_lookup_includes_later_pages_and_does_not_require_a_tag(self):
        draft = {"id": 5, "draft": True, "tag_name": "v0.1.0", "target_commitish": SHA}
        first_page = [{"id": index, "tag_name": f"other-{index}", "draft": False} for index in range(100)]
        with patch.object(release, "github_get", side_effect=[first_page, [draft]]) as get:
            self.assertEqual(release.find_release("v0.1.0"), draft)
            self.assertEqual(get.call_args_list[-1].args[0], "releases?per_page=100&page=2")
        with patch.object(release, "github_get", return_value=[draft, draft]):
            with self.assertRaisesRegex(ValueError, "multiple"):
                release.find_release("v0.1.0")

    def test_versions_never_move_and_published_assets_cannot_change(self):
        for existing, tag in ((None, OTHER), ({"draft": False, "tag_name": "v0.1.0", "target_commitish": SHA}, SHA),
                              ({"draft": True, "tag_name": "v0.1.0", "target_commitish": OTHER}, None)):
            with self.subTest(existing=existing, tag=tag), self.assertRaises(ValueError):
                self.state(existing, tag)
        draft = {"draft": True, "tag_name": "v0.1.0", "target_commitish": SHA}
        self.assertEqual(self.state(draft, SHA), (SHA, draft))
        self.assertEqual(self.state(None, SHA), (SHA, None))

    def test_annotated_tags_are_peeled_and_invalid_chains_rejected(self):
        reference = {"ref": "refs/tags/v0.1.0", "object": {"type": "tag", "sha": OTHER}}
        with patch.object(release, "github_get", side_effect=[reference, {"object": {"type": "commit", "sha": SHA}}]):
            self.assertEqual(release.tag_commit("v0.1.0"), SHA)
        with patch.object(release, "github_get", side_effect=[reference, {"object": {"type": "tag", "sha": OTHER}}]):
            with self.assertRaises(ValueError):
                release.tag_commit("v0.1.0")

    def test_retries_preserve_matching_assets_without_clobber(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "archive.tar.gz"
            path.write_bytes(b"verified archive")
            asset = {"name": path.name, "state": "uploaded", "digest": f"sha256:{release.digest(path)}"}
            self.assertEqual(release.matching_assets([asset], [path]), [])
            self.assertEqual(release.matching_assets([], [path]), [path])
            for changed in ({**asset, "digest": "sha256:different"}, {**asset, "state": "starter"},
                            {**asset, "name": "unexpected.nnue"}):
                with self.subTest(changed=changed), self.assertRaises(ValueError):
                    release.matching_assets([changed], [path])

    def finalize_mocks(self, directory):
        artifact = directory / "artifact"
        artifact.write_bytes(b"fixture")
        asset = {"name": artifact.name, "state": "uploaded", "digest": f"sha256:{release.digest(artifact)}"}
        draft = {"id": 5, "tag_name": "v0.1.0", "target_commitish": SHA, "draft": True,
                 "assets": [], "html_url": "https://github.com/owner/repo/releases/tag/v0.1.0"}
        return artifact, asset, draft

    def test_publish_occurs_only_after_assets_and_ci_are_verified_again(self):
        with tempfile.TemporaryDirectory() as temporary:
            artifact, asset, draft = self.finalize_mocks(Path(temporary))
            with patch.object(release, "git", return_value=SHA), patch.object(release, "version", return_value="0.1.0"), \
                 patch.object(release, "successful_ci", return_value=ci_run()) as ci, \
                 patch.object(release, "verify_artifacts", return_value=[artifact]), \
                 patch.object(release, "release_state", side_effect=[(None, None), (SHA, None), (SHA, {**draft, "assets": [asset]})]), \
                 patch.object(release, "github_request", side_effect=[{}, draft, {}]) as request, \
                 patch.object(release.subprocess, "run") as upload, patch.object(release, "write_outputs"):
                release.finalize("publish", SHA, Path(temporary))
            self.assertEqual(ci.call_count, 2)
            self.assertEqual(request.call_args_list[-1].args, ("releases/5", "PATCH", {"draft": False, "make_latest": "legacy"}))
            self.assertNotIn("--clobber", upload.call_args.args[0])

    def test_asset_or_ci_failures_and_publication_races_never_publish(self):
        with tempfile.TemporaryDirectory() as temporary:
            artifact, _, draft = self.finalize_mocks(Path(temporary))
            for failure in ("artifact", "race", "missing_remote", "ci"):
                with self.subTest(failure=failure), patch.object(release, "git", return_value=SHA), \
                     patch.object(release, "version", return_value="0.1.0"), \
                     patch.object(release, "successful_ci", side_effect=ValueError("CI failed") if failure == "ci" else None, return_value=ci_run()), \
                     patch.object(release, "verify_artifacts", side_effect=ValueError("artifact mismatch") if failure == "artifact" else None, return_value=[artifact]), \
                     patch.object(release, "release_state", side_effect=[(SHA, draft), ValueError("published concurrently")] if failure == "race" else [(SHA, draft)] * 3), \
                     patch.object(release, "github_request") as request, patch.object(release.subprocess, "run"):
                    with self.assertRaises(ValueError):
                        release.finalize("publish", SHA, Path(temporary))
                    request.assert_not_called()

    def test_assets_changed_during_final_ci_check_are_not_published(self):
        with tempfile.TemporaryDirectory() as temporary:
            artifact, asset, draft = self.finalize_mocks(Path(temporary))
            draft["assets"] = [asset]
            calls = 0

            def ci(_sha):
                nonlocal calls
                calls += 1
                if calls == 2:
                    draft["assets"] = []
                return ci_run()

            with patch.object(release, "git", return_value=SHA), patch.object(release, "version", return_value="0.1.0"), \
                 patch.object(release, "successful_ci", side_effect=ci), \
                 patch.object(release, "verify_artifacts", return_value=[artifact]), \
                 patch.object(release, "release_state", side_effect=lambda *_: (SHA, draft)), \
                 patch.object(release, "github_request") as request, patch.object(release.subprocess, "run") as upload:
                with self.assertRaisesRegex(ValueError, "incomplete"):
                    release.finalize("publish", SHA, Path(temporary))
                request.assert_not_called()
                upload.assert_not_called()


class ArtifactTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        files = {"Cargo.toml": '[workspace]\nmembers=["crates/example"]\n[workspace.package]\nversion="0.1.0"\nedition="2024"\nrust-version="1.85"\nlicense="GPL-3.0-or-later"\nrepository="https://github.com/owner/repo"\n[workspace.dependencies]\nexample={path="crates/example",version="0.1.0"}\n',
                 "Cargo.lock": 'version=4\n[[package]]\nname="example"\nversion="0.1.0"\n[[package]]\nname="dependency"\nversion="1.0.0"\nsource="registry+https://github.com/rust-lang/crates.io-index"\nchecksum="' + "c" * 64 + '"\n',
                 "crates/example/Cargo.toml": '[package]\nname="example"\nversion.workspace=true\nedition.workspace=true\nrust-version.workspace=true\nlicense.workspace=true\n',
                 ".gitignore": "/target/\n/artifacts/\n/smudge.py\n/smudge-called\n/with-smudge.tar\n",
                 "rust-toolchain.toml": '[toolchain]\nchannel="1.85.0"\n', "README.md": "readme", "CONTRIBUTING.md": "development guide",
                 "docs/releases.md": "release guide", "models/LICENSE-NNUE": "model terms", "models/README.md": "model setup",
                 "models/pikafish.nnue": "version https://git-lfs.github.com/spec/v1\noid sha256:" + "a" * 64 + "\nsize 100000000\n",
                 "scripts/reference.lock": "reference pins"}
        for name in release.LICENSE_FILES:
            files[name] = (release.ROOT / name).read_text(encoding="utf-8")
        package = {"name": "pikarust-web", "version": "0.1.0", "license": "MIT"}
        files["pikarust-web/frontend/package.json"] = json.dumps(package)
        files["pikarust-web/frontend/package-lock.json"] = json.dumps({**package, "packages": {"": package}})
        for name, content in files.items():
            path = self.root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(content, encoding="utf-8")
        subprocess.run(["git", "init", "-q", str(self.root)], check=True)
        subprocess.run(["git", "add", "."], cwd=self.root, check=True)
        subprocess.run(["git", "-c", "user.name=Release test", "-c", "user.email=release@example.invalid", "commit", "-qm", "fixture"], cwd=self.root, check=True)
        self.sha = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=self.root, text=True).strip()
        self.output = self.root / "artifacts"
        for target in release.TARGETS:
            suffix = ".exe" if "windows" in target else ""
            for binary in ("pikarust", "pikarust-server"):
                path = self.root / "target" / target / "release" / f"{binary}{suffix}"
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(b"application fixture")
        for mocked in (patch.object(release, "ROOT", self.root), patch.object(check_release, "ROOT", self.root),
                       patch.object(release, "compiler_version", return_value="rustc fixture"),
                       patch.object(release, "vendor_dependencies", side_effect=self.fixture_vendor)):
            mocked.start()
            self.addCleanup(mocked.stop)

    def fixture_vendor(self, directory):
        crate = directory / "dependency-1.0.0"
        crate.mkdir(parents=True)
        files = {"Cargo.toml": b'[package]\nname="dependency"\nversion="1.0.0"\nlicense="MIT"\n',
                 "LICENSE": b"Fixture dependency license and copyright notice", "src/lib.rs": b"// fixture source\n",
                 "src/empty.rs": b""}
        for name, contents in files.items():
            path = crate / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(contents)
        (crate / ".cargo-checksum.json").write_text(json.dumps({"package": "c" * 64,
            "files": {name: hashlib.sha256(contents).hexdigest() for name, contents in files.items()}}))
        return '[source.crates-io]\nreplace-with="vendored-sources"\n[source.vendored-sources]\ndirectory="vendor"\n'

    def changed_archive(self, path, remove=None, replacements=None):
        replacements = replacements or {}
        temporary = self.root / "target/archive-edit"
        if temporary.exists():
            shutil.rmtree(temporary)
        temporary.mkdir(parents=True)
        if path.suffix == ".zip":
            with zipfile.ZipFile(path) as archive:
                archive.extractall(temporary)
        else:
            with tarfile.open(path) as archive:
                archive.extractall(temporary, filter="data")
        stage, = temporary.iterdir()
        if remove:
            (stage / remove).unlink()
        for name, contents in replacements.items():
            (stage / name).write_bytes(contents)
        if path.suffix == ".zip":
            with zipfile.ZipFile(path, "w") as archive:
                for item in stage.rglob("*"):
                    if item.is_file():
                        archive.write(item, item.relative_to(temporary))
        else:
            release.write_tar(stage, path, 1, self.sha if stage.name.endswith("-source") else None)

    def test_all_archives_are_model_free_reproducible_and_tied_to_source(self):
        for target in release.TARGETS:
            release.package(target, self.output)
            archive = self.output / release.archive_name("0.1.0", target)
            original = release.digest(archive)
            os.utime(self.root / "models/LICENSE-NNUE", (1, 1))
            release.package(target, self.output)
            self.assertEqual(release.digest(archive), original)
        release.source(self.output)
        self.assertEqual(len(release.verify_artifacts(self.output, self.sha)), 8)
        with self.assertRaisesRegex(ValueError, "metadata"):
            release.verify_artifacts(self.output, OTHER)

    def test_single_target_local_verification_and_corrupt_checksum(self):
        target = release.TARGETS[0]
        release.package(target, self.output)
        release.source(self.output)
        self.assertEqual(len(release.verify_artifacts(self.output, self.sha, (target,))), 4)
        archive = self.output / release.archive_name("0.1.0", target)
        archive.write_bytes(archive.read_bytes() + b"corruption")
        with self.assertRaisesRegex(ValueError, "checksum"):
            release.verify_artifacts(self.output, self.sha, (target,))

    def test_missing_and_unexpected_assets_fail_before_publication(self):
        self.output.mkdir()
        with self.assertRaisesRegex(ValueError, "missing="):
            release.verify_artifacts(self.output, self.sha)
        (self.output / "weights.nnue").write_bytes(b"not allowed")
        with self.assertRaisesRegex(ValueError, "unexpected="):
            release.verify_artifacts(self.output, self.sha)

    def test_source_archive_rejects_real_model_weights(self):
        (self.root / "models/pikafish.nnue").write_bytes(b"actual model weights")
        subprocess.run(["git", "add", "models/pikafish.nnue"], cwd=self.root, check=True)
        subprocess.run(["git", "-c", "user.name=Release test", "-c", "user.email=release@example.invalid", "commit", "-qm", "weights"], cwd=self.root, check=True)
        with self.assertRaisesRegex(ValueError, "NNUE weights"):
            release.source(self.output)

    def test_source_archive_keeps_lfs_pointer_when_smudge_is_configured(self):
        isolated_config = patch.dict(os.environ, {"GIT_CONFIG_GLOBAL": os.devnull, "GIT_CONFIG_NOSYSTEM": "1"})
        isolated_config.start()
        self.addCleanup(isolated_config.stop)
        (self.root / ".gitattributes").write_text("models/*.nnue filter=lfs\n", encoding="utf-8")
        subprocess.run(["git", "add", ".gitattributes"], cwd=self.root, check=True)
        subprocess.run(["git", "-c", "user.name=Release test", "-c", "user.email=release@example.invalid", "commit", "-qm", "LFS attributes"], cwd=self.root, check=True)
        marker = self.root / "smudge-called"
        filter_script = self.root / "smudge.py"
        filter_script.write_text(
            "import pathlib, sys\nsys.stdin.buffer.read()\n"
            f"pathlib.Path({str(marker)!r}).write_text('called')\n"
            "sys.stdout.buffer.write(b'expanded model weights')\n", encoding="utf-8")
        command = f"{shlex.quote(Path(sys.executable).as_posix())} {shlex.quote(filter_script.as_posix())}"
        clean_command = f"{shlex.quote(Path(sys.executable).as_posix())} -c {shlex.quote('import sys; sys.stdout.buffer.write(sys.stdin.buffer.read())')}"
        for key, value in (("smudge", command), ("clean", clean_command), ("required", "true")):
            subprocess.run(["git", "config", "--local", f"filter.lfs.{key}", value], cwd=self.root, check=True)

        # Exercise Git's real conversion path, not a subprocess mock.
        control = self.root / "with-smudge.tar"
        subprocess.run(["git", "archive", "--format=tar", f"--output={control}", "HEAD"], cwd=self.root, check=True)
        with tarfile.open(control) as archive:
            self.assertEqual(archive.extractfile("models/pikafish.nnue").read(), b"expanded model weights")
        self.assertTrue(marker.exists())
        marker.unlink()

        release.source(self.output)
        self.assertFalse(marker.exists())
        pointer = subprocess.check_output(["git", "show", "HEAD:models/pikafish.nnue"], cwd=self.root)
        with tarfile.open(self.output / "pikarust-0.1.0-source.tar.gz") as archive:
            self.assertEqual(archive.extractfile("pikarust-0.1.0-source/models/pikafish.nnue").read(), pointer)

    def test_tar_zip_and_source_require_full_licenses_and_dependency_notices(self):
        for target in (release.TARGETS[0], release.TARGETS[2], "source"):
            if target == "source":
                release.source(self.output)
            else:
                release.package(target, self.output)
            archive = self.output / release.archive_name("0.1.0", target)
            original = archive.read_bytes()
            for name in (*release.LICENSE_FILES, "notices/dependencies/dependency-1.0.0/LICENSE"):
                with self.subTest(target=target, missing=name):
                    archive.write_bytes(original)
                    self.changed_archive(archive, remove=name)
                    with self.assertRaisesRegex(ValueError, "missing or empty"):
                        release.validate_archive(archive, "0.1.0", self.sha, target)
            for name in ("LICENSE", "LICENSE-MIT"):
                archive.write_bytes(original)
                self.changed_archive(archive, replacements={name: b"License summary"})
                with self.subTest(truncated=name), self.assertRaisesRegex(ValueError, "complete"):
                    release.validate_archive(archive, "0.1.0", self.sha, target)

    def test_source_requires_complete_exact_committed_project_and_vendored_dependencies(self):
        archive = release.source(self.output)
        original = archive.read_bytes()
        for name in ("crates/example/Cargo.toml", "vendor/dependency-1.0.0/src/lib.rs"):
            archive.write_bytes(original)
            self.changed_archive(archive, remove=name)
            with self.subTest(missing=name), self.assertRaises(ValueError):
                release.validate_archive(archive, "0.1.0", self.sha, "source")
        for name in ("crates/example/Cargo.toml", "vendor/dependency-1.0.0/src/lib.rs",
                     ".cargo/config.toml", "notices/dependencies/dependency-1.0.0/LICENSE"):
            archive.write_bytes(original)
            self.changed_archive(archive, replacements={name: b"changed = true\n"})
            with self.subTest(changed=name), self.assertRaises(ValueError):
                release.validate_archive(archive, "0.1.0", self.sha, "source")

    def test_binary_metadata_must_identify_matching_source_and_gpl(self):
        target = release.TARGETS[0]
        release.package(target, self.output)
        archive = self.output / release.archive_name("0.1.0", target)
        original = archive.read_bytes()
        with tarfile.open(archive) as bundle:
            metadata = json.load(bundle.extractfile(f"pikarust-0.1.0-{target}/build.json"))
        for key, value in (("code_license", "MIT"), ("corresponding_source", {"commit": OTHER})):
            archive.write_bytes(original)
            self.changed_archive(archive, replacements={"build.json": json.dumps({**metadata, key: value}).encode()})
            with self.subTest(key=key), self.assertRaisesRegex(ValueError, "matching GPL corresponding source"):
                release.validate_archive(archive, "0.1.0", self.sha, target)

    def test_dependency_missing_license_requires_explicit_versioned_supplement(self):
        vendor = self.root / "target/vendor"
        self.fixture_vendor(vendor)
        (vendor / "dependency-1.0.0/LICENSE").unlink()
        with self.assertRaisesRegex(ValueError, "no dependency license"):
            release.dependency_notices(vendor, self.output)
        supplement = self.root / "notices/dependency-supplements/dependency-1.0.0"
        supplement.mkdir(parents=True)
        (supplement / "SOURCE.md").write_text("Fixture upstream commit and source URL")
        with self.assertRaisesRegex(ValueError, "verified license text"):
            release.dependency_notices(vendor, self.output)
        (supplement / "LICENSE").write_text("Fixture upstream full license")
        release.dependency_notices(vendor, self.output)
        notices = json.loads((self.output / "notices/dependencies/manifest.json").read_text())
        self.assertEqual(notices[0]["notice_source"], "supplement")
        self.assertIn("SOURCE.md", notices[0]["files"])

    def test_distribution_rejects_tracked_and_untracked_dirty_source(self):
        self.assertEqual(release.clean_commit(), self.sha)
        readme = self.root / "README.md"
        original = readme.read_bytes()
        readme.write_bytes(b"local edit")
        with self.assertRaisesRegex(ValueError, "clean committed"):
            release.source(self.output)
        readme.write_bytes(original)
        (self.root / "untracked-source.rs").write_bytes(b"// new source")
        with self.assertRaisesRegex(ValueError, "clean committed"):
            release.package(release.TARGETS[0], self.output)

    def test_historical_mit_engine_metadata_cannot_be_released(self):
        manifest = self.root / "Cargo.toml"
        manifest.write_text(manifest.read_text().replace('license="GPL-3.0-or-later"', 'license="MIT"'))
        with self.assertRaisesRegex(ValueError, "historical MIT"):
            check_release.validate_workspace()

    def test_npm_runtime_notices_require_actual_nonempty_license_text(self):
        frontend = self.root / "pikarust-web/frontend"
        package = self.root / "target/node_modules/runtime"
        package.mkdir(parents=True)
        (package / "package.json").write_text(json.dumps({"name": "runtime", "version": "1.0.0", "license": "MIT"}))
        result = subprocess.CompletedProcess([], 0, stdout=f"{frontend}\n{package}\n")
        with patch.object(release.subprocess, "run", return_value=result):
            with self.assertRaisesRegex(ValueError, "license text is missing"):
                release.npm_notices(self.output)
            (package / "LICENSE").write_bytes(b"")
            with self.assertRaisesRegex(ValueError, "empty npm license"):
                release.npm_notices(self.output)
            (package / "LICENSE").write_bytes(b"Fixture full runtime license")
            release.npm_notices(self.output)
        self.assertEqual((self.output / "notices/dependencies/npm/runtime@1.0.0/LICENSE").read_bytes(), b"Fixture full runtime license")

    def test_frontend_license_scope_must_remain_mit(self):
        frontend = self.root / "pikarust-web/frontend"
        for filename, root in (("package.json", False), ("package-lock.json", True)):
            path = frontend / filename
            original = path.read_text()
            data = json.loads(original)
            package = data["packages"][""] if root else data
            package["license"] = "GPL-3.0-or-later"
            path.write_text(json.dumps(data))
            with self.subTest(filename=filename), self.assertRaisesRegex(ValueError, "license must remain MIT"):
                check_release.check_frontend_version("0.1.0")
            path.write_text(original)

    def test_lockfile_workspace_version_cannot_lag_manifest(self):
        self.assertEqual(check_release.validate_workspace()["version"], "0.1.0")
        lock = self.root / "Cargo.lock"
        lock.write_text(lock.read_text().replace('version="0.1.0"', 'version="0.0.1"'))
        with self.assertRaisesRegex(ValueError, "Cargo.lock"):
            check_release.validate_workspace()

    def test_frontend_manifest_and_both_lock_versions_must_match_workspace(self):
        frontend = self.root / "pikarust-web/frontend"
        manifest = frontend / "package.json"
        lockfile = frontend / "package-lock.json"
        original_manifest = json.loads(manifest.read_text())
        original_lock = json.loads(lockfile.read_text())
        for stale_part in ("manifest", "lock", "lock_root"):
            package = dict(original_manifest)
            lock = json.loads(json.dumps(original_lock))
            if stale_part == "manifest":
                package["version"] = "0.0.1"
            elif stale_part == "lock":
                lock["version"] = "0.0.1"
            else:
                lock["packages"][""]["version"] = "0.0.1"
            manifest.write_text(json.dumps(package))
            lockfile.write_text(json.dumps(lock))
            with self.subTest(stale_part=stale_part), self.assertRaises(ValueError):
                check_release.check_frontend_version("0.1.0")

    def test_release_version_obeys_cargo_semver(self):
        for valid in ("0.1.0", "1.2.3-alpha.1", "1.2.3+build.01", "1.2.3-0.a-1+build"):
            self.assertIsNotNone(check_release.SEMVER.fullmatch(valid))
        for invalid in ("01.2.3", "1.02.3", "1.2.3-alpha..1", "1.2.3-01", "1.2.3+", "1.2.3-"):
            self.assertIsNone(check_release.SEMVER.fullmatch(invalid))


if __name__ == "__main__":
    unittest.main()
