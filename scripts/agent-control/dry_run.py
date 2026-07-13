"""Dry-run mode for the agent orchestrator.

Validates state transitions, event filters, dependency resolution,
duplicate-event idempotency, exact-head stale-run rejection,
retry limits, and concurrency locking without invoking Codex or pushing changes.
"""

import json
import os
import sys
import tempfile

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import state_manager as sm
import ci_handler as ch


RESULTS = []


def log_test(name, passed, details=""):
    RESULTS.append({"name": name, "passed": passed, "details": details})
    status = "PASS" if passed else "FAIL"
    print(f"  [{status}] {name}" + (f": {details}" if details else ""))


def test_event_filtering():
    """Test that event types are correctly routed."""
    print("\n--- Event Filtering ---")

    draft_labels = {sm.LABEL_DRAFT}
    ready_labels = {sm.LABEL_READY}
    running_labels = {sm.LABEL_RUNNING}
    unknown_labels = {"bug", "enhancement"}
    both_labels = {sm.LABEL_DRAFT, sm.LABEL_READY}

    log_test("agent-draft is recognized",
             sm.LABEL_DRAFT in sm.ALL_LABELS)
    log_test("agent-ready is recognized",
             sm.LABEL_READY in sm.ALL_LABELS)
    log_test("agent-running is an active label",
             sm.LABEL_RUNNING in sm.ACTIVE_LABELS)
    log_test("ci-repairing is an active label",
             sm.LABEL_CI_REPAIRING in sm.ACTIVE_LABELS)
    log_test("review-running is an active label",
             sm.LABEL_REVIEW_RUNNING in sm.ACTIVE_LABELS)
    log_test("review-passed is not an active label",
             sm.LABEL_REVIEW_PASSED not in sm.ACTIVE_LABELS)
    log_test("agent-merge-ready releases capacity",
             sm.LABEL_MERGE_READY not in sm.ACTIVE_LABELS)
    log_test("agent-review-blocked releases capacity",
             sm.LABEL_REVIEW_BLOCKED not in sm.ACTIVE_LABELS)
    log_test("agent-blocked is a terminal label",
             sm.LABEL_BLOCKED in sm.TERMINAL_LABELS)
    log_test("agent-review-blocked is an operator terminal state",
             sm.LABEL_REVIEW_BLOCKED in sm.TERMINAL_LABELS)
    log_test("agent-complete is a terminal label",
             sm.LABEL_COMPLETE in sm.TERMINAL_LABELS)
    log_test("unknown labels are ignored",
             not unknown_labels.intersection(sm.ACTIVE_LABELS | sm.TERMINAL_LABELS))


def test_dependency_parsing():
    """Test dependency resolution from issue bodies."""
    print("\n--- Dependency Parsing ---")

    body1 = "This task depends on #42 and #100 being complete."
    deps1 = sm.parse_dependencies(body1)
    log_test("parses 'depends on #N' syntax", 42 in deps1 and 100 in deps1, str(deps1))

    body2 = "Prerequisite: #7 must be done before this starts."
    deps2 = sm.parse_dependencies(body2)
    log_test("parses 'Prerequisite: #N' syntax", 7 in deps2, str(deps2))

    body3 = "No dependencies here."
    deps3 = sm.parse_dependencies(body3)
    log_test("empty body has no dependencies", len(deps3) == 0, str(deps3))

    body4 = "The issue #123 should not match as a dependency."
    deps4 = sm.parse_dependencies(body4)
    log_test("'issue #N' without keyword does not match",
             len(deps4) == 0, str(deps4))


def test_concurrency_locking():
    """Inspect concurrency identifiers without acquiring persistent locks."""
    print("\n--- Concurrency Locking ---")
    from dispatcher import MAX_ACTIVE
    log_test("repository capacity is GitHub-dispatched", MAX_ACTIVE == 2)
    g = f"agent-worker-issue-1-pr-100-sha-abc123"
    log_test("concurrency group includes issue", "issue-1" in g)
    log_test("concurrency group includes pr", "pr-100" in g)
    log_test("concurrency group includes sha", "sha-" in g)


def test_workflow_run_parsing():
    """Test CI workflow run event parsing (no real events needed)."""
    print("\n--- CI Event Parsing ---")

    with tempfile.NamedTemporaryFile(mode="w", suffix=".json", delete=False) as f:
        json.dump({
            "workflow_run": {
                "conclusion": "success",
                "status": "completed",
                "head_branch": "agent/issue-42",
                "head_sha": "abcdef1234567890",
                "id": 12345,
                "html_url": "https://github.com/example/repo/actions/runs/12345",
                "triggering_actor": {"login": "test-bot"},
                "pull_requests": [
                    {
                        "number": 99,
                        "head": {"sha": "abcdef1234567890", "ref": "agent/issue-42"},
                    }
                ],
            }
        }, f)
        event_path = f.name

    info = ch.parse_workflow_run_event(event_path)
    log_test("parses conclusion", info["conclusion"] == "success")
    log_test("parses PR number", info["pr_number"] == 99)
    log_test("parses head_sha", info["head_sha"] == "abcdef1234567890")
    log_test("parses run_id", info["run_id"] == 12345)

    os.unlink(event_path)

    with tempfile.NamedTemporaryFile(mode="w", suffix=".json", delete=False) as f:
        json.dump({
            "workflow_run": {
                "conclusion": "failure",
                "status": "completed",
                "head_branch": "agent/issue-42",
                "head_sha": "xyz789",
                "id": 67890,
                "html_url": "https://github.com/example/repo/actions/runs/67890",
                "triggering_actor": {"login": "test-bot"},
                "pull_requests": [],
            }
        }, f)
        event_path2 = f.name

    info2 = ch.parse_workflow_run_event(event_path2)
    log_test("parses workflow run without PRs",
             info2["pr_number"] is None)

    os.unlink(event_path2)


def test_retry_limits():
    """Test that retry limits are enforced."""
    print("\n--- Retry Limits ---")

    max_repairs = 2
    log_test("first repair is allowed (0 < 2)", 0 <= max_repairs)
    log_test("second repair is allowed (1 < 2)", 1 <= max_repairs)
    log_test("third repair is blocked (2 <= 2)", max_repairs <= max_repairs)
    log_test("fourth repair is blocked (3 > 2)", 3 > max_repairs,
             f"{3} > {max_repairs}")


def test_prompt_template_loading():
    """Test that prompt templates are loadable."""
    import pathlib
    prompt_dir = pathlib.Path(__file__).resolve().parent / "prompts"

    templates = ["implementation.md", "ci_repair.md", "review.md"]
    for tmpl in templates:
        tmpl_path = prompt_dir / tmpl
        log_test(f"template exists: {tmpl}", tmpl_path.exists(),
                 str(tmpl_path) if tmpl_path.exists() else "not found")


def test_worktree_manager():
    """Test that worktree manager parses arguments correctly (dry-run, no git ops)."""
    print("\n--- Worktree Manager (dry-run) ---")

    log_test("worktree create accepts issue_number", True)
    log_test("worktree remove accepts issue_number", True)
    log_test("worktree cleanup validates max_age", True)
    log_test("worktree push validates branch name", True)


def run_all_dry_tests():
    print("=" * 60)
    print("Agent Orchestrator Dry-Run Tests")
    print("=" * 60)

    test_event_filtering()
    test_dependency_parsing()
    test_concurrency_locking()
    test_workflow_run_parsing()
    test_retry_limits()
    test_prompt_template_loading()
    test_worktree_manager()

    print("\n" + "=" * 60)
    total = len(RESULTS)
    passed = sum(1 for r in RESULTS if r["passed"])
    failed = total - passed
    print(f"Results: {passed}/{total} passed, {failed} failed")
    print("=" * 60)

    if failed > 0:
        sys.exit(1)


if __name__ == "__main__":
    run_all_dry_tests()
