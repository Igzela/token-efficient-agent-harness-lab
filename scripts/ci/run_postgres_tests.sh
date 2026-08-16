#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${ROOT}"

# Keep PostgreSQL coverage explicit. Targets remain internally serial, but each
# target receives a distinct database and may run concurrently with the other
# targets after one shared compile. This removes cross-target state collisions
# without hiding any pg-tests-gated target.
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

if [[ -z "${PG_CONTAINER:-}" ]]; then
  echo "PG_CONTAINER is required for isolated PostgreSQL target databases." >&2
  exit 1
fi

compile_args=(-p engine --features pg-tests --lib --no-run)
for target in "${expected_targets[@]}"; do
  compile_args+=(--test "${target}")
done
cargo test "${compile_args[@]}"

targets=(engine_lib "${expected_targets[@]}")
log_dir="$(mktemp -d -t acp-pg-targets.XXXXXX)"
pids=()

cleanup() {
  for pid in "${pids[@]}"; do
    if kill -0 "${pid}" 2>/dev/null; then
      kill "${pid}" 2>/dev/null || true
    fi
  done
  wait 2>/dev/null || true
  rm -rf "${log_dir}"
}
trap cleanup EXIT

for target in "${targets[@]}"; do
  database="ci_${target}"
  docker exec "${PG_CONTAINER}" psql \
    --username testuser \
    --dbname postgres \
    --set ON_ERROR_STOP=1 \
    --command "CREATE DATABASE ${database}" >/dev/null
  database_url="postgres://testuser:testpass@localhost:5432/${database}"
  if [[ "${target}" == "engine_lib" ]]; then
    (
      ACP_TEST_DATABASE_URL="${database_url}" \
        cargo test -p engine --features pg-tests --lib -- --test-threads=1
    ) >"${log_dir}/${target}.log" 2>&1 &
  else
    (
      ACP_TEST_DATABASE_URL="${database_url}" \
        cargo test -p engine --features pg-tests --test "${target}" -- --test-threads=1
    ) >"${log_dir}/${target}.log" 2>&1 &
  fi
  pids+=("$!")
done

failed=0
set +e
for index in "${!targets[@]}"; do
  wait "${pids[${index}]}"
  status=$?
  target="${targets[${index}]}"
  echo "===== PostgreSQL target: ${target} ====="
  sed -n '1,2000p' "${log_dir}/${target}.log"
  if [[ "${status}" -ne 0 ]]; then
    echo "PostgreSQL target ${target} failed with status ${status}." >&2
    failed=1
  fi
done
set -e

exit "${failed}"
