"""Regression tests for the production-like local startup preflight.

The tests copy the shell entrypoint into a temporary repository shape and use a
fake cargo binary under the repository-owned target/ tree so they exercise
preflight behavior without building or starting the engine.
"""

from __future__ import annotations

import json
import os
import shutil
import sqlite3
import subprocess
import tempfile
import textwrap
import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parent.parent
SCRIPT_SOURCE = REPO_ROOT / "scripts" / "start_production_like_local.sh"
TARGET_TMP_ROOT = REPO_ROOT / "target" / "test-tmp"
ADMIN_KEY = "harness_" + "a" * 64


class ProductionLikePreflightTests(unittest.TestCase):
    def setUp(self) -> None:
        TARGET_TMP_ROOT.mkdir(parents=True, exist_ok=True)
        self._tmp = tempfile.TemporaryDirectory(
            prefix="production-like-preflight-",
            dir=TARGET_TMP_ROOT,
        )
        self.root = Path(self._tmp.name)
        self.script = self.root / "scripts" / "start_production_like_local.sh"
        self.env_file = self.root / ".env.production-like.local"
        self.db_path = self.root / ".agent-control-plane" / "production-like" / "local-team.db"
        self.fake_cargo_log = self.root / "target" / "fake-cargo.log"

        (self.root / "scripts").mkdir(parents=True)
        shutil.copyfile(SCRIPT_SOURCE, self.script)
        (self.root / "dashboard" / "out").mkdir(parents=True)
        (self.root / "dashboard" / "out" / "index.html").write_text("<html></html>\n", encoding="utf-8")
        self._install_fake_cargo()

    def tearDown(self) -> None:
        self._tmp.cleanup()

    def _install_fake_cargo(self) -> None:
        fake_bin = self.root / "target" / "fake-bin"
        fake_bin.mkdir(parents=True)
        cargo = fake_bin / "cargo"
        cargo.write_text(
            "#!/usr/bin/env sh\n"
            "printf '%s\\n' \"$@\" > \"$FAKE_CARGO_LOG\"\n"
            "exit 0\n",
            encoding="utf-8",
        )
        cargo.chmod(0o755)
        self.fake_path = f"{fake_bin}{os.pathsep}{os.environ.get('PATH', '')}"

    def _write_env(self) -> None:
        self.env_file.write_text(
            textwrap.dedent(
                f"""
                ACP_REQUIRE_AUTH=1
                ACP_TRUSTED_LOCAL_PROFILE=1
                ACP_ADMIN_API_KEY={ADMIN_KEY}
                ACP_PROVIDER_TYPE=anthropic
                ACP_API_KEY=ACP_PROVIDER_SECRET
                ACP_PROVIDER_SECRET=placeholder
                ACP_DB_PATH=.agent-control-plane/production-like/local-team.db
                ACP_BACKUP_DIR=.agent-control-plane/production-like/backups
                ACP_DASHBOARD_DIR=dashboard/out
                """
            ).strip()
            + "\n",
            encoding="utf-8",
        )

    def _write_persisted_endpoints(self, value_json: str | None) -> None:
        self.db_path.parent.mkdir(parents=True, exist_ok=True)
        with sqlite3.connect(self.db_path) as connection:
            connection.execute("CREATE TABLE local_config (key TEXT PRIMARY KEY, value_json TEXT NOT NULL)")
            if value_json is not None:
                connection.execute(
                    "INSERT INTO local_config (key, value_json) VALUES (?, ?)",
                    ("adaptive_provider_endpoints", value_json),
                )

    def _run_script(self) -> subprocess.CompletedProcess[str]:
        env = os.environ.copy()
        env["PATH"] = self.fake_path
        env["FAKE_CARGO_LOG"] = str(self.fake_cargo_log)
        return subprocess.run(
            ["bash", str(self.script), str(self.env_file)],
            cwd=self.root,
            env=env,
            text=True,
            capture_output=True,
            check=False,
        )

    def test_valid_persisted_sqlite_endpoints_pass_preflight_and_reach_cargo(self) -> None:
        self._write_env()
        self._write_persisted_endpoints(
            json.dumps(
                [
                    {
                        "endpoint_id": "local-anthropic",
                        "provider_type": "anthropic",
                        "model": "test-model",
                        "credential_env": "ACP_PROVIDER_SECRET",
                        "input_cost_per_1k_usd": 0.001,
                        "output_cost_per_1k_usd": 0.002,
                    }
                ]
            )
        )

        result = self._run_script()

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue(self.fake_cargo_log.exists(), "expected fake cargo to be invoked after preflight")
        self.assertEqual(self.fake_cargo_log.read_text(encoding="utf-8").splitlines(), ["run", "-p", "engine"])

    def test_missing_persisted_endpoints_fail_before_cargo(self) -> None:
        self._write_env()
        self._write_persisted_endpoints(None)

        result = self._run_script()

        self.assertNotEqual(result.returncode, 0)
        self.assertIn(
            "requires ACP_ADAPTIVE_PROVIDER_ENDPOINTS_JSON or persisted SQLite provider endpoints",
            result.stderr,
        )
        self.assertFalse(self.fake_cargo_log.exists(), "preflight failure must not reach cargo")

    def test_invalid_persisted_endpoints_fail_before_cargo(self) -> None:
        self._write_env()
        self._write_persisted_endpoints("not-json")

        result = self._run_script()

        self.assertNotEqual(result.returncode, 0)
        self.assertIn(
            "requires ACP_ADAPTIVE_PROVIDER_ENDPOINTS_JSON or persisted SQLite provider endpoints",
            result.stderr,
        )
        self.assertFalse(self.fake_cargo_log.exists(), "preflight failure must not reach cargo")


if __name__ == "__main__":
    unittest.main()
