from __future__ import annotations

import copy
import importlib.util
import io
import json
import shutil
import sys
import tarfile
import tempfile
import unittest
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "release_provenance.py"
SPEC = importlib.util.spec_from_file_location("release_provenance_v2", SCRIPT)
assert SPEC is not None
MODULE = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = MODULE
assert SPEC.loader is not None
SPEC.loader.exec_module(MODULE)


def metadata_for(artifact: Path, *, package_kind: str = "package") -> dict[str, Any]:
    target = "x86_64-unknown-linux-gnu" if package_kind == "package" else "linux/amd64"
    return {
        "repository": "Igzela/token-efficient-agent-harness-lab",
        "source_commit": "a" * 40,
        "ref": "refs/tags/v1.2.3",
        "workflow": ".github/workflows/release.yml",
        "workflow_ref": "Igzela/token-efficient-agent-harness-lab/.github/workflows/release.yml@refs/tags/v1.2.3",
        "run_id": "1234",
        "run_attempt": 1,
        "job": f"build-{target}",
        "builder_id": "github-hosted:Linux",
        "target_os": "linux",
        "target_architecture": "x86_64" if package_kind == "package" else "amd64",
        "target_triple": target,
        "package_kind": package_kind,
        "artifact_name": artifact.name,
        "artifact_media_type": (
            "application/vnd.acp.release+tar"
            if package_kind == "package"
            else "application/vnd.oci.image.manifest.v1+json"
        ),
        "publication_mode": "github-release",
        "lockfiles": [
            {"path": "Cargo.lock", "sha256": MODULE.sha256_file(ROOT / "Cargo.lock")},
            {
                "path": "dashboard/bun.lock",
                "sha256": MODULE.sha256_file(ROOT / "dashboard/bun.lock"),
            },
            {
                "path": "sdk/typescript/bun.lock",
                "sha256": MODULE.sha256_file(ROOT / "sdk/typescript/bun.lock"),
            },
        ],
        "build_inputs": [
            {"path": "engine/Cargo.toml", "sha256": MODULE.sha256_file(ROOT / "engine/Cargo.toml")}
        ],
        "rollback": {
            "state": "previous_release",
            "previous": {
                "tag": "v1.2.2",
                "source_commit": "b" * 40,
                "artifact": {
                    "filename": "agent-control-plane-v1.2.2-x86_64-unknown-linux-gnu.tar.gz",
                    "sha256": "c" * 64,
                },
                "target_triple": target,
                "package_kind": package_kind,
            },
        },
    }


def verification_entry(
    *, metadata: dict[str, Any], artifact_sha: str, predicate_type: str, predicate: Any
) -> dict[str, Any]:
    workflow_uri = (
        f"https://github.com/{metadata['repository']}/{metadata['workflow']}@{metadata['ref']}"
    )
    return {
        "verificationResult": {
            "statement": {
                "_type": "https://in-toto.io/Statement/v1",
                "subject": [{"name": metadata["artifact_name"], "digest": {"sha256": artifact_sha}}],
                "predicateType": predicate_type,
                "predicate": predicate,
            },
            "signature": {
                "certificate": {
                    "issuer": MODULE.PRODUCTION_ISSUER,
                    "sourceRepositoryURI": f"https://github.com/{metadata['repository']}",
                    "sourceRepositoryRef": metadata["ref"],
                    "sourceRepositoryDigest": metadata["source_commit"],
                    "buildConfigURI": workflow_uri,
                    "buildSignerURI": workflow_uri,
                    "subjectAlternativeName": workflow_uri,
                    "runnerEnvironment": "github-hosted",
                }
            },
            "verifiedTimestamps": [{"type": "tlog", "timestamp": "2026-07-13T00:00:00Z"}],
        }
    }


class ReleaseProvenanceV2Tests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        self.artifact = self.root / "agent-control-plane-v1.2.3-x86_64-unknown-linux-gnu.tar.gz"
        self.artifact.write_bytes(b"release-v2\n")
        self.metadata = metadata_for(self.artifact)

    def tearDown(self) -> None:
        self.temporary.cleanup()

    def build_files(self) -> dict[str, Path]:
        inventory = MODULE.load_dependency_inventory(
            ROOT, ["Cargo.lock", "dashboard/bun.lock", "sdk/typescript/bun.lock"]
        )
        artifact_sha = MODULE.sha256_file(self.artifact)
        sbom = MODULE.build_spdx_sbom(
            metadata=self.metadata,
            artifact_sha256=artifact_sha,
            artifact_size=self.artifact.stat().st_size,
            inventory=inventory,
        )
        sbom_path = self.root / f"{self.artifact.name}.spdx.json"
        MODULE.write_canonical_json(sbom_path, sbom)
        bootstrap = [
            {
                "filename": "install-from-release.sh",
                "sha256": "d" * 64,
                "source_commit": self.metadata["source_commit"],
                "predicate_type": MODULE.SLSA_PREDICATE_TYPE,
            },
            {
                "filename": "release_provenance.py",
                "sha256": "e" * 64,
                "source_commit": self.metadata["source_commit"],
                "predicate_type": MODULE.SLSA_PREDICATE_TYPE,
            },
        ]
        manifest = MODULE.build_release_manifest(
            metadata=self.metadata,
            artifact_sha256=artifact_sha,
            artifact_size=self.artifact.stat().st_size,
            sbom=sbom,
            sbom_path=sbom_path,
            bootstrap_assets=bootstrap,
        )
        manifest_path = self.root / f"{self.artifact.name}.release-manifest.json"
        MODULE.write_canonical_json(manifest_path, manifest)
        paths = {"artifact": self.artifact, "sbom": sbom_path, "manifest": manifest_path}
        predicates = {
            "slsa": {"buildDefinition": {"buildType": "fixture"}, "runDetails": {}},
            "spdx": sbom,
            "release_manifest": manifest,
        }
        for role, predicate_type in MODULE.ATTESTATION_ROLES.items():
            bundle = MODULE.build_attestation_fixture(
                metadata=self.metadata,
                artifact_sha256=artifact_sha,
                role=role,
                predicate_type=predicate_type,
                predicate=predicates[role],
                identity=MODULE.fixture_identity(self.metadata),
            )
            path = self.root / f"{self.artifact.name}.{role}.bundle.json"
            MODULE.write_canonical_json(path, bundle)
            paths[f"{role}_bundle"] = path
        return paths

    def production_paths(self) -> dict[str, Path]:
        paths = self.build_files()
        paths["slsa_bundle"].write_bytes(b"opaque-production-slsa-bundle\n")
        paths["spdx_bundle"].write_bytes(b"opaque-production-spdx-bundle\n")
        paths["release_manifest_bundle"].write_bytes(
            b"opaque-production-release-manifest-bundle\n"
        )
        return paths

    def production_runner(
        self,
        paths: dict[str, Path],
        *,
        certificate_changes: dict[str, str] | None = None,
        predicate_changes: dict[str, Any] | None = None,
        artifact_sha: str | None = None,
    ):
        sbom = MODULE.read_json(paths["sbom"])
        manifest = MODULE.read_json(paths["manifest"])

        def runner(command: list[str]) -> Any:
            bundle = Path(command[command.index("--bundle") + 1])
            predicate_type = command[command.index("--predicate-type") + 1]
            predicate = (
                sbom
                if bundle == paths["spdx_bundle"]
                else manifest
                if bundle == paths["release_manifest_bundle"]
                else {"buildDefinition": {"buildType": "fixture"}, "runDetails": {}}
            )
            predicate = copy.deepcopy(predicate)
            if predicate_changes and bundle.name in predicate_changes:
                predicate["adversarial"] = predicate_changes[bundle.name]
            entry = verification_entry(
                metadata=self.metadata,
                artifact_sha=artifact_sha or MODULE.sha256_file(self.artifact),
                predicate_type=predicate_type,
                predicate=predicate,
            )
            certificate = entry["verificationResult"]["signature"]["certificate"]
            certificate.update(certificate_changes or {})
            return [entry]

        return runner

    def test_real_inventory_contains_exact_cargo_and_npm_packages_and_relationships(self) -> None:
        inventory = MODULE.load_dependency_inventory(
            ROOT, ["Cargo.lock", "dashboard/bun.lock", "sdk/typescript/bun.lock"]
        )
        packages = inventory["packages"]
        identities = {(item["ecosystem"], item["name"], item["version"]) for item in packages}
        self.assertIn(("cargo", "serde", "1.0.228"), identities)
        self.assertIn(("npm", "next", "15.5.18"), identities)
        self.assertIn(("npm", "typescript", "5.9.3"), identities)
        self.assertTrue(all(item["source_lockfile"] for item in packages))
        self.assertTrue(all(item["purl"].startswith("pkg:") for item in packages))
        self.assertTrue(inventory["relationships"])

        files = self.build_files()
        sbom = MODULE.read_json(files["sbom"])
        subject = next(
            package for package in sbom["packages"] if package["name"] == self.artifact.name
        )
        self.assertEqual(
            subject["checksums"],
            [{"algorithm": "SHA256", "checksumValue": MODULE.sha256_file(self.artifact)}],
        )

    def test_inventory_and_sbom_are_deterministic_and_lock_sensitive(self) -> None:
        first = MODULE.load_dependency_inventory(ROOT, ["Cargo.lock", "dashboard/bun.lock"])
        second = MODULE.load_dependency_inventory(ROOT, ["dashboard/bun.lock", "Cargo.lock"])
        self.assertEqual(MODULE.canonical_json_bytes(first), MODULE.canonical_json_bytes(second))

        lock_root = self.root / "locks"
        lock_root.mkdir()
        shutil.copy(ROOT / "Cargo.lock", lock_root / "Cargo.lock")
        shutil.copy(ROOT / "dashboard/bun.lock", lock_root / "bun.lock")
        before = MODULE.load_dependency_inventory(lock_root, ["Cargo.lock", "bun.lock"])
        text = (lock_root / "bun.lock").read_text(encoding="utf-8")
        (lock_root / "bun.lock").write_text(text.replace("15.5.18", "15.5.19", 1), encoding="utf-8")
        after = MODULE.load_dependency_inventory(lock_root, ["Cargo.lock", "bun.lock"])
        self.assertNotEqual(MODULE.sha256_bytes(MODULE.canonical_json_bytes(before)), MODULE.sha256_bytes(MODULE.canonical_json_bytes(after)))

    def test_missing_malformed_and_conflicting_lockfiles_fail(self) -> None:
        with self.assertRaises(MODULE.ContractError):
            MODULE.load_dependency_inventory(self.root, ["missing.lock"])
        (self.root / "Cargo.lock").write_text("[[package]\n", encoding="utf-8")
        with self.assertRaises(MODULE.ContractError):
            MODULE.load_dependency_inventory(self.root, ["Cargo.lock"])
        (self.root / "bun.lock").write_text('{"packages":{"x":["x@1.0.0","",{},"a"],"x@dup":["x@1.0.0","",{},"b"]}}', encoding="utf-8")
        with self.assertRaises(MODULE.ContractError):
            MODULE.load_dependency_inventory(self.root, ["bun.lock"])

    def test_package_and_container_subjects_remain_distinct(self) -> None:
        inventory = MODULE.load_dependency_inventory(ROOT, ["sdk/typescript/bun.lock"])
        package = MODULE.build_spdx_sbom(
            metadata=self.metadata,
            artifact_sha256=MODULE.sha256_file(self.artifact),
            artifact_size=self.artifact.stat().st_size,
            inventory=inventory,
        )
        container_metadata = metadata_for(self.artifact, package_kind="container")
        container = MODULE.build_spdx_sbom(
            metadata=container_metadata,
            artifact_sha256=MODULE.sha256_file(self.artifact),
            artifact_size=self.artifact.stat().st_size,
            inventory=inventory,
        )
        self.assertNotEqual(package["documentNamespace"], container["documentNamespace"])
        self.assertIn("package_kind=package", package["documentComment"])
        self.assertIn("package_kind=container", container["documentComment"])

    def test_fixture_three_role_bundle_is_structural_and_never_production_verified(self) -> None:
        paths = self.build_files()
        fixture = MODULE.verify_release(
            artifact_path=paths["artifact"],
            sbom_path=paths["sbom"],
            manifest_path=paths["manifest"],
            slsa_bundle_path=paths["slsa_bundle"],
            spdx_bundle_path=paths["spdx_bundle"],
            manifest_bundle_path=paths["release_manifest_bundle"],
            mode="fixture",
        )
        self.assertEqual(fixture["status"], "verified_fixture")
        production = MODULE.verify_release(
            artifact_path=paths["artifact"],
            sbom_path=paths["sbom"],
            manifest_path=paths["manifest"],
            slsa_bundle_path=paths["slsa_bundle"],
            spdx_bundle_path=paths["spdx_bundle"],
            manifest_bundle_path=paths["release_manifest_bundle"],
            mode="production",
            gh_runner=lambda _command: (_ for _ in ()).throw(RuntimeError("fixture must not invoke production")),
        )
        self.assertEqual(production["status"], "rejected")

    def test_manifest_cannot_select_another_repository_or_workflow_authority(self) -> None:
        for field, value in (
            ("repository", "other/repository"),
            ("workflow", ".github/workflows/other.yml"),
        ):
            with self.subTest(field=field):
                original = self.metadata[field]
                self.metadata[field] = value
                self.metadata["workflow_ref"] = (
                    f"{self.metadata['repository']}/{self.metadata['workflow']}@{self.metadata['ref']}"
                )
                paths = self.build_files()
                result = MODULE.verify_release(
                    artifact_path=paths["artifact"], sbom_path=paths["sbom"],
                    manifest_path=paths["manifest"], slsa_bundle_path=paths["slsa_bundle"],
                    spdx_bundle_path=paths["spdx_bundle"],
                    manifest_bundle_path=paths["release_manifest_bundle"], mode="fixture",
                )
                self.assertEqual(result["status"], "rejected")
                self.metadata[field] = original
                self.metadata["workflow_ref"] = (
                    f"{self.metadata['repository']}/{self.metadata['workflow']}@{self.metadata['ref']}"
                )

    def test_signed_but_digestless_spdx_subject_is_rejected(self) -> None:
        paths = self.build_files()
        sbom = MODULE.read_json(paths["sbom"])
        subject = next(
            package for package in sbom["packages"] if package["name"] == self.artifact.name
        )
        subject.pop("checksums")
        MODULE.write_canonical_json(paths["sbom"], sbom)
        manifest = MODULE.read_json(paths["manifest"])
        manifest["sbom"]["sha256"] = MODULE.sha256_file(paths["sbom"])
        manifest["sbom"]["size"] = paths["sbom"].stat().st_size
        MODULE.write_canonical_json(paths["manifest"], manifest)
        for role, predicate in (("spdx", sbom), ("release_manifest", manifest)):
            bundle = MODULE.read_json(paths[f"{role}_bundle"])
            bundle["predicate"] = predicate
            MODULE.write_canonical_json(paths[f"{role}_bundle"], bundle)
        result = MODULE.verify_release(
            artifact_path=paths["artifact"],
            sbom_path=paths["sbom"],
            manifest_path=paths["manifest"],
            slsa_bundle_path=paths["slsa_bundle"],
            spdx_bundle_path=paths["spdx_bundle"],
            manifest_bundle_path=paths["release_manifest_bundle"],
            mode="fixture",
        )
        self.assertEqual(result["status"], "rejected")

    def test_production_uses_exact_distinct_bundles_and_source_digest(self) -> None:
        paths = self.production_paths()
        artifact_sha = MODULE.sha256_file(self.artifact)
        sbom = MODULE.read_json(paths["sbom"])
        manifest = MODULE.read_json(paths["manifest"])
        seen: list[list[str]] = []

        def runner(command: list[str]) -> Any:
            seen.append(command)
            bundle = Path(command[command.index("--bundle") + 1])
            predicate_type = command[command.index("--predicate-type") + 1]
            predicate = (
                sbom
                if bundle == paths["spdx_bundle"]
                else manifest
                if bundle == paths["release_manifest_bundle"]
                else {"buildDefinition": {"buildType": "fixture"}, "runDetails": {}}
            )
            return [
                verification_entry(
                    metadata=self.metadata,
                    artifact_sha=artifact_sha,
                    predicate_type=predicate_type,
                    predicate=predicate,
                )
            ]

        result = MODULE.verify_release(
            artifact_path=paths["artifact"],
            sbom_path=paths["sbom"],
            manifest_path=paths["manifest"],
            slsa_bundle_path=paths["slsa_bundle"],
            spdx_bundle_path=paths["spdx_bundle"],
            manifest_bundle_path=paths["release_manifest_bundle"],
            mode="production",
            gh_runner=runner,
        )
        self.assertEqual(result["status"], "verified")
        self.assertEqual(len(seen), 3)
        self.assertEqual({Path(command[command.index("--bundle") + 1]) for command in seen}, {paths["slsa_bundle"], paths["spdx_bundle"], paths["release_manifest_bundle"]})
        self.assertTrue(all(command[command.index("--source-digest") + 1] == self.metadata["source_commit"] for command in seen))
        self.assertTrue(all("--source-repo" not in command for command in seen))
        self.assertTrue(all("--signer-repo" not in command for command in seen))
        self.assertTrue(all(command[command.index("--signer-workflow") + 1] == f"{self.metadata['repository']}/{self.metadata['workflow']}" for command in seen))

    def test_missing_or_swapped_local_bundle_and_api_only_evidence_are_rejected(self) -> None:
        for role in ("slsa_bundle", "spdx_bundle", "release_manifest_bundle"):
            with self.subTest(role=role):
                paths = self.production_paths()
                paths[role].unlink()
                result = MODULE.verify_release(
                    artifact_path=paths["artifact"], sbom_path=paths["sbom"],
                    manifest_path=paths["manifest"], slsa_bundle_path=paths["slsa_bundle"],
                    spdx_bundle_path=paths["spdx_bundle"],
                    manifest_bundle_path=paths["release_manifest_bundle"], mode="production",
                    gh_runner=lambda _command: [],
                )
                self.assertEqual(result["status"], "rejected")

        paths = self.production_paths()
        result = MODULE.verify_release(
            artifact_path=paths["artifact"], sbom_path=paths["sbom"],
            manifest_path=paths["manifest"], slsa_bundle_path=paths["spdx_bundle"],
            spdx_bundle_path=paths["slsa_bundle"],
            manifest_bundle_path=paths["release_manifest_bundle"], mode="production",
            gh_runner=self.production_runner(paths),
        )
        self.assertEqual(result["status"], "rejected")

    def test_wrong_predicate_content_subject_and_identity_claims_are_rejected(self) -> None:
        paths = self.production_paths()
        cases = [
            {"certificate_changes": {"sourceRepositoryURI": "https://github.com/other/repository"}},
            {"certificate_changes": {"buildSignerURI": "https://github.com/other/repository/.github/workflows/release.yml@refs/tags/v1.2.3"}},
            {"certificate_changes": {"buildConfigURI": "https://github.com/other/repository/.github/workflows/release.yml@refs/tags/v1.2.3"}},
            {"certificate_changes": {"sourceRepositoryDigest": "f" * 40}},
            {"certificate_changes": {"sourceRepositoryRef": "refs/tags/v9.9.9"}},
            {"certificate_changes": {"issuer": "https://issuer.invalid"}},
            {"certificate_changes": {"runnerEnvironment": "self-hosted"}},
            {"artifact_sha": "f" * 64},
            {"predicate_changes": {paths["spdx_bundle"].name: "wrong-local-sbom"}},
            {"predicate_changes": {paths["release_manifest_bundle"].name: "wrong-local-manifest"}},
        ]
        for index, arguments in enumerate(cases):
            with self.subTest(index=index):
                result = MODULE.verify_release(
                    artifact_path=paths["artifact"], sbom_path=paths["sbom"],
                    manifest_path=paths["manifest"], slsa_bundle_path=paths["slsa_bundle"],
                    spdx_bundle_path=paths["spdx_bundle"],
                    manifest_bundle_path=paths["release_manifest_bundle"], mode="production",
                    gh_runner=self.production_runner(paths, **arguments),
                )
                self.assertEqual(result["status"], "rejected")

    def test_bootstrap_binding_requires_exact_commit_digest_and_complete_assets(self) -> None:
        paths = self.build_files()
        manifest = MODULE.read_json(paths["manifest"])
        installer = self.root / "install-from-release.sh"
        verifier = self.root / "release_provenance.py"
        installer.write_bytes(b"installer\n")
        verifier.write_bytes(b"verifier\n")
        for record, path in zip(manifest["bootstrap"], (installer, verifier)):
            record["sha256"] = MODULE.sha256_file(path)
        MODULE.write_canonical_json(paths["manifest"], manifest)

        class Args:
            source_commit = self.metadata["source_commit"]
            manifest = str(paths["manifest"])
            asset = [f"install-from-release.sh={installer}", f"release_provenance.py={verifier}"]

        self.assertEqual(MODULE.command_verify_bootstrap(Args()), 0)
        verifier.write_bytes(b"other release verifier\n")
        with self.assertRaises(MODULE.ContractError):
            MODULE.command_verify_bootstrap(Args())
        verifier.write_bytes(b"verifier\n")
        Args.source_commit = "f" * 40
        with self.assertRaises(MODULE.ContractError):
            MODULE.command_verify_bootstrap(Args())
        Args.source_commit = self.metadata["source_commit"]
        Args.asset = [f"install-from-release.sh={installer}"]
        with self.assertRaises(MODULE.ContractError):
            MODULE.command_verify_bootstrap(Args())

    def test_swapped_duplicated_and_wrong_predicate_content_are_rejected(self) -> None:
        paths = self.build_files()
        duplicated = self.root / "duplicate.bundle.json"
        shutil.copy(paths["slsa_bundle"], duplicated)
        result = MODULE.verify_release(
            artifact_path=paths["artifact"],
            sbom_path=paths["sbom"],
            manifest_path=paths["manifest"],
            slsa_bundle_path=paths["slsa_bundle"],
            spdx_bundle_path=duplicated,
            manifest_bundle_path=paths["release_manifest_bundle"],
            mode="fixture",
        )
        self.assertEqual(result["status"], "rejected")

        tampered = MODULE.read_json(paths["spdx_bundle"])
        tampered["predicate"]["name"] = "swapped-sbom"
        MODULE.write_canonical_json(paths["spdx_bundle"], tampered)
        result = MODULE.verify_release(
            artifact_path=paths["artifact"],
            sbom_path=paths["sbom"],
            manifest_path=paths["manifest"],
            slsa_bundle_path=paths["slsa_bundle"],
            spdx_bundle_path=paths["spdx_bundle"],
            manifest_bundle_path=paths["release_manifest_bundle"],
            mode="fixture",
        )
        self.assertEqual(result["status"], "rejected")

    def test_rollback_state_is_explicit_and_immutable(self) -> None:
        MODULE.validate_rollback_state({"state": "first_release", "previous": None}, self.metadata)
        MODULE.validate_rollback_state(self.metadata["rollback"], self.metadata)
        arbitrary = copy.deepcopy(self.metadata["rollback"])
        arbitrary["previous"]["source_commit"] = "not-a-commit"
        with self.assertRaises(MODULE.ContractError):
            MODULE.validate_rollback_state(arbitrary, self.metadata)
        incompatible = copy.deepcopy(self.metadata["rollback"])
        incompatible["previous"]["target_triple"] = "aarch64-unknown-linux-gnu"
        with self.assertRaises(MODULE.ContractError):
            MODULE.validate_rollback_state(incompatible, self.metadata)
        current = copy.deepcopy(self.metadata["rollback"])
        current["previous"]["tag"] = "v1.2.3"
        with self.assertRaises(MODULE.ContractError):
            MODULE.validate_rollback_state(current, self.metadata)
        current = copy.deepcopy(self.metadata["rollback"])
        current["previous"]["source_commit"] = self.metadata["source_commit"]
        with self.assertRaises(MODULE.ContractError):
            MODULE.validate_rollback_state(current, self.metadata)


class ReleaseArchiveV2Tests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        self.expected_root = "agent-control-plane-v1.2.3-x86_64-unknown-linux-gnu"

    def tearDown(self) -> None:
        self.temporary.cleanup()

    def archive(self, members: list[tuple[str, bytes, str]]) -> Path:
        path = self.root / "release.tar.gz"
        with tarfile.open(path, "w:gz") as bundle:
            for name, content, kind in members:
                info = tarfile.TarInfo(name)
                if kind == "file":
                    info.size = len(content)
                    info.mode = 0o755 if name.endswith(("engine", ".sh")) else 0o644
                    bundle.addfile(info, io.BytesIO(content))
                elif kind == "dir":
                    info.type = tarfile.DIRTYPE
                    bundle.addfile(info)
                elif kind == "link":
                    info.type = tarfile.SYMTYPE
                    info.linkname = "engine"
                    bundle.addfile(info)
                elif kind == "device":
                    info.type = tarfile.CHRTYPE
                    bundle.addfile(info)
                elif kind == "fifo":
                    info.type = tarfile.FIFOTYPE
                    bundle.addfile(info)
                elif kind == "sparse":
                    info.type = tarfile.GNUTYPE_SPARSE
                    info.size = len(content)
                    bundle.addfile(info, io.BytesIO(content))
        return path

    def valid_members(self) -> list[tuple[str, bytes, str]]:
        root = self.expected_root
        return [
            (root, b"", "dir"),
            (f"{root}/engine", b"binary", "file"),
            (f"{root}/release_provenance.py", b"verifier", "file"),
            (f"{root}/install.sh", b"install", "file"),
            (f"{root}/upgrade.sh", b"upgrade", "file"),
        ]

    def test_valid_archive_extracts_only_after_bounded_validation(self) -> None:
        archive = self.archive(self.valid_members())
        summary = MODULE.validate_release_archive(archive, self.expected_root)
        self.assertEqual(summary["required_files"], 4)
        destination = self.root / "extract"
        MODULE.extract_release_archive(archive, destination, self.expected_root)
        self.assertEqual((destination / self.expected_root / "engine").read_bytes(), b"binary")

    def test_duplicate_path_conflict_special_types_and_traversal_fail(self) -> None:
        cases = [
            self.valid_members() + [(f"{self.expected_root}/engine", b"other", "file")],
            self.valid_members() + [(f"{self.expected_root}/engine/child", b"x", "file")],
            self.valid_members() + [(f"{self.expected_root}/link", b"", "link")],
            self.valid_members() + [(f"{self.expected_root}/device", b"", "device")],
            self.valid_members() + [(f"{self.expected_root}/fifo", b"", "fifo")],
            self.valid_members() + [(f"{self.expected_root}/sparse", b"x", "sparse")],
            self.valid_members() + [(f"{self.expected_root}/../escape", b"x", "file")],
            self.valid_members() + [(f"{self.expected_root}\\escape", b"x", "file")],
            self.valid_members() + [("other-root/file", b"x", "file")],
        ]
        for index, members in enumerate(cases):
            with self.subTest(index=index):
                archive = self.archive(members)
                with self.assertRaises(MODULE.ContractError):
                    MODULE.validate_release_archive(archive, self.expected_root)

    def test_member_archive_total_count_and_path_bounds_fail(self) -> None:
        archive = self.archive(
            self.valid_members() + [(f"{self.expected_root}/large", b"12345", "file")]
        )
        with self.assertRaises(MODULE.ContractError):
            MODULE.validate_release_archive(
                archive,
                self.expected_root,
                limits=MODULE.ArchiveLimits(max_member_bytes=4),
            )
        with self.assertRaises(MODULE.ContractError):
            MODULE.validate_release_archive(
                archive,
                self.expected_root,
                limits=MODULE.ArchiveLimits(max_total_uncompressed_bytes=10),
            )
        with self.assertRaises(MODULE.ContractError):
            MODULE.validate_release_archive(
                archive,
                self.expected_root,
                limits=MODULE.ArchiveLimits(max_archive_bytes=1),
            )
        with self.assertRaises(MODULE.ContractError):
            MODULE.validate_release_archive(
                archive,
                self.expected_root,
                limits=MODULE.ArchiveLimits(max_members=4),
            )
        with self.assertRaises(MODULE.ContractError):
            MODULE.validate_release_archive(
                archive,
                self.expected_root,
                limits=MODULE.ArchiveLimits(max_path_bytes=20),
            )

    def test_highly_compressible_oversized_member_is_rejected_before_extraction(self) -> None:
        bomb = self.archive(
            self.valid_members()
            + [(f"{self.expected_root}/compressed-bomb", b"0" * (1024 * 1024), "file")]
        )
        self.assertLess(bomb.stat().st_size, 16 * 1024)
        destination = self.root / "bomb-extract"
        with self.assertRaises(MODULE.ContractError):
            MODULE.extract_release_archive(
                bomb,
                destination,
                self.expected_root,
                limits=MODULE.ArchiveLimits(max_member_bytes=64 * 1024),
            )
        self.assertFalse((destination / self.expected_root).exists())


if __name__ == "__main__":
    unittest.main()
