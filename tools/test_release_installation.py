from __future__ import annotations

import importlib.util
import json
import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "release_provenance.py"
SPEC = importlib.util.spec_from_file_location("release_provenance_for_installation", SCRIPT)
assert SPEC is not None
MODULE = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = MODULE
assert SPEC.loader is not None
SPEC.loader.exec_module(MODULE)


def metadata_for(artifact: Path) -> dict[str, Any]:
    return {
        "repository": "Igzela/token-efficient-agent-harness-lab",
        "source_commit": "a" * 40,
        "ref": "refs/tags/v0.1.0",
        "workflow": ".github/workflows/release.yml",
        "workflow_ref": "repo/.github/workflows/release.yml@refs/tags/v0.1.0",
        "run_id": "fixture-run",
        "run_attempt": 1,
        "job": "fixture-build",
        "builder_id": "fixture-builder",
        "target_os": "linux",
        "target_architecture": "x86_64",
        "target_triple": "x86_64-unknown-linux-gnu",
        "package_kind": "package",
        "artifact_name": artifact.name,
        "artifact_media_type": "application/vnd.acp.release+tar",
        "previous_known_good": "v0.0.9",
        "rollback_target": "v0.0.9",
        "publication_mode": "dry-run",
        "lockfiles": [{"path": "Cargo.lock", "sha256": "b" * 64}],
        "build_inputs": [{"path": "engine/Cargo.toml", "sha256": "c" * 64}],
    }


def build_evidence(root: Path) -> dict[str, Path]:
    root.mkdir(parents=True, exist_ok=True)
    artifact = root / "agent-control-plane-v0.1.0-x86_64-unknown-linux-gnu.tar.gz"
    artifact.write_bytes(b"fixture archive bytes\n")
    metadata = metadata_for(artifact)
    artifact_sha = MODULE.sha256_file(artifact)
    sbom = MODULE.build_spdx_sbom(
        metadata=metadata,
        artifact_sha256=artifact_sha,
        artifact_size=artifact.stat().st_size,
        components=[{"name": "engine", "version": "locked", "source": "Cargo.lock"}],
    )
    sbom_path = root / "release.spdx.json"
    MODULE.write_canonical_json(sbom_path, sbom)
    identity = MODULE.fixture_identity(metadata)
    attestation = MODULE.build_attestation_fixture(
        metadata=metadata,
        artifact_sha256=artifact_sha,
        sbom_sha256=MODULE.sha256_file(sbom_path),
        identity=identity,
    )
    attestation_path = root / "release.attestation.json"
    MODULE.write_canonical_json(attestation_path, attestation)
    provenance = MODULE.build_provenance(
        metadata=metadata,
        artifact_sha256=artifact_sha,
        artifact_size=artifact.stat().st_size,
        sbom=sbom,
        sbom_path=sbom_path,
        attestation=attestation,
        attestation_path=attestation_path,
    )
    provenance_path = root / "release.provenance.json"
    MODULE.write_canonical_json(provenance_path, provenance)
    external_path = root / "external-verification.json"
    MODULE.write_canonical_json(external_path, {"verificationResult": {"verified": True}})
    return {
        "artifact": artifact,
        "sbom": sbom_path,
        "attestation": attestation_path,
        "provenance": provenance_path,
        "external": external_path,
    }


class ReleaseInstallationTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)

    def tearDown(self) -> None:
        self.temp.cleanup()

    def make_release_dir(self) -> Path:
        release = self.root / "release"
        release.mkdir()
        shutil.copy(ROOT / "scripts" / "upgrade.sh", release / "upgrade.sh")
        shutil.copy(ROOT / "scripts" / "install.sh", release / "install.sh")
        shutil.copy(ROOT / "scripts" / "release_provenance.py", release / "release_provenance.py")
        (release / "dashboard").mkdir()
        (release / "dashboard" / "index.html").write_text("new dashboard\n")
        (release / "engine").write_text("#!/usr/bin/env bash\necho new --help\n")
        (release / "engine").chmod(0o755)
        return release

    def run_script(self, script: Path, *args: str) -> subprocess.CompletedProcess[str]:
        env = os.environ.copy()
        env["HOME"] = str(self.root / "home")
        return subprocess.run(
            ["bash", str(script), *args],
            cwd=script.parent,
            env=env,
            text=True,
            capture_output=True,
            check=False,
        )

    def test_install_fails_closed_without_evidence(self) -> None:
        release = self.make_release_dir()
        prefix = self.root / "prefix"
        result = self.run_script(release / "install.sh", "--prefix", str(prefix))
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse((prefix / "bin" / "agent-control-plane").exists())

    def test_fixture_identity_cannot_upgrade_an_existing_installation(self) -> None:
        release = self.make_release_dir()
        evidence = build_evidence(self.root / "fixture-evidence")
        prefix = self.root / "prefix"
        data = self.root / "data"
        (prefix / "bin").mkdir(parents=True)
        old = prefix / "bin" / "agent-control-plane"
        old.write_text("#!/usr/bin/env bash\necho old --help\n")
        old.chmod(0o755)
        before = MODULE.sha256_file(old)
        result = self.run_script(
            release / "upgrade.sh",
            "--prefix",
            str(prefix),
            "--data-dir",
            str(data),
            "--artifact",
            str(evidence["artifact"]),
            "--sbom",
            str(evidence["sbom"]),
            "--attestation",
            str(evidence["attestation"]),
            "--provenance",
            str(evidence["provenance"]),
            "--external-verification",
            str(evidence["external"]),
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(before, MODULE.sha256_file(old))
        self.assertFalse((prefix / "bin" / "agent-control-plane.bak").exists())

    def test_development_upgrade_is_atomic_and_health_checked(self) -> None:
        release = self.make_release_dir()
        prefix = self.root / "prefix"
        data = self.root / "data"
        (prefix / "bin").mkdir(parents=True)
        old = prefix / "bin" / "agent-control-plane"
        old.write_text("#!/usr/bin/env bash\necho old --help\n")
        old.chmod(0o755)

        result = self.run_script(
            release / "upgrade.sh",
            "--prefix",
            str(prefix),
            "--data-dir",
            str(data),
            "--development",
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("new", old.read_text())
        self.assertIn("old", (prefix / "bin" / "agent-control-plane.bak").read_text())
        self.assertEqual((data / "dashboard" / "index.html").read_text(), "new dashboard\n")


if __name__ == "__main__":
    unittest.main()
