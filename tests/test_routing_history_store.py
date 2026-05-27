"""Tests for routing/history_store.py — tier-aware history indexing."""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

import unittest
from harness_core.dispatch.routing.history_store import RoutingHistoryStore, _task_group_from_row
from harness_core.usage_ledger import UsageLedgerRow


def _make_row(
    profile_id: str = "profile-1",
    cost_group: str = "suite/code/review/quality",
    cost: float = 0.01,
    passed: bool = True,
    input_tokens: int = 100,
    output_tokens: int = 50,
) -> UsageLedgerRow:
    return UsageLedgerRow(
        run_id="run-1",
        case_id="case-1",
        input_tokens=input_tokens,
        output_tokens=output_tokens,
        cached_tokens=0,
        request_count=1,
        tool_call_count=0,
        retry_count=0,
        wall_clock_ms=100,
        estimated_cost=cost,
        pass_=passed,
        cost_of_pass_group=cost_group,
        model_profile_id=profile_id,
        context_pack_id="",
    )


class TaskGroupExtractionTests(unittest.TestCase):
    def test_valid_group(self):
        row = _make_row(cost_group="suite/code/review/quality")
        tg = _task_group_from_row(row)
        self.assertEqual(tg, "code_review")

    def test_invalid_group(self):
        row = _make_row(cost_group="invalid")
        tg = _task_group_from_row(row)
        self.assertIsNone(tg)


class RoutingHistoryStoreTests(unittest.TestCase):
    def test_empty_store(self):
        store = RoutingHistoryStore()
        self.assertEqual(store.total_rows(), 0)
        self.assertIsNone(store.aggregate_by_tier("cheap_executor"))
        self.assertEqual(store.sample_count("code_review"), 0)
        self.assertEqual(store.tiers_observed("code_review"), ())

    def test_add_row(self):
        store = RoutingHistoryStore()
        store.add_row(_make_row(profile_id="p1"))
        self.assertEqual(store.total_rows(), 1)

    def test_tier_profile_map(self):
        store = RoutingHistoryStore(tier_profile_map={"p1": "cheap_executor"})
        self.assertEqual(store.tier_for_profile("p1"), "cheap_executor")
        self.assertIsNone(store.tier_for_profile("p2"))

    def test_set_tier_map(self):
        store = RoutingHistoryStore()
        store.set_tier_map({"p1": "balanced_worker"})
        self.assertEqual(store.tier_for_profile("p1"), "balanced_worker")

    def test_rows_by_tier(self):
        store = RoutingHistoryStore(tier_profile_map={"p1": "cheap_executor", "p2": "balanced_worker"})
        store.add_row(_make_row(profile_id="p1"))
        store.add_row(_make_row(profile_id="p2"))
        store.add_row(_make_row(profile_id="p1"))
        cheap = store.rows_by_tier("cheap_executor")
        balanced = store.rows_by_tier("balanced_worker")
        self.assertEqual(len(cheap), 2)
        self.assertEqual(len(balanced), 1)

    def test_rows_by_task_group(self):
        store = RoutingHistoryStore()
        store.add_row(_make_row(cost_group="suite/code/review/quality"))
        store.add_row(_make_row(cost_group="suite/docs/summarize/speed"))
        store.add_row(_make_row(cost_group="suite/code/review/quality"))
        rows = store.rows_by_task_group("code_review")
        self.assertEqual(len(rows), 2)

    def test_rows_by_tier_and_task_group(self):
        store = RoutingHistoryStore(tier_profile_map={"p1": "cheap_executor", "p2": "balanced_worker"})
        store.add_row(_make_row(profile_id="p1", cost_group="s/code/review/q"))
        store.add_row(_make_row(profile_id="p2", cost_group="s/code/review/q"))
        store.add_row(_make_row(profile_id="p1", cost_group="s/docs/sum/s"))
        rows = store.rows_by_tier_and_task_group("cheap_executor", "code_review")
        self.assertEqual(len(rows), 1)
        self.assertEqual(rows[0].model_profile_id, "p1")

    def test_aggregate_by_tier(self):
        store = RoutingHistoryStore(tier_profile_map={"p1": "cheap_executor"})
        store.add_row(_make_row(profile_id="p1", cost_group="s/c/r/q", cost=0.01, passed=True))
        store.add_row(_make_row(profile_id="p1", cost_group="s/c/r/q", cost=0.02, passed=False))
        agg = store.aggregate_by_tier("cheap_executor")
        self.assertIsNotNone(agg)
        self.assertEqual(agg.total_count, 2)
        self.assertEqual(agg.success_count, 1)
        self.assertEqual(agg.failure_count, 1)

    def test_aggregate_by_tier_and_task_group(self):
        store = RoutingHistoryStore(tier_profile_map={"p1": "cheap_executor"})
        store.add_row(_make_row(profile_id="p1", cost_group="s/code/review/q", cost=0.01, passed=True))
        store.add_row(_make_row(profile_id="p1", cost_group="s/docs/sum/s", cost=0.02, passed=True))
        agg = store.aggregate_by_tier_and_task_group("cheap_executor", "code_review")
        self.assertIsNotNone(agg)
        self.assertEqual(agg.total_count, 1)

    def test_sample_count(self):
        store = RoutingHistoryStore()
        store.add_row(_make_row(cost_group="s/c/r/q"))
        store.add_row(_make_row(cost_group="s/c/r/q"))
        store.add_row(_make_row(cost_group="s/d/s/q"))
        self.assertEqual(store.sample_count("c_r"), 2)
        self.assertEqual(store.sample_count("d_s"), 1)

    def test_sample_count_for_tier(self):
        store = RoutingHistoryStore(tier_profile_map={"p1": "cheap_executor", "p2": "balanced_worker"})
        store.add_row(_make_row(profile_id="p1", cost_group="s/c/r/q"))
        store.add_row(_make_row(profile_id="p2", cost_group="s/c/r/q"))
        self.assertEqual(store.sample_count_for_tier("c_r", "cheap_executor"), 1)

    def test_tiers_observed(self):
        store = RoutingHistoryStore(tier_profile_map={"p1": "cheap_executor", "p2": "balanced_worker"})
        store.add_row(_make_row(profile_id="p1", cost_group="s/c/r/q"))
        store.add_row(_make_row(profile_id="p2", cost_group="s/c/r/q"))
        tiers = store.tiers_observed("c_r")
        self.assertEqual(tiers, ("balanced_worker", "cheap_executor"))

    def test_all_rows(self):
        store = RoutingHistoryStore()
        store.add_row(_make_row())
        store.add_row(_make_row())
        self.assertEqual(len(store.all_rows()), 2)


if __name__ == "__main__":
    unittest.main()
