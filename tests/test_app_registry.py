"""Tests for the local Harness app repository registry."""

from __future__ import annotations

import json
import tempfile
import unittest
from pathlib import Path

from harness_core.app_registry import AppRegistry, AppRegistryError, RepoRef, APP_REGISTRY_SCHEMA_VERSION


class AppRegistryTests(unittest.TestCase):
    def test_add_local_repo_resolves_absolute_path(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            registry = AppRegistry.empty().add_repo(
                RepoRef(id="local-repo", name="Local Repo", kind="local", path=str(root))
            )

            repo = registry.get_repo("local-repo")
            self.assertIsNotNone(repo)
            self.assertEqual("local", repo.kind)
            self.assertEqual(str(root.resolve()), repo.path)

    def test_add_remote_repo_is_metadata_only(self):
        registry = AppRegistry.empty().add_repo(
            RepoRef(
                id="remote-repo",
                name="Remote Repo",
                kind="remote",
                url="https://github.com/example/project.git",
                branch="main",
            )
        )

        repo = registry.get_repo("remote-repo")
        self.assertIsNotNone(repo)
        self.assertEqual("remote", repo.kind)
        self.assertEqual("https://github.com/example/project.git", repo.url)
        self.assertIsNone(repo.path)

    def test_duplicate_repo_id_is_rejected(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            registry = AppRegistry.empty().add_repo(
                RepoRef(id="same", name="First", kind="local", path=str(root))
            )

            with self.assertRaises(AppRegistryError):
                registry.add_repo(RepoRef(id="same", name="Second", kind="local", path=str(root)))

    def test_invalid_local_path_is_rejected(self):
        with tempfile.TemporaryDirectory() as tmp:
            missing = Path(tmp) / "missing"
            with self.assertRaises(AppRegistryError):
                AppRegistry.empty().add_repo(
                    RepoRef(id="missing", name="Missing", kind="local", path=str(missing))
                )

    def test_remote_repo_with_path_is_rejected(self):
        with self.assertRaises(AppRegistryError):
            AppRegistry.empty().add_repo(
                RepoRef(
                    id="bad-remote",
                    name="Bad Remote",
                    kind="remote",
                    url="https://github.com/example/project.git",
                    path="/tmp/project",
                )
            )

    def test_save_and_load_registry(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            registry_path = root / "registry.json"
            target = root / "target"
            target.mkdir()
            registry = AppRegistry.empty().add_repo(
                RepoRef(id="target", name="Target", kind="local", path=str(target))
            )

            registry.save(registry_path)
            data = json.loads(registry_path.read_text(encoding="utf-8"))
            self.assertEqual(APP_REGISTRY_SCHEMA_VERSION, data["schema_version"])

            loaded = AppRegistry.load(registry_path)
            self.assertEqual("Target", loaded.get_repo("target").name)


if __name__ == "__main__":
    unittest.main()
