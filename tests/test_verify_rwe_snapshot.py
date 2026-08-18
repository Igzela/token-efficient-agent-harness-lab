from contextlib import redirect_stderr
import hashlib
from io import StringIO
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

from scripts import verify_rwe_snapshot


class VerifyRweSnapshotTests(unittest.TestCase):
    def test_canonical_manifest_digest_is_stable_and_field_bound(self) -> None:
        manifest = {"schema_version": "test", "value": 1}
        first = verify_rwe_snapshot.canonical_manifest_digest(manifest)
        manifest["value"] = 2
        self.assertNotEqual(first, verify_rwe_snapshot.canonical_manifest_digest(manifest))

    def test_missing_required_sections_fail_closed(self) -> None:
        manifest = {"schema_version": "test"}
        manifest["manifest_sha256"] = verify_rwe_snapshot.canonical_manifest_digest(manifest)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            path = root / "manifest.json"
            path.write_text(json.dumps(manifest), encoding="utf-8")
            failures = verify_rwe_snapshot.verify(path, root, root)
        self.assertTrue(any("manifest section is missing or malformed" in item for item in failures))

    def test_verify_file_rejects_wrong_digest(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            path = root / "input.txt"
            path.write_text("bound", encoding="utf-8")
            failures: list[str] = []
            verify_rwe_snapshot.verify_file(root, "input.txt", hashlib.sha256(b"other").hexdigest(), failures)
        self.assertEqual(failures, ["snapshot hash mismatch: input.txt"])

    def test_reconstruction_metadata_rejects_effect_authority(self) -> None:
        manifest = {
            "status": "RECONSTRUCTABLE",
            "reconstructable": True,
            "authority": {
                "external_effects": True,
                "provider_calls": False,
                "rwe_authority_consumed": False,
                "target_writes": False,
            },
            "rebuild": {"provider_free": False},
            "reconstruction_recipe": {"target_default_branch_write": True},
        }
        failures: list[str] = []
        verify_rwe_snapshot.verify_reconstruction_metadata(manifest, failures)
        self.assertIn("snapshot authority must deny external_effects", failures)
        self.assertIn("snapshot rebuild is not provider-free", failures)
        self.assertIn("reconstruction recipe permits a target-default-branch write", failures)

    def test_isolated_roots_reject_shared_or_nested_worktrees(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            failures: list[str] = []
            verify_rwe_snapshot.verify_isolated_roots(root, root, failures)
            self.assertEqual(
                failures,
                ["pre-AC source and post-AC harness must use distinct roots"],
            )

            nested = root / "source"
            nested.mkdir()
            failures = []
            verify_rwe_snapshot.verify_isolated_roots(nested, root, failures)
            self.assertIn("pre-AC source and post-AC harness roots must not be nested", failures)

    def test_post_ac_identity_binding_rejects_wrong_head_tree_and_lockfiles(self) -> None:
        failures: list[str] = []
        verify_rwe_snapshot.verify_post_ac_harness(
            Path.cwd(),
            {
                "main_sha": "0" * 40,
                "tree_sha": "0" * 40,
                "cargo_lock_sha256": "0" * 64,
                "rust_toolchain_sha256": "0" * 64,
            },
            failures,
        )
        self.assertIn("post-AC harness HEAD differs from the bound accepted main", failures)
        self.assertIn("post-AC harness tree differs from the bound accepted tree", failures)
        self.assertIn("snapshot hash mismatch: Cargo.lock", failures)
        self.assertIn("snapshot hash mismatch: rust-toolchain.toml", failures)

    def test_manifest_digest_must_match_the_frozen_binding(self) -> None:
        manifest_path = Path(
            "engine/rwe/corpora/rwe-minimum-first-corpus/v2/snapshot/pre_ac_harness_snapshot.v2.json"
        )
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
        manifest["status"] = "tampered"
        manifest["manifest_sha256"] = verify_rwe_snapshot.canonical_manifest_digest(manifest)
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "manifest.json"
            path.write_text(json.dumps(manifest), encoding="utf-8")
            failures = verify_rwe_snapshot.verify(path, Path(directory), Path(directory))
        self.assertIn("snapshot manifest digest differs from the frozen RWE binding", failures)

    def test_frozen_snapshot_metadata_is_provider_free(self) -> None:
        manifest_path = Path(
            "engine/rwe/corpora/rwe-minimum-first-corpus/v2/snapshot/pre_ac_harness_snapshot.v2.json"
        )
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
        failures: list[str] = []
        verify_rwe_snapshot.verify_reconstruction_metadata(manifest, failures)
        self.assertEqual(failures, [])

    def test_git_overlay_paths_include_staged_changes(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for args in (
                ("init", "-b", "main"),
                ("config", "user.name", "Test"),
                ("config", "user.email", "test@example.invalid"),
            ):
                subprocess.run(("git", *args), cwd=root, check=True, capture_output=True)
            (root / "base.txt").write_text("base\n", encoding="utf-8")
            subprocess.run(("git", "add", "base.txt"), cwd=root, check=True)
            subprocess.run(("git", "commit", "-m", "base"), cwd=root, check=True, capture_output=True)
            (root / "recipe.txt").write_text("recipe\n", encoding="utf-8")
            (root / "staged.txt").write_text("staged\n", encoding="utf-8")
            subprocess.run(("git", "add", "staged.txt"), cwd=root, check=True)

            self.assertEqual(
                verify_rwe_snapshot.git_overlay_paths(root),
                ["recipe.txt", "staged.txt"],
            )

    def test_git_queries_override_repository_local_aliases(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            marker = root / "alias-executed"
            for args in (
                ("init", "-b", "main"),
                ("config", "user.name", "Test"),
                ("config", "user.email", "test@example.invalid"),
            ):
                subprocess.run(("git", *args), cwd=root, check=True, capture_output=True)
            subprocess.run(
                ("git", "config", "alias.status", f"!touch {marker}"),
                cwd=root,
                check=True,
                capture_output=True,
            )
            subprocess.run(
                ("git", "config", "core.bare", "true"),
                cwd=root,
                check=True,
                capture_output=True,
            )

            self.assertEqual(verify_rwe_snapshot.git_output(root, "status", "--porcelain"), "")
            self.assertEqual(
                verify_rwe_snapshot.git_output(root, "rev-parse", "--is-bare-repository"),
                "false",
            )
            self.assertFalse(marker.exists())

    def test_copy_git_revision_excludes_ignored_host_artifacts(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for args in (
                ("init", "-b", "main"),
                ("config", "user.name", "Test"),
                ("config", "user.email", "test@example.invalid"),
            ):
                subprocess.run(("git", *args), cwd=root, check=True, capture_output=True)
            (root / ".gitignore").write_text(".venv/\n", encoding="utf-8")
            (root / "tracked.txt").write_text("tracked\n", encoding="utf-8")
            (root / ".venv").mkdir()
            (root / ".venv" / "private.txt").write_text("private\n", encoding="utf-8")
            subprocess.run(("git", "add", ".gitignore", "tracked.txt"), cwd=root, check=True)
            subprocess.run(("git", "commit", "-m", "base"), cwd=root, check=True, capture_output=True)

            destination = root / "copy"
            verify_rwe_snapshot.copy_git_revision(root, "HEAD", destination)

            self.assertEqual((destination / "tracked.txt").read_text(encoding="utf-8"), "tracked\n")
            self.assertFalse((destination / ".venv").exists())

    def test_copy_cache_snapshot_preserves_symlinks_without_dereferencing(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "source"
            source.mkdir()
            (source / "payload").write_text("bound\n", encoding="utf-8")
            (source / "link").symlink_to("payload")
            destination = root / "destination"

            digest = verify_rwe_snapshot.copy_cache_snapshot(source, destination, "test")

            self.assertTrue((destination / "link").is_symlink())
            self.assertEqual((destination / "link").read_text(encoding="utf-8"), "bound\n")
            self.assertEqual(digest, verify_rwe_snapshot.cache_snapshot_digest(destination))

    def test_copy_cache_snapshot_excludes_escaping_symlinks(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "source"
            source.mkdir()
            (source / "payload").write_text("bound\n", encoding="utf-8")
            (source / "escape").symlink_to("/etc/passwd")
            destination = root / "destination"

            digest = verify_rwe_snapshot.copy_cache_snapshot(source, destination, "test")

            self.assertFalse((destination / "escape").is_symlink())
            self.assertEqual(digest, verify_rwe_snapshot.cache_snapshot_digest(destination))

    def test_copy_recipe_overlay_rejects_executable_mode_mismatch(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "recipe.py"
            source.write_text("print('bound')\n", encoding="utf-8")
            destination = root / "destination"
            with self.assertRaisesRegex(OSError, "executable mode differs"):
                verify_rwe_snapshot.copy_recipe_overlay(
                    root, destination, ["recipe.py"], {"recipe.py": 0o755}
                )

    def test_bounded_command_caps_captured_output(self) -> None:
        result = verify_rwe_snapshot._run_bounded_command(
            [sys.executable, "-c", "print('x' * 300000)"],
            cwd=Path("/"),
            environment={"PATH": str(Path(sys.executable).parent)},
            timeout=30,
        )
        self.assertEqual(result.returncode, 0)
        self.assertLessEqual(len(result.stdout.encode()), verify_rwe_snapshot.MAX_TRACE_OUTPUT_BYTES)

    def test_bounded_command_caps_an_unterminated_output_line(self) -> None:
        result = verify_rwe_snapshot._run_bounded_command(
            [sys.executable, "-c", "import sys; sys.stdout.write('x' * 300000)"],
            cwd=Path("/"),
            environment={"PATH": str(Path(sys.executable).parent)},
            timeout=30,
        )
        self.assertEqual(result.returncode, 0)
        self.assertLessEqual(len(result.stdout.encode()), verify_rwe_snapshot.MAX_TRACE_OUTPUT_BYTES)

    def test_main_does_not_report_absolute_paths_from_exceptions(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            manifest_path = root / "manifest.json"
            stderr = StringIO()
            with (
                patch.object(
                    verify_rwe_snapshot,
                    "verify",
                    side_effect=OSError(f"private path: {manifest_path}"),
                ),
                patch.object(
                    sys,
                    "argv",
                    [
                        "verify_rwe_snapshot.py",
                        "--manifest",
                        str(manifest_path),
                        "--source-root",
                        str(root / "source"),
                        "--harness-root",
                        str(root / "harness"),
                        "--post-ac-main-sha",
                        "0" * 40,
                        "--post-ac-tree-sha",
                        "0" * 40,
                        "--post-ac-cargo-lock-sha256",
                        "0" * 64,
                        "--post-ac-rust-toolchain-sha256",
                        "0" * 64,
                    ],
                ),
                redirect_stderr(stderr),
            ):
                status = verify_rwe_snapshot.main()

        self.assertEqual(status, 1)
        self.assertNotIn(str(manifest_path), stderr.getvalue())
        self.assertEqual(stderr.getvalue(), "snapshot verification failed: OSError\n")

    def test_cache_failures_do_not_report_absolute_paths(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            failures: list[str] = []
            with patch.object(
                verify_rwe_snapshot,
                "cache_snapshot_digest",
                side_effect=OSError(f"private path: {root / 'cache'}"),
            ):
                verify_rwe_snapshot.verify_cache_snapshot(root, "expected", "test", failures)

        self.assertEqual(failures, ["test cache snapshot is not verifiable: OSError"])

    def test_trace_failures_do_not_report_child_output(self) -> None:
        failures: list[str] = []
        command = [
            sys.executable,
            "-c",
            "import sys; print('/private/repository/path'); "
            "print('/tmp/secret-output', file=sys.stderr); raise SystemExit(3)",
        ]
        verify_rwe_snapshot._run_provider_free_trace(
            "test_trace",
            command,
            Path("/"),
            {"PATH": str(Path(sys.executable).parent)},
            {},
            set(),
            False,
            failures,
        )

        self.assertEqual(len(failures), 1)
        self.assertNotIn("/private/repository/path", failures[0])
        self.assertNotIn("/tmp/secret-output", failures[0])
        self.assertIn("output=captured", failures[0])

    def test_process_cleanup_refuses_start_time_mismatch(self) -> None:
        with (
            patch.object(verify_rwe_snapshot, "_process_start_time", return_value=123),
            patch.object(os, "kill") as kill,
        ):
            self.assertFalse(verify_rwe_snapshot._terminate_processes({99999: 456}))

        kill.assert_not_called()

    def test_bounded_command_cleans_a_detached_descendant(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            pid_path = Path(directory) / "child.pid"
            child_code = (
                "import os, pathlib, time; os.setsid(); "
                f"pathlib.Path({str(pid_path)!r}).write_text(str(os.getpid())); time.sleep(30)"
            )
            parent_code = (
                "import pathlib, subprocess, sys, time\n"
                "subprocess.Popen([sys.executable, '-c', "
                + repr(child_code)
                + "])\n"
                f"deadline=time.monotonic()+5\n"
                f"path=pathlib.Path({str(pid_path)!r})\n"
                "while not path.exists() and time.monotonic() < deadline:\n"
                "    time.sleep(.01)\n"
            )
            result = verify_rwe_snapshot._run_bounded_command(
                [sys.executable, "-c", parent_code],
                cwd=Path("/"),
                environment={"PATH": str(Path(sys.executable).parent)},
                timeout=30,
            )
            self.assertEqual(result.returncode, 0)
            child_pid = int(pid_path.read_text(encoding="utf-8"))
            with self.assertRaises(ProcessLookupError):
                os.kill(child_pid, 0)

    def test_rust_reconstruction_binding_is_manifest_bound(self) -> None:
        manifest_path = Path(
            "engine/rwe/corpora/rwe-minimum-first-corpus/v2/snapshot/pre_ac_harness_snapshot.v2.json"
        )
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
        failures: list[str] = []

        verify_rwe_snapshot.verify_rust_reconstruction_binding(Path.cwd(), manifest, failures)

        self.assertEqual(failures, [])

    def test_registered_source_commands_are_manifest_bound(self) -> None:
        manifest = {
            "rebuild": {
                "commands": [
                    "git clone source",
                    "git apply recipe",
                    "uv run --locked --project source/apps/api/pyproject.toml --extra dev python source/tools/materialize_sample_baseline.py",
                    "PYTHONPATH=source/apps/api/src uv run --locked --project source/apps/api pytest source/apps/api/tests/ -q",
                ]
            }
        }
        failures: list[str] = []
        registered = verify_rwe_snapshot.registered_source_commands(
            manifest, "/fixed/uv", failures
        )

        self.assertEqual(failures, [])
        assert registered is not None
        materializer_environment, materializer, pytest_environment, pytest = registered
        self.assertEqual(materializer[0], "/fixed/uv")
        self.assertEqual(materializer_environment, {})
        self.assertEqual(pytest_environment, {"PYTHONPATH": "source/apps/api/src"})
        self.assertEqual(pytest[0], "/fixed/uv")


if __name__ == "__main__":
    unittest.main()
