#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${ROOT}"

# Keep PostgreSQL coverage explicit. The ordinary Rust lane already runs every
# default-feature target; this lane reruns the library with PostgreSQL enabled
# and only the integration targets that contain pg-tests-gated cases.
expected_targets=(
  test_pe6_fault_drills
  test_pg_integration
  test_product_golden_path_g2
  test_product_golden_path_recovery
)

mapfile -t actual_targets < <(
  git grep -l 'feature = "pg-tests"' -- 'engine/tests/*.rs' \
    | sed -E 's#^engine/tests/##; s#\.rs$##' \
    | sort
)

if [[ "${actual_targets[*]}" != "${expected_targets[*]}" ]]; then
  echo "PostgreSQL-gated integration target set changed." >&2
  echo "Expected: ${expected_targets[*]}" >&2
  echo "Actual:   ${actual_targets[*]}" >&2
  echo "Update this runner deliberately so new PostgreSQL tests cannot be skipped." >&2
  exit 1
fi

cargo test -p engine --features pg-tests --lib -- --test-threads=1
for target in "${expected_targets[@]}"; do
  cargo test -p engine --features pg-tests --test "${target}" -- --test-threads=1
done
