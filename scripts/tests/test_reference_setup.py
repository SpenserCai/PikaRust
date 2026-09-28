"""Exercise reference selection and isolation using tiny local Git fixtures."""

import hashlib
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


SCRIPT = Path(__file__).parents[1] / "setup-pikafish.sh"


class ReferenceSetupTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        (self.root / "scripts").mkdir()
        (self.root / "models").mkdir()
        shutil.copy2(SCRIPT, self.root / "scripts/setup-pikafish.sh")
        self.environment = os.environ.copy()
        for key in ("PIKAFISH_SOURCE_DIR", "PIKAFISH_OUTPUT_DIR", "PIKAFISH_ARCH",
                    "PIKARUST_NNUE_MODEL", "BUILD_JOBS"):
            self.environment.pop(key, None)
        self.environment.update(GIT_CONFIG_GLOBAL=os.devnull, GIT_CONFIG_NOSYSTEM="1",
                                GIT_TERMINAL_PROMPT="0", BUILD_JOBS="1")
        self.upstream = self.root / "upstream"
        self.upstream.mkdir()
        self.git(self.upstream, "init", "--initial-branch=main")
        (self.upstream / "src").mkdir()
        (self.upstream / "src/Makefile").write_text(
            "build:\n\ttest -f pikafish.nnue\n\ttest ! -e stale.o\n\tcp payload pikafish\n"
        )
        self.previous = self.commit("previous source revision")
        self.current = self.commit("current reference")
        self.model_current = self.root / "models/pikafish.nnue"
        self.model_other = self.root / "models/unrelated.nnue"
        self.model_current.write_bytes(b"current model fixture")
        self.model_other.write_bytes(b"unrelated model fixture")
        self.write_lock("reference.lock", self.current, self.model_current)

    def git(self, directory, *args):
        result = subprocess.run(
            ["git", "-c", "user.name=Reference Test", "-c", "user.email=test@example.invalid", *args],
            cwd=directory, env=self.environment, capture_output=True, text=True, check=True,
        )
        return result.stdout.strip()

    def commit(self, payload):
        (self.upstream / "src/payload").write_text(payload)
        self.git(self.upstream, "add", ".")
        self.git(self.upstream, "commit", "-m", payload)
        return self.git(self.upstream, "rev-parse", "HEAD")

    def write_lock(self, filename, commit, model):
        # Single quoting keeps paths portable when a temporary root has spaces.
        repository = str(self.upstream).replace("'", "'\\''")
        (self.root / "scripts" / filename).write_text(
            f"PIKAFISH_REPOSITORY='{repository}'\n"
            f"PIKAFISH_COMMIT={commit}\n"
            f"PIKAFISH_NNUE_SHA256={hashlib.sha256(model.read_bytes()).hexdigest()}\n"
            f"PIKAFISH_NNUE_FILE={model.relative_to(self.root).as_posix()}\n"
        )

    def setup(self, *args, environment=None):
        return subprocess.run(
            ["bash", str(self.root / "scripts/setup-pikafish.sh"), *args],
            cwd=self.root, env={**self.environment, **(environment or {})},
            capture_output=True, text=True, timeout=20,
        )

    def test_current_model_can_be_verified_at_an_explicit_location(self):
        alternate = self.root / "selected.nnue"
        alternate.write_bytes(self.model_current.read_bytes())
        for environment in ({}, {"PIKARUST_NNUE_MODEL": str(alternate)}):
            with self.subTest(environment=environment):
                result = self.setup("--verify-model", environment=environment)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertIn(hashlib.sha256(self.model_current.read_bytes()).hexdigest(), result.stdout)
        self.assertFalse((self.root / "tests").exists())

    def test_override_cannot_replace_current_model_with_another_model(self):
        result = self.setup("--verify-model",
                            environment={"PIKARUST_NNUE_MODEL": str(self.model_other)})
        self.assertEqual(result.returncode, 1)
        self.assertIn("NNUE hash mismatch", result.stderr)
        self.assertFalse((self.root / "tests").exists())

    def test_missing_and_pointer_models_are_rejected_before_source_setup(self):
        self.model_current.unlink()
        result = self.setup("build")
        self.assertEqual(result.returncode, 1)
        self.assertIn("Missing NNUE", result.stderr)
        self.model_current.write_text(
            "version https://git-lfs.github.com/spec/v1\noid sha256:" + "a" * 64 + "\nsize 100\n"
        )
        result = self.setup("build")
        self.assertEqual(result.returncode, 1)
        self.assertIn("NNUE hash mismatch", result.stderr)
        self.assertFalse((self.root / "tests").exists())

    def test_invalid_arguments_fail_before_side_effects(self):
        for args in (("--unknown",), ("--legacy",), ("--legacy", "--verify-model"),
                     ("--legacy", "--source-only"), ("--legacy", "build"),
                     ("build", "--legacy"), ("--verify-model", "ignored"), ("build", "extra")):
            with self.subTest(args=args):
                result = self.setup(*args)
                self.assertEqual(result.returncode, 2)
                self.assertIn("Usage:", result.stderr)
        self.assertFalse((self.root / "tests").exists())

    def test_source_revisions_are_isolated_and_old_checkout_is_preserved(self):
        root = self.root / "tests/fixtures/pikafish"
        old_checkout = root / "source"
        old_checkout.mkdir(parents=True)
        (old_checkout / "analysis").write_text("preserve existing work")
        for commit in (self.previous, self.current):
            with self.subTest(commit=commit):
                self.write_lock("reference.lock", commit, self.model_current)
                result = self.setup("--source-only")
                self.assertEqual(result.returncode, 0, result.stderr)
                checkout = root / f"source-{commit[:12]}"
                self.assertEqual(self.git(checkout, "rev-parse", "HEAD"), commit)
        self.assertEqual(self.git(root / f"source-{self.previous[:12]}", "rev-parse", "HEAD"), self.previous)
        self.assertEqual((old_checkout / "analysis").read_text(), "preserve existing work")

    def test_mismatched_or_modified_sources_are_rejected_without_reset(self):
        environment = {"PIKAFISH_SOURCE_DIR": str(self.upstream)}
        self.write_lock("reference.lock", self.previous, self.model_current)
        result = self.setup("--source-only", environment=environment)
        self.assertEqual(result.returncode, 1)
        self.assertIn("another revision", result.stderr)
        self.write_lock("reference.lock", self.current, self.model_current)
        (self.upstream / "src/payload").write_text("local changes")
        result = self.setup("--source-only", environment=environment)
        self.assertEqual(result.returncode, 1)
        self.assertIn("tracked modifications", result.stderr)
        self.assertEqual((self.upstream / "src/payload").read_text(), "local changes")
        self.assertEqual(self.git(self.upstream, "rev-parse", "HEAD"), self.current)

    def test_build_from_linked_worktree_excludes_untracked_build_artifacts(self):
        import json

        worktree = self.root / "linked-source"
        self.git(self.upstream, "worktree", "add", "--detach", str(worktree), self.current)
        self.assertTrue((worktree / ".git").is_file())
        (worktree / "src/stale.o").write_text("incompatible old build")
        result = self.setup("build", environment={"PIKAFISH_SOURCE_DIR": str(worktree),
                                                  "PIKAFISH_ARCH": "test-architecture"})
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual((worktree / "src/stale.o").read_text(), "incompatible old build")
        self.assertFalse((worktree / "src/pikafish").exists())
        output = self.root / "tests/fixtures/pikafish"
        self.assertEqual((output / "bin/pikafish").read_text(), "current reference")
        self.assertEqual((output / "bin/pikafish.nnue").read_bytes(), self.model_current.read_bytes())
        metadata = json.loads((output / "reference.json").read_text())
        self.assertEqual(metadata["commit"], self.current)
        self.assertEqual(metadata["architecture"], "test-architecture")
        self.assertEqual(metadata["nnue_sha256"], hashlib.sha256(self.model_current.read_bytes()).hexdigest())
        self.assertEqual(metadata["binary_sha256"], hashlib.sha256(b"current reference").hexdigest())
        self.assertEqual(list(output.glob("build.*")), [])
        result = self.setup("build", environment={"PIKAFISH_SOURCE_DIR": str(worktree),
                                                  "PIKAFISH_OUTPUT_DIR": "separate-output"})
        self.assertEqual(result.returncode, 0, result.stderr)
        separate = self.root / "separate-output"
        self.assertEqual((separate / "bin/pikafish").read_text(), "current reference")
        self.assertEqual((output / "bin/pikafish").read_text(), "current reference")
        self.assertEqual((separate / "bin/pikafish.nnue").read_bytes(), self.model_current.read_bytes())
        self.assertEqual(json.loads((output / "reference.json").read_text()), metadata)


if __name__ == "__main__":
    unittest.main()
