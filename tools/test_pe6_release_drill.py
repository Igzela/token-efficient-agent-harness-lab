from __future__ import annotations

import importlib.util
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
SPEC = importlib.util.spec_from_file_location("pe6_release_provenance", SCRIPT)
assert SPEC is not None
MODULE = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = MODULE
assert SPEC.loader is not None
SPEC.loader.exec_module(MODULE)


def _metadata(artifact: Path) -> dict[str, Any]:
    return {
        "repository": "Igzela/token-efficient-agent-harness-lab",
        "source_commit": "a" * 40,
        "ref": "refs/tags/v0.1.0",
        "workflow": ".github/workflows/release.yml",
        "workflow_ref": "repo/.github/workflows/release.yml@refs/tags/v0.1.0",
        "run_id": "pe6-release-fixture",
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


def _build_evidence(root: Path) -> dict[str, Path]:
    root.mkdir(parents=True, exist_ok=True)
    artifact = root / "agent-control-plane-v0.1.0-x86_64-unknown-linux-gnu.tar.gz"
    artifact.write_bytes(b"PE-6 disposable release bundle\n")
    metadata = _metadata(artifact)
    artifact_sha = MODULE.sha256_file(artifact)
    sbom = MODULE.build_spdx_sbom(
        metadata=metadata,
        artifact_sha256=artifact_sha,
        artifact_size=artifact.stat().st_size,
        components=[{"name": "engine", "version": "locked", "source": "Cargo.lock"}],
    )
    sbom_path = root / "bundle.spdx.json"
    MODULE.write_canonical_json(sbom_path, sbom)
    attestation = MODULE.build_attestation_fixture(
        metadata=metadata,
        artifact_sha256=artifact_sha,
        sbom_sha256=MODULE.sha256_file(sbom_path),
        identity=MODULE.fixture_identity(metadata),
    )
    attestation_path = root / "bundle.attestation.json"
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
    provenance_path = root / "bundle.provenance.json"
    MODULE.write_canonical_json(provenance_path, provenance)
    external_path = root / "bundle.external.json"
    MODULE.write_canonical_json(external_path, {"verificationResult": {"verified": True}})
    return {
        "artifact": artifact,
        "sbom": sbom_path,
        "attestation": attestation_path,
        "provenance": provenance_path,
        "external": external_path,
    }


class ReleaseOwnerDrillTests(unittest.TestCase):
    def test_release_verification_precedes_activation_and_rolls_back(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            evidence = _build_evidence(root / "evidence")
            fixture = MODULE.verify_bundle(
                artifact_path=evidence["artifact"],
                sbom_path=evidence["sbom"],
                attestation_path=evidence["attestation"],
                provenance_path=evidence["provenance"],
                mode="fixture",
            )
            self.assertEqual(fixture["status"], "verified_fixture")

            tampered = evidence["artifact"]
            tampered.write_bytes(b"tampered PE-6 bundle\n")
            rejected = MODULE.verify_bundle(
                artifact_path=tampered,
                sbom_path=evidence["sbom"],
                attestation_path=evidence["attestation"],
                provenance_path=evidence["provenance"],
                mode="fixture",
            )
            self.assertEqual(rejected["status"], "rejected")
            self.assertIn("ARTIFACT_DIGEST_MISMATCH", rejected["reason_codes"])

            release = root / "release"
            release.mkdir()
            for name in ("upgrade.sh", "install.sh", "release_provenance.py"):
                shutil.copy(ROOT / "scripts" / name, release / name)
            prefix = root / "prefix"
            data = root / "data"
            (prefix / "bin").mkdir(parents=True)
            previous = prefix / "bin" / "agent-control-plane"
            previous.write_text("#!/usr/bin/env bash\necho previous --help\n", encoding="utf-8")
            previous.chmod(0o755)
            previous_digest = MODULE.sha256_file(previous)

            result = subprocess.run(
                [
                    "bash",
                    str(release / "upgrade.sh"),
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
                ],
                cwd=release,
                env={**os.environ, "HOME": str(root / "home")},
                capture_output=True,
                text=True,
                timeout=20,
                check=False,
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(MODULE.sha256_file(previous), previous_digest)
            self.assertFalse((prefix / "bin" / "agent-control-plane.bak").exists())

            install_result = subprocess.run(
                ["bash", str(release / "install.sh"), "--prefix", str(root / "install")],
                cwd=release,
                env={**os.environ, "HOME": str(root / "home")},
                capture_output=True,
                text=True,
                timeout=20,
                check=False,
            )
            self.assertNotEqual(install_result.returncode, 0)
            self.assertFalse((root / "install" / "bin" / "agent-control-plane").exists())


if __name__ == "__main__":
    unittest.main()
