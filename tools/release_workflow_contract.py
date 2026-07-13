#!/usr/bin/env python3
"""Semantic contract checker for the repository-owned release workflow subset."""

from __future__ import annotations

from dataclasses import dataclass, field
from pathlib import Path
import re
import sys


ATTEST_SHA = "a1948c3f048ba23858d222213b7c278aabede763"
ATTEST_ACTION = f"actions/attest@{ATTEST_SHA}"
SLSA = "https://slsa.dev/provenance/v1"
SPDX = "https://spdx.dev/Document/v2.3"
MANIFEST = (
    "https://github.com/Igzela/token-efficient-agent-harness-lab/"
    "attestations/release-manifest/v2"
)
FULL_PIN = re.compile(r"^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+@[0-9a-f]{40}$")


class WorkflowContractError(ValueError):
    pass


@dataclass
class Step:
    name: str = ""
    step_id: str = ""
    uses: str = ""
    inputs: dict[str, str] = field(default_factory=dict)
    run: str = ""


@dataclass
class Job:
    name: str
    needs: str = ""
    permissions: dict[str, str] = field(default_factory=dict)
    steps: list[Step] = field(default_factory=list)


def _value(line: str) -> tuple[str, str]:
    key, separator, value = line.strip().partition(":")
    if not separator:
        raise WorkflowContractError(f"expected mapping entry: {line}")
    return key, value.strip().strip('"').strip("'")


def parse_workflow(text: str) -> dict[str, Job]:
    """Parse the job/permission/step portion of the workflow's YAML subset."""

    lines = text.splitlines()
    jobs: dict[str, Job] = {}
    in_jobs = False
    current: Job | None = None
    index = 0
    while index < len(lines):
        raw = lines[index]
        stripped = raw.strip()
        indent = len(raw) - len(raw.lstrip(" "))
        if raw == "jobs:":
            in_jobs = True
            index += 1
            continue
        if not in_jobs:
            index += 1
            continue
        if stripped and indent == 0:
            break
        if indent == 2 and stripped.endswith(":"):
            name = stripped[:-1]
            if name in jobs:
                raise WorkflowContractError(f"duplicate job: {name}")
            current = Job(name=name)
            jobs[name] = current
            index += 1
            continue
        if current is None:
            index += 1
            continue
        if indent == 4 and stripped.startswith("needs:"):
            current.needs = _value(raw)[1]
            index += 1
            continue
        if indent == 4 and stripped == "permissions:":
            index += 1
            while index < len(lines):
                nested = lines[index]
                nested_indent = len(nested) - len(nested.lstrip(" "))
                if nested.strip() and nested_indent <= 4:
                    break
                if nested.strip() and nested_indent == 6:
                    key, value = _value(nested)
                    current.permissions[key] = value
                index += 1
            continue
        if indent == 4 and stripped == "steps:":
            index += 1
            while index < len(lines):
                if not lines[index].strip():
                    index += 1
                    continue
                step_indent = len(lines[index]) - len(lines[index].lstrip(" "))
                if step_indent <= 4:
                    break
                if step_indent != 6 or not lines[index].strip().startswith("- "):
                    index += 1
                    continue
                block_start = index
                index += 1
                while index < len(lines):
                    candidate = lines[index]
                    candidate_indent = len(candidate) - len(candidate.lstrip(" "))
                    if candidate.strip() and candidate_indent == 6 and candidate.strip().startswith("- "):
                        break
                    if candidate.strip() and candidate_indent <= 4:
                        break
                    index += 1
                block = lines[block_start:index]
                step = Step()
                first = block[0].strip()[2:]
                if first.startswith("name:"):
                    step.name = _value(first)[1]
                block_index = 1
                while block_index < len(block):
                    entry = block[block_index]
                    entry_indent = len(entry) - len(entry.lstrip(" "))
                    entry_text = entry.strip()
                    if entry_indent == 8 and entry_text.startswith("name:"):
                        step.name = _value(entry)[1]
                    elif entry_indent == 8 and entry_text.startswith("id:"):
                        step.step_id = _value(entry)[1]
                    elif entry_indent == 8 and entry_text.startswith("uses:"):
                        step.uses = _value(entry)[1].split(" #", 1)[0]
                    elif entry_indent == 8 and entry_text == "with:":
                        block_index += 1
                        while block_index < len(block):
                            child = block[block_index]
                            child_indent = len(child) - len(child.lstrip(" "))
                            if child.strip() and child_indent <= 8:
                                block_index -= 1
                                break
                            if child.strip() and child_indent == 10:
                                key, value = _value(child)
                                step.inputs[key] = value
                            block_index += 1
                    elif entry_indent == 8 and entry_text.startswith("run:"):
                        marker = _value(entry)[1]
                        if marker in {"|", ">", "|-", ">-"}:
                            step.run = "\n".join(
                                child[10:] if child.startswith(" " * 10) else child.strip()
                                for child in block[block_index + 1 :]
                            )
                        else:
                            step.run = marker
                        break
                    block_index += 1
                current.steps.append(step)
            continue
        index += 1
    if not jobs:
        raise WorkflowContractError("workflow has no parsed jobs")
    return jobs


def _require(condition: bool, message: str) -> None:
    if not condition:
        raise WorkflowContractError(message)


def validate_release_contract_text(
    workflow_text: str,
    installer_text: str,
    installation_documents: tuple[str, ...] = (),
) -> None:
    jobs = parse_workflow(workflow_text)
    for required in ("verify", "bootstrap", "build", "publish"):
        _require(required in jobs, f"missing release job: {required}")
    for job_name in ("bootstrap", "build"):
        permissions = jobs[job_name].permissions
        for permission, value in {
            "contents": "read",
            "id-token": "write",
            "attestations": "write",
            "artifact-metadata": "write",
        }.items():
            _require(
                permissions.get(permission) == value,
                f"{job_name} lacks least-privilege {permission}: {value}",
            )

    for job in jobs.values():
        for step in job.steps:
            if step.uses:
                _require(FULL_PIN.fullmatch(step.uses) is not None, f"action is not SHA-pinned: {step.uses}")

    build_attests = [step for step in jobs["build"].steps if step.uses == ATTEST_ACTION]
    _require(len(build_attests) == 3, "build must contain exactly three actions/attest calls")
    by_id = {step.step_id: step for step in build_attests}
    _require(set(by_id) == {"attest-slsa", "attest-spdx", "attest-manifest"}, "attestation step IDs are not role-separated")
    _require(set(by_id["attest-slsa"].inputs) == {"subject-path"}, "SLSA call must use default provenance mode only")
    _require(by_id["attest-spdx"].inputs.get("sbom-path") == "${{ steps.package.outputs.sbom }}", "SPDX call lacks the canonical SBOM")
    _require("predicate-type" not in by_id["attest-spdx"].inputs, "SPDX call cannot alias the custom predicate")
    _require(by_id["attest-manifest"].inputs.get("predicate-type") == MANIFEST, "custom predicate type must be release-manifest v2")
    _require(by_id["attest-manifest"].inputs.get("predicate-path") == "${{ steps.package.outputs.manifest }}", "custom attestation must sign the canonical manifest")

    preserve = {step.name: step.run for step in jobs["build"].steps if step.name.startswith("Preserve")}
    expected_bundle_outputs = {
        "Preserve SLSA bundle": "steps.package.outputs.slsa_bundle",
        "Preserve SPDX bundle": "steps.package.outputs.spdx_bundle",
        "Preserve release-manifest bundle": "steps.package.outputs.manifest_bundle",
    }
    _require(set(expected_bundle_outputs).issubset(preserve), "three bundle-preservation steps are required")
    for name, output in expected_bundle_outputs.items():
        _require(output in preserve[name], f"{name} writes the wrong output")
    _require(len(set(preserve[name] for name in expected_bundle_outputs)) == 3, "bundle outputs are aliased")

    bootstrap_attests = [step for step in jobs["bootstrap"].steps if step.uses == ATTEST_ACTION]
    _require(len(bootstrap_attests) == 2, "installer and verifier need separate SLSA attestations")
    _require(all(set(step.inputs) == {"subject-path"} for step in bootstrap_attests), "bootstrap attestations must use SLSA mode")

    publish = jobs["publish"]
    verify_indexes = [index for index, step in enumerate(publish.steps) if "verify-release" in step.run]
    release_indexes = [index for index, step in enumerate(publish.steps) if step.uses.startswith("softprops/action-gh-release@")]
    _require(verify_indexes and len(release_indexes) == 1, "publish must verify and contain one publication step")
    _require(max(verify_indexes) < release_indexes[0], "verification must precede publication")
    verification_run = "\n".join(step.run for step in publish.steps[: release_indexes[0]])
    for required in (
        "--source-digest",
        "--bundle",
        ".slsa.bundle.json",
        ".spdx.bundle.json",
        ".release-manifest.bundle.json",
        "validate-previous-release",
        "gh release list",
        "first_release refused because a previous GitHub release exists",
    ):
        _require(required in verification_run, f"publish verification lacks {required}")

    combined = workflow_text + "\n" + installer_text
    all_installation_text = combined + "\n" + "\n".join(installation_documents)
    _require("--source-repo" not in combined, "unsupported gh --source-repo flag is forbidden")
    _require(
        "--signer-repo" not in combined,
        "gh --signer-repo cannot be combined with exact --signer-workflow",
    )
    for required in (
        "--repo",
        "--signer-workflow",
        "--source-ref",
        "--source-digest",
        "--cert-oidc-issuer",
        "--deny-self-hosted-runners",
    ):
        _require(required in installer_text, f"bootstrap verification lacks {required}")
    _require(
        "raw.githubusercontent.com" not in all_installation_text,
        "mutable raw GitHub bootstrap is forbidden",
    )
    _require("/main/" not in all_installation_text, "mutable main bootstrap is forbidden")
    _require("releases/latest" not in installer_text, "bootstrap must not select latest release")
    _require("| bash" not in combined and "| sh" not in combined, "unverified curl-to-shell is forbidden")
    _require(installer_text.index("verify-release") < installer_text.index("extract-archive"), "exact bundle verification must precede extraction")
    _require("--bootstrap-bundle" in installer_text and "--source-commit" in installer_text, "bootstrap digest/attestation and source commit are mandatory")
    _require("verify_slsa_asset \"${SCRIPT_PATH}\"" in installer_text, "installer must reverify its exact local bytes")


def main(argv: list[str]) -> int:
    if len(argv) < 3:
        print(
            "usage: release_workflow_contract.py WORKFLOW INSTALLER [INSTALL_DOC ...]",
            file=sys.stderr,
        )
        return 2
    try:
        validate_release_contract_text(
            Path(argv[1]).read_text(encoding="utf-8"),
            Path(argv[2]).read_text(encoding="utf-8"),
            tuple(Path(path).read_text(encoding="utf-8") for path in argv[3:]),
        )
    except (OSError, WorkflowContractError, ValueError) as exc:
        print(f"release workflow contract failed: {exc}", file=sys.stderr)
        return 1
    print("release workflow contract ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
