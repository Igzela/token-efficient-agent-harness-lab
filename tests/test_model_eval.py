import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

from harness_core.evaluation import EvalSpec
from harness_core.model_eval import ControlledModelEvalHarness, ModelEvalReport
from harness_core.model_gateway import create_default_gateway


FIXTURE_DIR = Path(__file__).resolve().parents[1] / "docs" / "stage0"
EVENTS_FILE = FIXTURE_DIR / "events.jsonl"


def make_cases():
    return (
        EvalSpec(
            case_id="case_1",
            fixture_path=EVENTS_FILE,
            expected_outcome="pass",
        ),
        EvalSpec(
            case_id="case_2",
            fixture_path=EVENTS_FILE,
            expected_outcome="pass",
        ),
    )


class ControlledModelEvalHarnessTests(unittest.TestCase):
    def test_stub_only_suite(self):
        stub_gw = create_default_gateway()
        harness = ControlledModelEvalHarness(stub_gateway=stub_gw)
        report = harness.run_suite("suite_1", make_cases())
        self.assertIsInstance(report, ModelEvalReport)
        self.assertEqual("suite_1", report.suite_id)
        self.assertGreaterEqual(len(report.cases), 1)

    def test_real_result_none_when_no_real_gateway(self):
        stub_gw = create_default_gateway()
        harness = ControlledModelEvalHarness(stub_gateway=stub_gw)
        report = harness.run_suite("suite_2", make_cases())
        for case in report.cases:
            self.assertIsNone(case.real_result)

    def test_real_score_none_when_no_real_gateway(self):
        stub_gw = create_default_gateway()
        harness = ControlledModelEvalHarness(stub_gateway=stub_gw)
        report = harness.run_suite("suite_3", make_cases())
        self.assertIsNone(report.real_score)

    def test_deterministic_stub_score(self):
        stub_gw = create_default_gateway()
        harness = ControlledModelEvalHarness(stub_gateway=stub_gw)
        r1 = harness.run_suite("det_1", make_cases())
        r2 = harness.run_suite("det_1", make_cases())
        self.assertEqual(
            r1.stub_score.aggregate_score,
            r2.stub_score.aggregate_score,
        )

    def test_failing_case_does_not_abort_suite(self):
        stub_gw = create_default_gateway()
        harness = ControlledModelEvalHarness(stub_gateway=stub_gw)
        bad_fixture = Path("/nonexistent/path/file.jsonl")
        cases = (
            EvalSpec(
                case_id="good",
                fixture_path=EVENTS_FILE,
                expected_outcome="pass",
            ),
            EvalSpec(
                case_id="bad",
                fixture_path=bad_fixture,
                expected_outcome="pass",
            ),
        )
        report = harness.run_suite("fail_suite", cases)
        self.assertGreaterEqual(len(report.cases), 2)

    def test_no_network_call_path(self):
        stub_gw = create_default_gateway()
        harness = ControlledModelEvalHarness(stub_gateway=stub_gw)
        # Verify provider is stub only
        self.assertEqual("stub", stub_gw.invoke("strong_planner", "test").provider)

    def test_recommendation_stub_sufficient(self):
        stub_gw = create_default_gateway()
        harness = ControlledModelEvalHarness(stub_gateway=stub_gw)
        report = harness.run_suite("rec_suite", make_cases())
        self.assertEqual("stub_is_sufficient", report.recommendation)

    def test_empty_cases(self):
        stub_gw = create_default_gateway()
        harness = ControlledModelEvalHarness(stub_gateway=stub_gw)
        report = harness.run_suite("empty", ())
        self.assertEqual(0, report.stub_score.item_count)


if __name__ == "__main__":
    unittest.main()
