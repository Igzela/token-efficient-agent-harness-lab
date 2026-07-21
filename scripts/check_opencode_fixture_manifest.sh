#!/usr/bin/env bash
# Validate adapters/opencode/FIXTURE_ADAPTER_MANIFEST.json against on-disk sources.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MANIFEST="${ROOT}/adapters/opencode/FIXTURE_ADAPTER_MANIFEST.json"
PIN="${ROOT}/adapters/opencode/PIN.json"
PKG="${ROOT}/adapters/opencode"

if [[ ! -f "$MANIFEST" ]]; then
  echo "missing fixture adapter manifest: $MANIFEST" >&2
  exit 1
fi

python3 - <<'PY' "$MANIFEST" "$PIN" "$PKG"
import hashlib, json, sys
from pathlib import Path

manifest_path = Path(sys.argv[1])
pin_path = Path(sys.argv[2])
pkg = Path(sys.argv[3])

REQUIRED = [
    ("src/acp_opencode_adapter/__init__.py", "package_init"),
    ("src/acp_opencode_adapter/__main__.py", "entrypoint"),
    ("src/acp_opencode_adapter/adapter.py", "adapter_source"),
    ("pyproject.toml", "package_manifest"),
]

manifest = json.loads(manifest_path.read_text())
if manifest.get("schema_version") != "opencode_fixture_adapter_manifest.v1":
    raise SystemExit("invalid manifest schema_version")
if manifest.get("admission_status") != "fixture_adapter_only":
    raise SystemExit("admission_status must be fixture_adapter_only")
if manifest.get("binary_admission_status") != "not_admitted":
    raise SystemExit("binary_admission_status must be not_admitted")

artifacts = manifest.get("artifacts") or []
if not artifacts:
    raise SystemExit("artifacts must be non-empty")
if len(artifacts) != len(REQUIRED):
    raise SystemExit(f"expected exactly {len(REQUIRED)} artifacts")

seen_paths = set()
seen_roles = set()
for artifact in artifacts:
    rel = artifact["path"]
    role = artifact["role"]
    if not rel or rel.startswith("/") or ".." in rel:
        raise SystemExit(f"invalid artifact path: {rel}")
    if rel in seen_paths:
        raise SystemExit(f"duplicate path: {rel}")
    if role in seen_roles:
        raise SystemExit(f"duplicate role: {role}")
    seen_paths.add(rel)
    seen_roles.add(role)
    expected_role = dict(REQUIRED).get(rel)
    if expected_role is None:
        raise SystemExit(f"unexpected artifact path: {rel}")
    if role != expected_role:
        raise SystemExit(f"role mismatch for {rel}")
    expected = artifact["sha256"]
    if len(expected) != 64 or any(c not in "0123456789abcdef" for c in expected):
        raise SystemExit(f"invalid sha256 for {rel}")
    if set(expected) == {"0"}:
        raise SystemExit(f"placeholder all-zero checksum for {rel}")
    data = (pkg / rel).read_bytes()
    actual = hashlib.sha256(data).hexdigest()
    if actual != expected:
        raise SystemExit(f"sha256 mismatch for {rel}: expected {expected}, got {actual}")

for path, role in REQUIRED:
    if path not in seen_paths:
        raise SystemExit(f"missing path {path}")
    if role not in seen_roles:
        raise SystemExit(f"missing role {role}")

profile = {
    "approval_mode": "deny_by_default",
    "background_agents": False,
    "mcp_enabled": False,
    "network_enabled": False,
    "provider_fallback": False,
    "remote_agents": False,
    "webfetch": False,
    "websearch": False,
}
profile_json = json.dumps(profile, sort_keys=True, separators=(",", ":"), ensure_ascii=False)
profile_hash = hashlib.sha256(profile_json.encode()).hexdigest()
if manifest.get("permission_profile_hash") != profile_hash:
    raise SystemExit(
        f"permission_profile_hash mismatch: expected {profile_hash}, got {manifest.get('permission_profile_hash')}"
    )

pin = json.loads(pin_path.read_text())
if pin.get("binary_admission_status") != "not_admitted":
    raise SystemExit("PIN.json must declare binary_admission_status=not_admitted")
if pin.get("artifact_checksum_sha256") in (
    "0000000000000000000000000000000000000000000000000000000000000000",
):
    raise SystemExit("PIN.json must not use all-zero artifact checksum as admitted pin")
if pin.get("pinned_commit") == "fixture-pin-not-downloaded":
    raise SystemExit("PIN.json must not treat fixture-pin-not-downloaded as a real commit pin")

print("opencode fixture adapter manifest ok")
print(f"  artifacts={len(manifest['artifacts'])}")
print(f"  admission={manifest['admission_status']}")
print(f"  binary={manifest['binary_admission_status']}")
PY
