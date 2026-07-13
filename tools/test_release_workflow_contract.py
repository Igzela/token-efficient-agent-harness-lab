from __future__ import annotations

import unittest
from pathlib import Path

from tools.release_workflow_contract import WorkflowContractError, validate_release_contract_text


ROOT = Path(__file__).resolve().parents[1]
WORKFLOW = (ROOT / ".github/workflows/release.yml").read_text(encoding="utf-8")
INSTALLER = (ROOT / "scripts/install-from-release.sh").read_text(encoding="utf-8")
INSTALLATION_DOCUMENTS = tuple(
    (ROOT / path).read_text(encoding="utf-8")
    for path in ("README.md", "docs/RUNBOOK.md")
)


class ReleaseWorkflowContractTests(unittest.TestCase):
    def test_repository_workflow_passes_semantic_contract(self) -> None:
        validate_release_contract_text(WORKFLOW, INSTALLER)

    def assert_rejected(
        self,
        workflow: str = WORKFLOW,
        installer: str = INSTALLER,
        installation_documents: tuple[str, ...] = INSTALLATION_DOCUMENTS,
    ) -> None:
        with self.assertRaises(WorkflowContractError):
            validate_release_contract_text(workflow, installer, installation_documents)

    def test_missing_distinct_slsa_call_is_rejected(self) -> None:
        self.assert_rejected(WORKFLOW.replace("id: attest-slsa", "id: attest-spdx", 1))

    def test_missing_artifact_metadata_permission_is_rejected(self) -> None:
        self.assert_rejected(WORKFLOW.replace("      artifact-metadata: write\n", "", 1))

    def test_predicate_mismatch_and_bundle_alias_are_rejected(self) -> None:
        self.assert_rejected(WORKFLOW.replace(
            "attestations/release-manifest/v2", "attestations/release-manifest/v1", 1
        ))
        self.assert_rejected(WORKFLOW.replace(
            "steps.package.outputs.spdx_bundle", "steps.package.outputs.slsa_bundle", 1
        ))

    def test_publication_before_verification_is_rejected(self) -> None:
        start = WORKFLOW.index("      - name: Create GitHub Release after all verification")
        block = WORKFLOW[start:]
        self.assert_rejected(WORKFLOW[:start].replace(
            "      - name: Verify bootstrap, current release, and rollback target",
            block + "\n      - name: Verify bootstrap, current release, and rollback target",
            1,
        ))

    def test_first_release_requires_an_empty_release_history(self) -> None:
        self.assert_rejected(WORKFLOW.replace("gh release list", "printf 0", 1))

    def test_mutable_or_unbound_bootstrap_is_rejected(self) -> None:
        self.assert_rejected(installer=INSTALLER + "\n# /main/install.sh\n")
        self.assert_rejected(installer=INSTALLER.replace("--source-commit", "--commit"))
        self.assert_rejected(installer=INSTALLER.replace(
            'verify_slsa_asset "${SCRIPT_PATH}" "${BOOTSTRAP_BUNDLE}"', "true", 1
        ))
        self.assert_rejected(
            installation_documents=INSTALLATION_DOCUMENTS
            + ("curl https://raw.githubusercontent.com/example/project/main/install.sh | bash",)
        )

    def test_unsupported_or_incomplete_gh_policy_is_rejected(self) -> None:
        self.assert_rejected(installer=INSTALLER.replace(
            '--repo "${REPO}"', '--repo "${REPO}" --source-repo "${REPO}"', 1
        ))
        self.assert_rejected(installer=INSTALLER.replace(
            '--signer-workflow "${REPO}/.github/workflows/release.yml"',
            '--signer-repo "${REPO}"',
            1,
        ))
        self.assert_rejected(installer=INSTALLER.replace(
            '--cert-oidc-issuer "https://token.actions.githubusercontent.com"',
            '--hostname github.com',
            1,
        ))


if __name__ == "__main__":
    unittest.main()
