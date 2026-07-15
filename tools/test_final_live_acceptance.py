#!/usr/bin/env python3
"""Tests for the bounded final live-acceptance seal."""

from __future__ import annotations

import hashlib
import importlib.util
import json
import pathlib
import tempfile
import unittest


ROOT = pathlib.Path(__file__).resolve().parents[1]
MODULE_PATH = ROOT / "scripts" / "final_live_acceptance.py"
SPEC = importlib.util.spec_from_file_location("final_live_acceptance", MODULE_PATH)
assert SPEC and SPEC.loader
acceptance = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(acceptance)
HEAD = "a" * 40
NOW = "2026-07-15T10:00:00Z"


class FinalLiveAcceptanceTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.root = pathlib.Path(self.directory.name)

    def tearDown(self):
        self.directory.cleanup()

    def evidence(self, component: str, status: str = "PASS", reasons=None) -> pathlib.Path:
        value = {
            "schema_version": "final-live-acceptance-evidence.v1",
            "component": component,
            "source_head": HEAD,
            "status": status,
            "command_id": f"acceptance.{component}",
            "environment_class": "fixture",
            "artifact_hashes": [hashlib.sha256(component.encode()).hexdigest()],
            "reason_codes": reasons or [],
            "completed_at": NOW,
        }
        path = self.root / f"{component}.json"
        path.write_text(json.dumps(value), encoding="utf-8")
        return path

    def test_all_pass_produces_hash_bound_not_published_seal(self):
        evidence = {component: self.evidence(component) for component in acceptance.COMPONENTS}
        report = acceptance.build_report(
            source_head=HEAD,
            evidence=evidence,
            generated_at=NOW,
        )
        self.assertEqual(report["status"], "PASS")
        self.assertEqual(report["release_state"], "RELEASE_READY_NOT_PUBLISHED")
        digest = report.pop("report_sha256")
        self.assertEqual(digest, hashlib.sha256(acceptance._canonical(report)).hexdigest())

    def test_skip_unsupported_and_missing_are_blocked(self):
        for status in ("SKIP", "UNSUPPORTED"):
            with self.subTest(status=status):
                report = acceptance.build_report(
                    source_head=HEAD,
                    evidence={"orchestrator": self.evidence(
                        "orchestrator", status, [status.lower()]
                    )},
                    required_components=("orchestrator", "target_output"),
                    generated_at=NOW,
                )
                self.assertEqual(report["status"], "BLOCKED")
                self.assertEqual(report["release_state"], "BLOCKED")
                self.assertEqual(
                    report["components"]["target_output"]["reason_codes"],
                    ["missing_evidence"],
                )

    def test_source_mismatch_and_raw_fields_are_rejected(self):
        path = self.evidence("orchestrator")
        value = json.loads(path.read_text())
        value["source_head"] = "b" * 40
        path.write_text(json.dumps(value))
        with self.assertRaisesRegex(acceptance.AcceptanceError, "source_head_mismatch"):
            acceptance.build_report(
                source_head=HEAD,
                evidence={"orchestrator": path},
                required_components=("orchestrator",),
                generated_at=NOW,
            )
        value["source_head"] = HEAD
        value["raw_log"] = "must-not-be-copied"
        path.write_text(json.dumps(value))
        with self.assertRaisesRegex(acceptance.AcceptanceError, "schema_invalid"):
            acceptance.build_report(
                source_head=HEAD,
                evidence={"orchestrator": path},
                required_components=("orchestrator",),
                generated_at=NOW,
            )

    def test_publication_requires_explicit_authorization(self):
        with self.assertRaisesRegex(acceptance.AcceptanceError, "publication_not_authorized"):
            acceptance.build_report(
                source_head=HEAD,
                evidence={"orchestrator": self.evidence("orchestrator")},
                required_components=("orchestrator",),
                generated_at=NOW,
                requested_release_state="PUBLISHED",
            )

    def test_stale_or_future_evidence_is_blocked(self):
        for completed_at in ("2026-07-13T10:00:00Z", "2026-07-16T10:00:00Z"):
            with self.subTest(completed_at=completed_at):
                path = self.evidence("orchestrator")
                value = json.loads(path.read_text())
                value["completed_at"] = completed_at
                path.write_text(json.dumps(value))
                report = acceptance.build_report(
                    source_head=HEAD,
                    evidence={"orchestrator": path},
                    required_components=("orchestrator",),
                    generated_at=NOW,
                )
                self.assertEqual(report["status"], "BLOCKED")
                self.assertIn(
                    "stale_evidence",
                    report["components"]["orchestrator"]["reason_codes"],
                )

    def test_cli_writes_only_bounded_summary_inside_output_root(self):
        evidence = self.evidence("orchestrator")
        result = acceptance.main([
            "--source-head", HEAD,
            "--evidence", f"orchestrator={evidence}",
            "--required-component", "orchestrator",
            "--generated-at", NOW,
            "--output-root", str(self.root),
        ])
        self.assertEqual(result, 0)
        report = json.loads((self.root / "final-live-acceptance.json").read_text())
        serialized = json.dumps(report)
        self.assertNotIn(str(self.root), serialized)
        self.assertNotIn("raw", serialized)


if __name__ == "__main__":
    unittest.main(verbosity=2)
