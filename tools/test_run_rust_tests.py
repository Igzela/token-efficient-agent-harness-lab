from __future__ import annotations

import importlib.util
from pathlib import Path
import sys
import tempfile
import unittest


SCRIPT = Path(__file__).resolve().parents[1] / "scripts" / "ci" / "run_rust_tests.py"
SPEC = importlib.util.spec_from_file_location("run_rust_tests", SCRIPT)
assert SPEC and SPEC.loader
run_rust_tests = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = run_rust_tests
SPEC.loader.exec_module(run_rust_tests)


class RustTestModeTests(unittest.TestCase):
    def test_current_unrepaired_tree_stays_serial(self) -> None:
        root = Path(__file__).resolve().parents[1]
        self.assertFalse(run_rust_tests.parallel_contract_present(root))

    def test_complete_canonical_lock_contract_enables_parallel(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            tests = root / "engine/tests"
            (tests / "http_server").mkdir(parents=True)
            harness = []
            common = []
            for name in run_rust_tests.LOCK_NAMES:
                harness.append(
                    f"fn {name}() -> &'static Mutex<()> {{ common::{name}() }}"
                )
                common.append(
                    f"pub(crate) fn {name}() -> &'static Mutex<()> {{ todo!() }}"
                )
            auth = []
            for name in run_rust_tests.AUTH_TESTS:
                auth.append(
                    f"async fn {name}() {{ let _lock = "
                    "provider_cli_env_lock().lock().await; }}"
                )
            (tests / "test_http_server.rs").write_text(
                "\n".join(harness), encoding="utf-8"
            )
            (tests / "http_server/common.rs").write_text(
                "\n".join(common), encoding="utf-8"
            )
            (tests / "http_server/auth.rs").write_text(
                "\n".join(auth), encoding="utf-8"
            )
            self.assertTrue(run_rust_tests.parallel_contract_present(root))

    def test_partial_contract_stays_serial(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            (root / "engine/tests/http_server").mkdir(parents=True)
            (root / "engine/tests/test_http_server.rs").write_text(
                "fn provider_cli_env_lock() -> X { common::provider_cli_env_lock() }",
                encoding="utf-8",
            )
            (root / "engine/tests/http_server/common.rs").write_text(
                "pub(crate) fn provider_cli_env_lock() -> X { todo!() }",
                encoding="utf-8",
            )
            (root / "engine/tests/http_server/auth.rs").write_text("", encoding="utf-8")
            self.assertFalse(run_rust_tests.parallel_contract_present(root))


if __name__ == "__main__":
    unittest.main()
