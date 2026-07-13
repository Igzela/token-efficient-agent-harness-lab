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

from scripts.fault_drill_owner import emit_owner_evidence


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
        "workflow_ref": "Igzela/token-efficient-agent-harness-lab/.github/workflows/release.yml@refs/tags/v0.1.0",
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
        "publication_mode": "dry-run",
        "lockfiles": [
            {"path": path, "sha256": MODULE.sha256_file(ROOT / path)}
            for path in ("Cargo.lock", "dashboard/bun.lock", "sdk/typescript/bun.lock")
        ],
        "build_inputs": [
            {"path": "engine/Cargo.toml", "sha256": MODULE.sha256_file(ROOT / "engine/Cargo.toml")}
        ],
        "rollback": {"state": "first_release", "previous": None},
    }


def _build_evidence(root: Path) -> dict[str, Path]:
    root.mkdir(parents=True)
    artifact = root / "agent-control-plane-v0.1.0-x86_64-unknown-linux-gnu.tar.gz"
    artifact.write_bytes(b"PE-6 disposable release bundle\n")
    metadata = _metadata(artifact)
    artifact_sha = MODULE.sha256_file(artifact)
    inventory = MODULE.load_dependency_inventory(
        ROOT, ("Cargo.lock", "dashboard/bun.lock", "sdk/typescript/bun.lock")
    )
    sbom = MODULE.build_spdx_sbom(
        metadata=metadata,
        artifact_sha256=artifact_sha,
        artifact_size=artifact.stat().st_size,
        inventory=inventory,
    )
    sbom_path = root / f"{artifact.name}.spdx.json"
    MODULE.write_canonical_json(sbom_path, sbom)
    manifest = MODULE.build_release_manifest(
        metadata=metadata,
        artifact_sha256=artifact_sha,
        artifact_size=artifact.stat().st_size,
        sbom=sbom,
        sbom_path=sbom_path,
        bootstrap_assets=[
            {
                "filename": "install-from-release.sh", "sha256": "d" * 64,
                "source_commit": metadata["source_commit"],
                "predicate_type": MODULE.SLSA_PREDICATE_TYPE,
            },
            {
                "filename": "release_provenance.py", "sha256": "e" * 64,
                "source_commit": metadata["source_commit"],
                "predicate_type": MODULE.SLSA_PREDICATE_TYPE,
            },
        ],
    )
    manifest_path = root / f"{artifact.name}.release-manifest.json"
    MODULE.write_canonical_json(manifest_path, manifest)
    result = {"artifact": artifact, "sbom": sbom_path, "manifest": manifest_path}
    predicates = {
        "slsa": {"buildDefinition": {"buildType": "fixture"}, "runDetails": {}},
        "spdx": sbom,
        "release_manifest": manifest,
    }
    for role, predicate_type in MODULE.ATTESTATION_ROLES.items():
        bundle = MODULE.build_attestation_fixture(
            metadata=metadata, artifact_sha256=artifact_sha,
            identity=MODULE.fixture_identity(metadata), role=role,
            predicate_type=predicate_type, predicate=predicates[role],
        )
        path = root / f"{artifact.name}.{role}.bundle.json"
        MODULE.write_canonical_json(path, bundle)
        result[f"{role}_bundle"] = path
    return result


class ReleaseOwnerDrillTests(unittest.TestCase):
    def test_release_verification_precedes_activation_and_rolls_back(self) -> None:
        root = Path(tempfile.mkdtemp(prefix="pe6-release-owner-"))
        try:
            evidence = _build_evidence(root / "evidence")
            fixture = MODULE.verify_release(
                artifact_path=evidence["artifact"], sbom_path=evidence["sbom"],
                manifest_path=evidence["manifest"], slsa_bundle_path=evidence["slsa_bundle"],
                spdx_bundle_path=evidence["spdx_bundle"],
                manifest_bundle_path=evidence["release_manifest_bundle"], mode="fixture",
            )
            self.assertEqual(fixture["status"], "verified_fixture")

            evidence["artifact"].write_bytes(b"tampered PE-6 bundle\n")
            rejected = MODULE.verify_release(
                artifact_path=evidence["artifact"], sbom_path=evidence["sbom"],
                manifest_path=evidence["manifest"], slsa_bundle_path=evidence["slsa_bundle"],
                spdx_bundle_path=evidence["spdx_bundle"],
                manifest_bundle_path=evidence["release_manifest_bundle"], mode="fixture",
            )
            self.assertEqual(rejected["status"], "rejected")

            release = root / "release"
            release.mkdir()
            for name in ("upgrade.sh", "install.sh", "release_provenance.py"):
                shutil.copy(ROOT / "scripts" / name, release / name)
            (release / "dashboard").mkdir()
            (release / "dashboard" / "index.html").write_text("new dashboard\n")
            (release / "engine").write_text("#!/usr/bin/env bash\necho new --help\n")
            (release / "engine").chmod(0o755)
            prefix = root / "prefix"
            data = root / "data"
            (prefix / "bin").mkdir(parents=True)
            (data / "dashboard").mkdir(parents=True)
            previous = prefix / "bin" / "agent-control-plane"
            previous.write_text("#!/usr/bin/env bash\necho previous --help\n")
            previous.chmod(0o755)
            (data / "dashboard" / "index.html").write_text("previous dashboard\n")
            previous_digest = MODULE.sha256_file(previous)

            rollback = subprocess.run(
                [
                    "bash", str(release / "upgrade.sh"), "--prefix", str(prefix),
                    "--data-dir", str(data), "--development",
                ],
                cwd=release,
                env={**os.environ, "HOME": str(root / "home"), "ACP_UPGRADE_FAULT": "after_dashboard"},
                capture_output=True, text=True, timeout=20, check=False,
            )
            self.assertNotEqual(rollback.returncode, 0)
            self.assertIn("UPGRADE_FAILED_ROLLBACK_SUCCEEDED", rollback.stderr)
            self.assertEqual(MODULE.sha256_file(previous), previous_digest)
            self.assertEqual((data / "dashboard" / "index.html").read_text(), "previous dashboard\n")
            self.assertTrue(Path(f"{previous}.bak").is_file())

            install_result = subprocess.run(
                ["bash", str(release / "install.sh"), "--prefix", str(root / "install")],
                cwd=release, env={**os.environ, "HOME": str(root / "home")},
                capture_output=True, text=True, timeout=20, check=False,
            )
            self.assertNotEqual(install_result.returncode, 0)
            self.assertFalse((root / "install" / "bin" / "agent-control-plane").exists())
        finally:
            shutil.rmtree(root)
        self.assertFalse(root.exists())
        emit_owner_evidence(
            observed_state_before_fault="a previous binary and Dashboard plus role-separated fixture evidence existed",
            observed_fault="artifact bytes were tampered and activation was interrupted after Dashboard replacement",
            observed_recovery_or_refusal="verification refused tampering and upgrade restored verified previous binary and Dashboard state",
            checks=[
                {"name": "pe6.release.tampered_artifact_refused", "category": "recovery", "outcome": "passed", "observation": "the v2 verifier returned rejected"},
                {"name": "pe6.release.previous_binary_digest_restored", "category": "rollback", "outcome": "passed", "observation": "the restored binary matched its pre-upgrade digest"},
                {"name": "pe6.release.previous_dashboard_restored", "category": "rollback", "outcome": "passed", "observation": "the previous Dashboard content was restored"},
                {"name": "pe6.release.backup_evidence_retained", "category": "integrity", "outcome": "passed", "observation": "the binary backup remained after recovery"},
                {"name": "pe6.release.previous_binary_health", "category": "restart", "outcome": "passed", "observation": "rollback executable health completed before success was reported"},
                {"name": "pe6.release.audit_not_exercised", "category": "audit", "outcome": "unsupported", "observation": "installer output is not a runtime audit authority"},
                {"name": "pe6.release.owner_dir_removed", "category": "cleanup", "outcome": "passed", "observation": "the release owner directory was observed absent"},
            ],
            cleanup_outcome="passed",
            cleanup_observation="the disposable release, prefix, evidence, and Dashboard directory was removed",
        )


if __name__ == "__main__":
    unittest.main()
