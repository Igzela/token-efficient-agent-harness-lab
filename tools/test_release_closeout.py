from __future__ import annotations

import importlib.util
import re
import tempfile
import unittest
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "release_provenance.py"
SPEC = importlib.util.spec_from_file_location("release_provenance_for_closeout", SCRIPT)
assert SPEC is not None
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(MODULE)


PACKAGE_TARGETS = (
    ("linux", "x86_64", "x86_64-unknown-linux-gnu"),
    ("linux", "aarch64", "aarch64-unknown-linux-gnu"),
    ("macos", "x86_64", "x86_64-apple-darwin"),
    ("macos", "aarch64", "aarch64-apple-darwin"),
)
CONTAINER_TARGETS = (
    ("linux", "amd64", "linux/amd64"),
    ("linux", "arm64", "linux/arm64"),
)


def metadata_for(
    *, kind: str, operating_system: str, architecture: str, target: str, artifact_name: str
) -> dict[str, Any]:
    return {
        "repository": "Igzela/token-efficient-agent-harness-lab",
        "source_commit": "a" * 40,
        "ref": "refs/tags/v0.1.0",
        "workflow": ".github/workflows/release.yml",
        "workflow_ref": "Igzela/token-efficient-agent-harness-lab/.github/workflows/release.yml@refs/tags/v0.1.0",
        "run_id": "closeout-fixture-run",
        "run_attempt": 1,
        "job": f"closeout-{kind}-{architecture}",
        "builder_id": "closeout-fixture-builder",
        "target_os": operating_system,
        "target_architecture": architecture,
        "target_triple": target,
        "package_kind": kind,
        "artifact_name": artifact_name,
        "artifact_media_type": (
            "application/vnd.acp.release+tar"
            if kind == "package"
            else "application/vnd.oci.image.manifest.v1+json"
        ),
        "previous_known_good": "v0.0.9",
        "rollback_target": "v0.0.9",
        "publication_mode": "dry-run",
        "lockfiles": [
            {"path": "Cargo.lock", "sha256": "b" * 64},
            {"path": "dashboard/bun.lock", "sha256": "c" * 64},
        ],
        "build_inputs": [
            {"path": "engine/Cargo.toml", "sha256": "d" * 64},
            {"path": ".env.example", "sha256": "e" * 64},
        ],
    }


def build_fixture(root: Path, metadata: dict[str, Any]) -> dict[str, Path]:
    root.mkdir(parents=True, exist_ok=True)
    artifact = root / metadata["artifact_name"]
    artifact.write_bytes(
        f"{metadata['package_kind']}:{metadata['target_triple']}:closeout-seed-v1\n".encode()
    )
    artifact_sha = MODULE.sha256_file(artifact)
    components = [
        {"name": "engine", "version": "locked", "source": "Cargo.lock"},
        {"name": "javascript-dependencies", "version": "locked", "source": "dashboard/bun.lock"},
    ]
    sbom = MODULE.build_spdx_sbom(
        metadata=metadata,
        artifact_sha256=artifact_sha,
        artifact_size=artifact.stat().st_size,
        components=components,
    )
    sbom_path = root / f"{artifact.name}.spdx.json"
    MODULE.write_canonical_json(sbom_path, sbom)
    attestation = MODULE.build_attestation_fixture(
        metadata=metadata,
        artifact_sha256=artifact_sha,
        sbom_sha256=MODULE.sha256_file(sbom_path),
        identity=MODULE.fixture_identity(metadata),
    )
    attestation_path = root / f"{artifact.name}.attestation.json"
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
    provenance_path = root / f"{artifact.name}.provenance.json"
    MODULE.write_canonical_json(provenance_path, provenance)
    verification_path = root / f"{artifact.name}.verification.json"
    result = MODULE.verify_bundle(
        artifact_path=artifact,
        sbom_path=sbom_path,
        attestation_path=attestation_path,
        provenance_path=provenance_path,
        mode="fixture",
    )
    MODULE.write_canonical_json(verification_path, result)
    return {
        "artifact": artifact,
        "sbom": sbom_path,
        "attestation": attestation_path,
        "provenance": provenance_path,
        "verification": verification_path,
    }


class ReleaseCloseoutTests(unittest.TestCase):
    def test_non_publishing_fixture_matrix_covers_all_supported_subjects(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            matrix = [("package", *target) for target in PACKAGE_TARGETS]
            matrix.extend(("container", *target) for target in CONTAINER_TARGETS)
            for index, (kind, operating_system, architecture, target) in enumerate(matrix):
                safe_target = target.replace("/", "-")
                artifact_name = (
                    f"agent-control-plane-v0.1.0-{safe_target}.tar.gz"
                    if kind == "package"
                    else f"agent-control-plane-image-v0.1.0-{safe_target}.json"
                )
                metadata = metadata_for(
                    kind=kind,
                    operating_system=operating_system,
                    architecture=architecture,
                    target=target,
                    artifact_name=artifact_name,
                )
                paths = build_fixture(root / str(index), metadata)
                result = MODULE.read_json(paths["verification"])
                self.assertEqual(result["status"], "verified_fixture")
                self.assertEqual(result["reason_codes"], ["FIXTURE_IDENTITY_NON_AUTHORITATIVE"])
                self.assertIn(metadata["target_triple"], MODULE.read_json(paths["sbom"])["documentComment"])

            self.assertEqual(len(list(root.iterdir())), len(matrix))

    def test_fixture_matrix_cannot_satisfy_production_trust_policy(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            metadata = metadata_for(
                kind="package",
                operating_system="linux",
                architecture="x86_64",
                target="x86_64-unknown-linux-gnu",
                artifact_name="agent-control-plane-v0.1.0-x86_64-unknown-linux-gnu.tar.gz",
            )
            paths = build_fixture(Path(directory), metadata)
            result = MODULE.verify_bundle(
                artifact_path=paths["artifact"],
                sbom_path=paths["sbom"],
                attestation_path=paths["attestation"],
                provenance_path=paths["provenance"],
                mode="production",
            )
            self.assertEqual(result["status"], "rejected")
            self.assertIn("UNTRUSTED_IDENTITY", result["reason_codes"])
            self.assertIn("EXTERNAL_VERIFICATION_UNAVAILABLE", result["reason_codes"])

    def test_release_workflow_and_installer_guards_are_present(self) -> None:
        workflow = (ROOT / ".github/workflows/release.yml").read_text()
        installer = (ROOT / "scripts/install-from-release.sh").read_text()
        action_refs = re.findall(r"uses:\s+[^@]+@([0-9a-f]{40})", workflow)
        self.assertGreaterEqual(len(action_refs), 7)
        self.assertNotRegex(workflow, r"uses:\s+[^@]+@[A-Za-z0-9._/-]+\s*$")
        self.assertIn("concurrency:", workflow)
        self.assertIn('test "${GITHUB_RUN_ATTEMPT}" = "1"', workflow)
        self.assertIn('test "$(git rev-parse HEAD)" = "${GITHUB_SHA}"', workflow)
        self.assertIn('test -z "$(git status --porcelain)"', workflow)
        self.assertIn("gh release view", workflow)
        self.assertIn("id-token: write", workflow)
        self.assertIn("attestations: write", workflow)
        self.assertIn(
            '"external_action_authorized":False',
            (ROOT / "scripts/release_provenance.py").read_text().replace(" ", ""),
        )
        self.assertIn("source-ref", installer)
        self.assertIn("--deny-self-hosted-runners", installer)
        self.assertIn("--predicate-type", installer)
        self.assertIn("unsafe release archive member", installer)
        self.assertIn("signed tag", installer)
        self.assertNotRegex(workflow, r"BEGIN (RSA|OPENSSH|EC|PRIVATE) KEY")

    def test_closeout_evidence_is_hash_bound_and_bounded(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            metadata = metadata_for(
                kind="container",
                operating_system="linux",
                architecture="amd64",
                target="linux/amd64",
                artifact_name="agent-control-plane-image-v0.1.0-linux-amd64.json",
            )
            paths = build_fixture(Path(directory), metadata)
            result = MODULE.read_json(paths["verification"])
            self.assertEqual(
                result["inputs"]["artifact_sha256"], MODULE.sha256_file(paths["artifact"])
            )
            self.assertEqual(
                result["inputs"]["provenance_sha256"], MODULE.sha256_file(paths["provenance"])
            )
            self.assertTrue(set(result["reason_codes"]).issubset(MODULE.REASON_CODES))


if __name__ == "__main__":
    unittest.main()
