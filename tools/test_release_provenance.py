from __future__ import annotations

import hashlib
import importlib.util
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "release_provenance.py"
SPEC = importlib.util.spec_from_file_location("release_provenance", SCRIPT)
assert SPEC is not None
MODULE = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = MODULE
assert SPEC.loader is not None
SPEC.loader.exec_module(MODULE)


def valid_metadata(artifact: Path) -> dict[str, Any]:
    return {
        "repository": "Igzela/token-efficient-agent-harness-lab",
        "source_commit": "a" * 40,
        "ref": "refs/tags/v0.1.0",
        "workflow": ".github/workflows/release.yml",
        "workflow_ref": "Igzela/token-efficient-agent-harness-lab/.github/workflows/release.yml@refs/tags/v0.1.0",
        "run_id": "123456",
        "run_attempt": 1,
        "job": "build (x86_64-unknown-linux-gnu)",
        "builder_id": "github-hosted:ubuntu-latest",
        "target_os": "linux",
        "target_architecture": "x86_64",
        "target_triple": "x86_64-unknown-linux-gnu",
        "package_kind": "package",
        "artifact_name": artifact.name,
        "artifact_media_type": "application/vnd.acp.release+tar",
        "previous_known_good": "v0.0.9",
        "rollback_target": "v0.0.9",
        "publication_mode": "dry-run",
        "lockfiles": [
            {"path": "Cargo.lock", "sha256": "b" * 64},
            {"path": "dashboard/bun.lock", "sha256": "c" * 64},
        ],
        "build_inputs": [
            {"path": "engine/Cargo.toml", "sha256": "d" * 64},
        ],
    }


class ReleaseProvenanceTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.artifact = self.root / "agent-control-plane-v0.1.0-x86_64-unknown-linux-gnu.tar.gz"
        self.artifact.write_bytes(b"deterministic release payload\n")
        self.metadata = valid_metadata(self.artifact)

    def tearDown(self) -> None:
        self.temp.cleanup()

    def make_bundle(self) -> dict[str, Path]:
        artifact_sha = MODULE.sha256_file(self.artifact)
        sbom = MODULE.build_spdx_sbom(
            metadata=self.metadata,
            artifact_sha256=artifact_sha,
            artifact_size=self.artifact.stat().st_size,
            components=[
                {"name": "engine", "version": "0.1.0", "source": "workspace"},
                {"name": "serde", "version": "1.0.0", "source": "Cargo.lock"},
            ],
        )
        sbom_path = self.root / "release.spdx.json"
        MODULE.write_canonical_json(sbom_path, sbom)

        identity = MODULE.fixture_identity(self.metadata)
        attestation = MODULE.build_attestation_fixture(
            metadata=self.metadata,
            artifact_sha256=artifact_sha,
            sbom_sha256=MODULE.sha256_file(sbom_path),
            identity=identity,
        )
        attestation_path = self.root / "release.attestation.json"
        MODULE.write_canonical_json(attestation_path, attestation)

        provenance = MODULE.build_provenance(
            metadata=self.metadata,
            artifact_sha256=artifact_sha,
            artifact_size=self.artifact.stat().st_size,
            sbom=sbom,
            sbom_path=sbom_path,
            attestation=attestation,
            attestation_path=attestation_path,
        )
        provenance_path = self.root / "release-provenance.json"
        MODULE.write_canonical_json(provenance_path, provenance)
        return {
            "artifact": self.artifact,
            "sbom": sbom_path,
            "attestation": attestation_path,
            "provenance": provenance_path,
        }

    def test_spdx_output_is_deterministic_and_artifact_bound(self) -> None:
        artifact_sha = MODULE.sha256_file(self.artifact)
        first = MODULE.build_spdx_sbom(
            metadata=self.metadata,
            artifact_sha256=artifact_sha,
            artifact_size=self.artifact.stat().st_size,
            components=[{"name": "serde", "version": "1.0.0", "source": "Cargo.lock"}],
        )
        second = MODULE.build_spdx_sbom(
            metadata=self.metadata,
            artifact_sha256=artifact_sha,
            artifact_size=self.artifact.stat().st_size,
            components=[{"name": "serde", "version": "1.0.0", "source": "Cargo.lock"}],
        )
        self.assertEqual(MODULE.canonical_json_bytes(first), MODULE.canonical_json_bytes(second))
        self.assertEqual(first["spdxVersion"], "SPDX-2.3")
        self.assertIn(artifact_sha, first["documentNamespace"])

    def test_container_subject_uses_the_same_deterministic_sbom_contract(self) -> None:
        metadata = dict(
            self.metadata,
            package_kind="container",
            artifact_name="agent-control-plane-image",
            artifact_media_type="application/vnd.oci.image.manifest.v1+json",
            target_triple="linux/amd64",
        )
        sbom = MODULE.build_spdx_sbom(
            metadata=metadata,
            artifact_sha256=MODULE.sha256_file(self.artifact),
            artifact_size=self.artifact.stat().st_size,
            components=[{"name": "engine", "version": "locked", "source": "Cargo.lock"}],
        )
        self.assertIn("package_kind=container", sbom["documentComment"])
        self.assertEqual(sbom["spdxVersion"], "SPDX-2.3")

    def test_valid_fixture_bundle_is_explicitly_non_authoritative(self) -> None:
        paths = self.make_bundle()
        result = MODULE.verify_bundle(
            artifact_path=paths["artifact"],
            sbom_path=paths["sbom"],
            attestation_path=paths["attestation"],
            provenance_path=paths["provenance"],
            mode="fixture",
        )
        self.assertEqual(result["status"], "verified_fixture")
        self.assertIn("FIXTURE_IDENTITY_NON_AUTHORITATIVE", result["reason_codes"])
        self.assertNotEqual(result["status"], "verified")

    def test_fixture_identity_cannot_enable_legacy_production_verification(self) -> None:
        paths = self.make_bundle()
        result = MODULE.verify_bundle(
            artifact_path=paths["artifact"],
            sbom_path=paths["sbom"],
            attestation_path=paths["attestation"],
            provenance_path=paths["provenance"],
            mode="production",
        )
        self.assertEqual(result["status"], "unsupported")
        self.assertEqual(
            result["reason_codes"], ["LEGACY_PRODUCTION_VERIFICATION_UNSUPPORTED"]
        )

    def test_legacy_production_request_is_unsupported_without_transcript(self) -> None:
        paths = self.make_bundle()
        unverified = MODULE.verify_bundle(
            artifact_path=paths["artifact"],
            sbom_path=paths["sbom"],
            attestation_path=paths["attestation"],
            provenance_path=paths["provenance"],
            mode="production",
        )
        self.assertEqual(unverified["status"], "unsupported")
        self.assertEqual(
            unverified["reason_codes"], ["LEGACY_PRODUCTION_VERIFICATION_UNSUPPORTED"]
        )

    def test_unstructured_external_transcript_cannot_authorize_production(self) -> None:
        paths = self.make_bundle()
        external = self.root / "external.json"
        MODULE.write_canonical_json(external, {"verificationResult": {"verified": True}})
        result = MODULE.verify_bundle(
            artifact_path=paths["artifact"],
            sbom_path=paths["sbom"],
            attestation_path=paths["attestation"],
            provenance_path=paths["provenance"],
            mode="production",
            external_verification_path=external,
        )
        self.assertEqual(result["status"], "unsupported")
        self.assertEqual(
            result["reason_codes"], ["LEGACY_PRODUCTION_VERIFICATION_UNSUPPORTED"]
        )

    def test_matching_legacy_external_transcript_cannot_authorize_production(self) -> None:
        paths = self.make_bundle()
        provenance = json.loads(paths["provenance"].read_text())
        provenance["attestation"]["identity"] = MODULE.production_identity(self.metadata)
        MODULE.write_canonical_json(paths["provenance"], provenance)
        artifact_sha = MODULE.sha256_file(paths["artifact"])
        certificate = {
            "oidcIssuer": MODULE.PRODUCTION_ISSUER,
            "sourceRepository": self.metadata["repository"],
            "sourceRepositoryRef": self.metadata["ref"],
            "sourceRepositoryWorkflow": self.metadata["workflow"],
            "subjectAlternativeName": self.metadata["workflow"],
        }
        entries = []
        for predicate_type in (MODULE.SLSA_PREDICATE_TYPE, MODULE.SPDX_PREDICATE_TYPE):
            entries.append(
                {
                    "verificationResult": {
                        "statement": {
                            "predicateType": predicate_type,
                            "subject": [{"digest": {"sha256": artifact_sha}}],
                        },
                        "signature": {"certificate": certificate},
                        "verifiedTimestamps": ["2026-07-13T00:00:00Z"],
                    }
                }
            )
        external = self.root / "matching-external.json"
        MODULE.write_canonical_json(external, entries)

        result = MODULE.verify_bundle(
            artifact_path=paths["artifact"],
            sbom_path=paths["sbom"],
            attestation_path=paths["attestation"],
            provenance_path=paths["provenance"],
            mode="production",
            external_verification_path=external,
        )
        self.assertEqual(result["status"], "unsupported")
        self.assertIn("LEGACY_PRODUCTION_VERIFICATION_UNSUPPORTED", result["reason_codes"])

        cli = subprocess.run(
            [
                sys.executable,
                str(SCRIPT),
                "verify",
                "--artifact",
                str(paths["artifact"]),
                "--sbom",
                str(paths["sbom"]),
                "--attestation",
                str(paths["attestation"]),
                "--provenance",
                str(paths["provenance"]),
                "--mode",
                "production",
                "--external-verification",
                str(external),
            ],
            text=True,
            capture_output=True,
            check=False,
        )
        self.assertNotEqual(cli.returncode, 0)
        self.assertNotIn('"status":"verified"', cli.stdout)

    def test_legacy_attestation_cli_rejects_production_identity(self) -> None:
        result = subprocess.run(
            [
                sys.executable,
                str(SCRIPT),
                "create-attestation",
                "--metadata",
                str(self.root / "metadata.json"),
                "--artifact",
                str(self.artifact),
                "--sbom",
                str(self.root / "sbom.json"),
                "--output",
                str(self.root / "attestation.json"),
                "--identity",
                "production",
            ],
            text=True,
            capture_output=True,
            check=False,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse((self.root / "attestation.json").exists())

    def test_arbitrary_rollback_cannot_restore_legacy_production_authority(self) -> None:
        paths = self.make_bundle()
        provenance = json.loads(paths["provenance"].read_text())
        provenance["rollback"] = {
            "previous_known_good": "unknown",
            "target": "unknown",
        }
        MODULE.write_canonical_json(paths["provenance"], provenance)
        result = MODULE.verify_bundle(
            artifact_path=paths["artifact"],
            sbom_path=paths["sbom"],
            attestation_path=paths["attestation"],
            provenance_path=paths["provenance"],
            mode="production",
        )
        self.assertEqual(result["status"], "unsupported")
        self.assertEqual(
            result["reason_codes"], ["LEGACY_PRODUCTION_VERIFICATION_UNSUPPORTED"]
        )

    def test_artifact_tampering_is_rejected_with_bounded_reason(self) -> None:
        paths = self.make_bundle()
        paths["artifact"].write_bytes(b"tampered\n")
        result = MODULE.verify_bundle(
            artifact_path=paths["artifact"],
            sbom_path=paths["sbom"],
            attestation_path=paths["attestation"],
            provenance_path=paths["provenance"],
            mode="fixture",
        )
        self.assertEqual(result["status"], "rejected")
        self.assertIn("ARTIFACT_DIGEST_MISMATCH", result["reason_codes"])

    def test_sbom_and_attestation_tampering_is_rejected(self) -> None:
        paths = self.make_bundle()
        sbom = json.loads(paths["sbom"].read_text())
        sbom["packages"][0]["versionInfo"] = "9.9.9"
        paths["sbom"].write_text(json.dumps(sbom))
        result = MODULE.verify_bundle(
            artifact_path=paths["artifact"],
            sbom_path=paths["sbom"],
            attestation_path=paths["attestation"],
            provenance_path=paths["provenance"],
            mode="fixture",
        )
        self.assertEqual(result["status"], "rejected")
        self.assertIn("SBOM_DIGEST_MISMATCH", result["reason_codes"])

    def test_media_type_and_predicate_bindings_are_fail_closed(self) -> None:
        paths = self.make_bundle()
        provenance = json.loads(paths["provenance"].read_text())
        provenance["package"]["media_type"] = "application/octet-stream"
        provenance["attestation"]["predicate_type"] = "https://example.invalid/predicate"
        MODULE.write_canonical_json(paths["provenance"], provenance)
        result = MODULE.verify_bundle(
            artifact_path=paths["artifact"],
            sbom_path=paths["sbom"],
            attestation_path=paths["attestation"],
            provenance_path=paths["provenance"],
            mode="fixture",
        )
        self.assertEqual(result["status"], "rejected")
        self.assertIn("ARTIFACT_MEDIA_TYPE_MISMATCH", result["reason_codes"])
        self.assertIn("ATTESTATION_PREDICATE_MISMATCH", result["reason_codes"])

    def test_verification_policy_cannot_enable_external_actions(self) -> None:
        paths = self.make_bundle()
        provenance = json.loads(paths["provenance"].read_text())
        provenance["publication"]["external_action_authorized"] = True
        MODULE.write_canonical_json(paths["provenance"], provenance)
        result = MODULE.verify_bundle(
            artifact_path=paths["artifact"],
            sbom_path=paths["sbom"],
            attestation_path=paths["attestation"],
            provenance_path=paths["provenance"],
            mode="fixture",
        )
        self.assertEqual(result["status"], "rejected")
        self.assertIn("VERIFICATION_POLICY_MISMATCH", result["reason_codes"])

    def test_missing_attestation_is_unsupported_not_a_pass(self) -> None:
        paths = self.make_bundle()
        paths["attestation"].unlink()
        result = MODULE.verify_bundle(
            artifact_path=paths["artifact"],
            sbom_path=paths["sbom"],
            attestation_path=paths["attestation"],
            provenance_path=paths["provenance"],
            mode="fixture",
        )
        self.assertEqual(result["status"], "unsupported")
        self.assertIn("ATTESTATION_EVIDENCE_MISSING", result["reason_codes"])

    def test_verification_result_is_hash_bound_to_inputs(self) -> None:
        paths = self.make_bundle()
        result = MODULE.verify_bundle(
            artifact_path=paths["artifact"],
            sbom_path=paths["sbom"],
            attestation_path=paths["attestation"],
            provenance_path=paths["provenance"],
            mode="fixture",
        )
        self.assertEqual(result["schema_version"], MODULE.VERIFICATION_SCHEMA_VERSION)
        self.assertEqual(result["inputs"]["artifact_sha256"], hashlib.sha256(paths["artifact"].read_bytes()).hexdigest())
        self.assertEqual(result["inputs"]["provenance_sha256"], MODULE.sha256_file(paths["provenance"]))


if __name__ == "__main__":
    unittest.main()
