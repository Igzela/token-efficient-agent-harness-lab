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
        "publication_mode": "dry-run",
        "lockfiles": [{"path": "Cargo.lock", "sha256": "b" * 64}],
        "build_inputs": [{"path": "engine/Cargo.toml", "sha256": "c" * 64}],
        "rollback": {"state": "first_release", "previous": None},
    }


def build_fixture_evidence(root: Path) -> dict[str, Path]:
    root.mkdir(parents=True, exist_ok=True)
    artifact = root / "agent-control-plane-v0.1.0-x86_64-unknown-linux-gnu.tar.gz"
    artifact.write_bytes(b"fixture archive bytes\n")
    metadata = metadata_for(artifact)
    artifact_sha = MODULE.sha256_file(artifact)
    sbom = MODULE.build_spdx_sbom(
        metadata=metadata,
        artifact_sha256=artifact_sha,
        artifact_size=artifact.stat().st_size,
        components=[{"name": "engine", "version": "0.1.0", "source": "Cargo.lock"}],
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
                "filename": "install-from-release.sh",
                "sha256": "d" * 64,
                "source_commit": metadata["source_commit"],
                "predicate_type": MODULE.SLSA_PREDICATE_TYPE,
            },
            {
                "filename": "release_provenance.py",
                "sha256": "e" * 64,
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
            metadata=metadata,
            artifact_sha256=artifact_sha,
            identity=MODULE.fixture_identity(metadata),
            role=role,
            predicate_type=predicate_type,
            predicate=predicates[role],
        )
        path = root / f"{artifact.name}.{role}.bundle.json"
        MODULE.write_canonical_json(path, bundle)
        result[f"{role}_bundle"] = path
    return result


class ReleaseInstallationTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)

    def tearDown(self) -> None:
        self.temp.cleanup()

    def make_release_dir(self) -> Path:
        release = self.root / "release"
        release.mkdir(exist_ok=True)
        for name in ("upgrade.sh", "install.sh", "release_provenance.py"):
            shutil.copy(ROOT / "scripts" / name, release / name)
        (release / "dashboard").mkdir(exist_ok=True)
        (release / "dashboard" / "index.html").write_text("new dashboard\n")
        (release / "engine").write_text("#!/usr/bin/env bash\necho new --help\n")
        (release / "engine").chmod(0o755)
        return release

    def run_script(
        self, script: Path, *args: str, extra_env: dict[str, str] | None = None
    ) -> subprocess.CompletedProcess[str]:
        env = os.environ.copy()
        env["HOME"] = str(self.root / "home")
        env.update(extra_env or {})
        return subprocess.run(
            ["bash", str(script), *args],
            cwd=script.parent,
            env=env,
            text=True,
            capture_output=True,
            check=False,
        )

    def existing_install(self) -> tuple[Path, Path, Path]:
        prefix = self.root / "prefix"
        data = self.root / "data"
        (prefix / "bin").mkdir(parents=True, exist_ok=True)
        (data / "dashboard").mkdir(parents=True, exist_ok=True)
        binary = prefix / "bin" / "agent-control-plane"
        binary.write_text("#!/usr/bin/env bash\necho old --help\n")
        binary.chmod(0o755)
        (data / "dashboard" / "index.html").write_text("old dashboard\n")
        (data / "operator.db").write_text("operator data\n")
        return prefix, data, binary

    def development_upgrade(
        self,
        release: Path,
        prefix: Path,
        data: Path,
        *,
        upgrade_fault: str = "",
        rollback_fault: str = "",
        hooks: tuple[str, str] | None = None,
    ) -> subprocess.CompletedProcess[str]:
        args = ["--prefix", str(prefix), "--data-dir", str(data), "--development"]
        if hooks:
            args += ["--stop-command", hooks[0], "--restart-command", hooks[1]]
        return self.run_script(
            release / "upgrade.sh",
            *args,
            extra_env={
                "ACP_UPGRADE_FAULT": upgrade_fault,
                "ACP_ROLLBACK_FAULT": rollback_fault,
            },
        )

    def test_install_fails_closed_without_evidence(self) -> None:
        release = self.make_release_dir()
        prefix = self.root / "prefix"
        result = self.run_script(release / "install.sh", "--prefix", str(prefix))
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse((prefix / "bin" / "agent-control-plane").exists())

    def test_fixture_identity_cannot_upgrade_an_existing_installation(self) -> None:
        release = self.make_release_dir()
        evidence = build_fixture_evidence(self.root / "fixture")
        prefix, data, binary = self.existing_install()
        before = MODULE.sha256_file(binary)
        result = self.run_script(
            release / "upgrade.sh",
            "--prefix", str(prefix), "--data-dir", str(data),
            "--artifact", str(evidence["artifact"]),
            "--sbom", str(evidence["sbom"]),
            "--manifest", str(evidence["manifest"]),
            "--slsa-bundle", str(evidence["slsa_bundle"]),
            "--spdx-bundle", str(evidence["spdx_bundle"]),
            "--manifest-bundle", str(evidence["release_manifest_bundle"]),
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(before, MODULE.sha256_file(binary))
        self.assertFalse(Path(f"{binary}.bak").exists())

    def test_development_upgrade_succeeds_and_retains_backup(self) -> None:
        release = self.make_release_dir()
        prefix, data, binary = self.existing_install()
        result = self.development_upgrade(release, prefix, data)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("new", binary.read_text())
        self.assertIn("old", Path(f"{binary}.bak").read_text())
        self.assertEqual((data / "dashboard" / "index.html").read_text(), "new dashboard\n")
        self.assertEqual((data / "operator.db").read_text(), "operator data\n")
        self.assertTrue((data / "upgrade-rollback.state").is_file())

    def test_successful_fresh_install_preserves_preexisting_dashboard_and_operator_data(self) -> None:
        release = self.make_release_dir()
        prefix = self.root / "fresh-prefix"
        data = self.root / "fresh-data"
        (prefix / "bin").mkdir(parents=True)
        (data / "dashboard").mkdir(parents=True)
        (data / "dashboard" / "operator.html").write_text("operator dashboard\n")
        (data / "operator.db").write_text("operator data\n")

        result = self.run_script(
            release / "install.sh",
            "--prefix", str(prefix), "--data-dir", str(data), "--development",
        )

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual((data / "dashboard" / "index.html").read_text(), "new dashboard\n")
        self.assertEqual((data / "operator.db").read_text(), "operator data\n")
        backups = list(data.glob(".dashboard.preinstall.*"))
        self.assertEqual(len(backups), 1)
        self.assertEqual(
            (backups[0] / "operator.html").read_text(), "operator dashboard\n"
        )

    def test_fresh_install_partial_failures_leave_no_activation_or_data_loss(self) -> None:
        for fault in (
            "binary_move",
            "after_binary",
            "interrupt_after_binary",
            "dashboard_move",
            "after_dashboard",
            "interrupt_after_dashboard",
        ):
            with self.subTest(fault=fault):
                release = self.make_release_dir()
                prefix = self.root / f"prefix-{fault}"
                data = self.root / f"data-{fault}"
                (prefix / "bin").mkdir(parents=True)
                (data / "dashboard").mkdir(parents=True)
                (data / "dashboard" / "operator.html").write_text("old\n")
                (data / "operator.db").write_text("keep\n")
                result = self.run_script(
                    release / "install.sh",
                    "--prefix", str(prefix), "--data-dir", str(data), "--development",
                    extra_env={"ACP_INSTALL_FAULT": fault},
                )
                self.assertNotEqual(result.returncode, 0)
                self.assertFalse((prefix / "bin" / "agent-control-plane").exists())
                self.assertEqual((data / "dashboard" / "operator.html").read_text(), "old\n")
                self.assertEqual((data / "operator.db").read_text(), "keep\n")

    def test_upgrade_binary_dashboard_and_interruption_faults_restore_exact_state(self) -> None:
        for fault in (
            "binary_move",
            "after_binary",
            "interrupt_after_binary",
            "dashboard_move",
            "after_dashboard",
            "interrupt_after_dashboard",
        ):
            with self.subTest(fault=fault):
                release = self.make_release_dir()
                prefix, data, binary = self.existing_install()
                old_digest = MODULE.sha256_file(binary)
                result = self.development_upgrade(release, prefix, data, upgrade_fault=fault)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("UPGRADE_FAILED_ROLLBACK_SUCCEEDED", result.stderr)
                self.assertEqual(MODULE.sha256_file(binary), old_digest)
                self.assertEqual((data / "dashboard" / "index.html").read_text(), "old dashboard\n")
                self.assertEqual((data / "operator.db").read_text(), "operator data\n")
                shutil.rmtree(prefix)
                shutil.rmtree(data)

    def test_binary_and_dashboard_restoration_failures_are_reported_and_preserve_backups(self) -> None:
        for rollback_fault in ("binary_restore", "dashboard_restore"):
            with self.subTest(rollback_fault=rollback_fault):
                release = self.make_release_dir()
                prefix, data, binary = self.existing_install()
                result = self.development_upgrade(
                    release, prefix, data,
                    upgrade_fault="after_dashboard",
                    rollback_fault=rollback_fault,
                )
                self.assertEqual(result.returncode, 70)
                self.assertIn("UPGRADE_FAILED_ROLLBACK_FAILED", result.stderr)
                self.assertTrue(Path(f"{binary}.bak").is_file())
                self.assertTrue((data / "dashboard.bak").is_dir())
                shutil.rmtree(prefix)
                shutil.rmtree(data)

    def test_restart_and_health_failures_have_distinct_verified_outcomes(self) -> None:
        release = self.make_release_dir()
        prefix, data, binary = self.existing_install()
        restart = self.development_upgrade(
            release, prefix, data, upgrade_fault="restart", hooks=("true", "true")
        )
        self.assertIn("UPGRADE_FAILED_ROLLBACK_SUCCEEDED", restart.stderr)
        self.assertIn("old", binary.read_text())

        shutil.rmtree(prefix)
        shutil.rmtree(data)
        prefix, data, binary = self.existing_install()
        failed_restart = self.development_upgrade(
            release,
            prefix,
            data,
            upgrade_fault="after_binary",
            rollback_fault="restart",
            hooks=("true", "true"),
        )
        self.assertEqual(failed_restart.returncode, 70)
        self.assertIn("UPGRADE_FAILED_ROLLBACK_FAILED", failed_restart.stderr)

        shutil.rmtree(prefix)
        shutil.rmtree(data)
        prefix, data, binary = self.existing_install()
        health = self.development_upgrade(release, prefix, data, upgrade_fault="health")
        self.assertIn("UPGRADE_FAILED_ROLLBACK_SUCCEEDED", health.stderr)
        self.assertIn("old", binary.read_text())

    def test_repeated_recovery_is_idempotent(self) -> None:
        release = self.make_release_dir()
        prefix, data, binary = self.existing_install()
        initial = self.development_upgrade(release, prefix, data, upgrade_fault="after_dashboard")
        self.assertIn("UPGRADE_FAILED_ROLLBACK_SUCCEEDED", initial.stderr)
        digest = MODULE.sha256_file(binary)
        for _ in range(2):
            result = self.run_script(
                release / "upgrade.sh",
                "--prefix", str(prefix), "--data-dir", str(data), "--recover",
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("UPGRADE_FAILED_ROLLBACK_SUCCEEDED", result.stderr)
            self.assertEqual(MODULE.sha256_file(binary), digest)
            self.assertEqual((data / "dashboard" / "index.html").read_text(), "old dashboard\n")

    def test_recovery_uses_preserved_state_when_active_binary_is_missing(self) -> None:
        release = self.make_release_dir()
        prefix, data, binary = self.existing_install()
        old_digest = MODULE.sha256_file(binary)
        initial = self.development_upgrade(
            release,
            prefix,
            data,
            upgrade_fault="after_dashboard",
            rollback_fault="binary_restore",
        )
        self.assertEqual(initial.returncode, 70)
        binary.unlink(missing_ok=True)

        recovered = self.run_script(
            release / "upgrade.sh",
            "--prefix", str(prefix), "--data-dir", str(data), "--recover",
        )

        self.assertIn("UPGRADE_FAILED_ROLLBACK_SUCCEEDED", recovered.stderr)
        self.assertEqual(MODULE.sha256_file(binary), old_digest)
        self.assertEqual((data / "dashboard" / "index.html").read_text(), "old dashboard\n")

    def test_recovery_restarts_a_previously_managed_process(self) -> None:
        release = self.make_release_dir()
        prefix, data, binary = self.existing_install()
        marker = self.root / "restart.marker"
        initial = self.development_upgrade(
            release,
            prefix,
            data,
            upgrade_fault="after_binary",
            rollback_fault="restart",
            hooks=("true", "true"),
        )
        self.assertEqual(initial.returncode, 70)

        recovered = self.run_script(
            release / "upgrade.sh",
            "--prefix", str(prefix), "--data-dir", str(data), "--recover",
            "--stop-command", "true",
            "--restart-command", f"touch {marker}",
        )

        self.assertIn("UPGRADE_FAILED_ROLLBACK_SUCCEEDED", recovered.stderr)
        self.assertTrue(marker.is_file())
        self.assertIn("old", binary.read_text())

    def test_absent_dashboard_state_rejects_stale_backup_authority(self) -> None:
        release = self.make_release_dir()
        prefix, data, binary = self.existing_install()
        shutil.rmtree(data / "dashboard")
        (data / "dashboard.bak").mkdir()
        (data / "dashboard.bak" / "stale.html").write_text("stale\n")

        result = self.development_upgrade(
            release, prefix, data, upgrade_fault="after_binary"
        )

        self.assertIn("UPGRADE_FAILED_ROLLBACK_SUCCEEDED", result.stderr)
        self.assertFalse((data / "dashboard").exists())
        self.assertFalse((data / "dashboard.bak").exists())
        state = (data / "upgrade-rollback.state").read_text()
        self.assertIn("dashboard=absent", state)
        self.assertIn("old", binary.read_text())

    def test_tampered_rollback_state_fails_closed(self) -> None:
        release = self.make_release_dir()
        prefix, data, _ = self.existing_install()
        initial = self.development_upgrade(release, prefix, data)
        self.assertEqual(initial.returncode, 0, initial.stderr)
        state = data / "upgrade-rollback.state"
        state.write_text(state.read_text().replace("old_digest=", "old_digest=bad"))

        recovered = self.run_script(
            release / "upgrade.sh",
            "--prefix", str(prefix), "--data-dir", str(data), "--recover",
        )

        self.assertNotEqual(recovered.returncode, 0)
        self.assertNotIn("previous installation restored", recovered.stderr)


if __name__ == "__main__":
    unittest.main()
