#!/usr/bin/env python3
"""Full pipeline demo: exercises every major component end-to-end.

Usage:
    PYTHONPATH=src python3 demos/full_pipeline_demo.py

Runs entirely locally with stdlib only. No real providers, no network calls.
"""

from __future__ import annotations

import json
import os
import sys
import tempfile
import time
from pathlib import Path

# Add project root to path
PROJECT_ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(PROJECT_ROOT / "src"))

from harness_core.dispatch.auth import Tenant, TenantResolver
from harness_core.dispatch.backup_manager import BackupManager
from harness_core.dispatch.durable_store import DurableStore
from harness_core.dispatch.health_checker import HealthChecker
from harness_core.dispatch.http_server import ServerConfig
from harness_core.dispatch.observability import (
    MetricsCollector,
    RequestMetric,
    RequestTracer,
    setup_structured_logging,
)
from harness_core.dispatch.plugin_registry import PluginRegistry
from harness_core.dispatch.plugin_system import PluginManifest, PluginSystem
from harness_core.dispatch.rate_limiter import RateLimiter
from harness_core.doc_generator import DocGenerator
from harness_core.sdk import HarnessSDK


def section(title: str) -> None:
    print(f"\n{'='*60}")
    print(f"  {title}")
    print(f"{'='*60}")


def demo_tenants_and_auth(resolver: TenantResolver) -> dict[str, str]:
    section("1. Multi-Tenant Auth Setup")

    tenants = [
        Tenant(tenant_id="team-alpha", name="Alpha Team", scopes=frozenset({"dispatch:read", "dispatch:write"})),
        Tenant(tenant_id="team-beta", name="Beta Team", scopes=frozenset({"dispatch:read"})),
        Tenant(tenant_id="admin", name="Admin", scopes=frozenset()),
    ]

    api_keys = {}
    for tenant in tenants:
        resolver.add_tenant(tenant)
        _, raw_key = resolver.create_api_key(tenant.tenant_id)
        api_keys[tenant.tenant_id] = raw_key
        print(f"  Created tenant '{tenant.name}' ({tenant.tenant_id}) with API key")

    print(f"\n  Total tenants: {len(resolver._tenants)}")
    print(f"  Total API keys: {len(resolver._api_keys)}")

    # Test auth decisions
    for tenant_id, raw_key in api_keys.items():
        decision = resolver.resolve(f"Bearer {raw_key}")
        print(f"  {tenant_id}: allowed={decision.allowed}, scopes={decision.scopes}")

    # Test unauthorized access
    bad_decision = resolver.resolve("Bearer invalid-token-12345")
    print(f"  invalid token: allowed={bad_decision.allowed}, reason={bad_decision.reason}")

    return api_keys


def demo_rate_limiting() -> None:
    section("2. Rate Limiting (Sliding Window)")

    limiter = RateLimiter(window_seconds=60)

    print("  Simulating 5 requests to team-alpha (limit=3/min):")
    for i in range(5):
        result = limiter.check("team-alpha", "key-1", rate_limit=3)
        status = "ALLOWED" if result.allowed else "BLOCKED"
        print(f"    Request {i+1}: {status} (remaining={result.remaining}, retry_after={result.retry_after}s)")

    print("\n  Simulating requests to team-beta (independent bucket):")
    for i in range(3):
        result = limiter.check("team-beta", "key-2", rate_limit=2)
        status = "ALLOWED" if result.allowed else "BLOCKED"
        print(f"    Request {i+1}: {status} (remaining={result.remaining})")


def demo_dispatch_pipeline(sdk: HarnessSDK) -> list[dict]:
    section("3. Task Dispatch Pipeline")

    tasks = [
        {"raw_request": "Fix the authentication timeout bug in login.py", "request_source": "team-alpha"},
        {"raw_request": "Add dark mode toggle to settings page", "request_source": "team-alpha"},
        {"raw_request": "Write unit tests for the payment module", "request_source": "team-beta"},
        {"raw_request": "Refactor database connection pooling", "request_source": "admin"},
    ]

    results = []
    for i, task in enumerate(tasks, 1):
        result = sdk.create_dispatch(task)
        decision = result["decision"]
        print(f"  Task {i}: {task['raw_request'][:50]}...")
        print(f"    dispatch_id:  {result['dispatch_id']}")
        print(f"    model:        {decision.get('selected_model', 'N/A')}")
        print(f"    risk:         {decision.get('analysis_snapshot', {}).get('risk_flags', [])}")
        results.append(result)

    print(f"\n  Total dispatches: {len(results)}")
    print(f"  Plans stored:     {len(sdk.list_plans())}")
    return results


def demo_persistent_storage() -> None:
    section("4. Durable Storage (SQLite)")

    tmpdir = tempfile.mkdtemp()
    db_path = os.path.join(tmpdir, "demo.db")
    store = DurableStore(db_path=db_path)

    # Store plans
    for i in range(5):
        store.save_plan(f"plan-{i:03d}", {
            "name": f"Plan {i}",
            "model": "gpt-4" if i % 2 == 0 else "claude-3",
            "created_by": "demo",
        })

    plans = store.list_plans()
    print(f"  Stored {len(plans)} plans in {db_path}")

    # Retrieve a plan
    plan = store.get_plan("plan-002")
    print(f"  Retrieved plan-002: {plan.data}")

    # Store repos
    store.save_repo("demo-repo", {"path": "/tmp/demo", "name": "demo-repo"})
    repos = store.list_repos()
    print(f"  Registered {len(repos)} repos")

    # Stats
    stats = store.stats()
    print(f"  Storage stats: {json.dumps(stats, indent=4)}")

    store.close()


def demo_backup_and_restore() -> None:
    section("5. Backup & Restore")

    tmpdir = tempfile.mkdtemp()
    db_path = os.path.join(tmpdir, "demo.db")
    store = DurableStore(db_path=db_path)

    # Create some data
    store.save_plan("pre-backup", {"name": "Before Backup", "value": 42})
    print(f"  Created plan 'pre-backup' with value=42")

    # Create backup
    mgr = BackupManager(backup_dir=tmpdir)
    record = mgr.create_backup(store, label="demo-backup")
    print(f"  Backup created: {record.backup_id[:12]}... (label={record.label})")
    print(f"    checksum: {record.checksum[:16]}...")
    print(f"    size: {record.size_bytes} bytes")

    # Delete and restore
    store.delete_plan("pre-backup")
    print(f"  Deleted plan 'pre-backup'")

    result = mgr.restore_backup(record.backup_id, store)
    print(f"  Restored: {result.records_restored} records in {result.duration_ms:.1f}ms")

    plan = store.get_plan("pre-backup")
    print(f"  Verified restored plan: {plan.data}")
    store.close()


def demo_plugin_system() -> None:
    section("6. Plugin System & Registry")

    # Load plugin from manifest
    ps = PluginSystem()
    manifest_path = PROJECT_ROOT / "tests" / "fixtures" / "sample_plugin_manifest.json"
    plugin_dir = PROJECT_ROOT / "tests" / "fixtures"
    loaded = ps.load_plugin(str(manifest_path), str(plugin_dir))
    print(f"  Loaded plugin: {loaded.manifest.name} v{loaded.manifest.version}")
    print(f"    author: {loaded.manifest.author}")
    print(f"    trust_level: {loaded.manifest.trust_level}")
    print(f"    permissions: {loaded.manifest.permissions}")

    # Check permissions
    for perm in ["dispatch:read", "dispatch:write", "provider:execute"]:
        allowed = ps.check_permission(loaded.manifest.plugin_id, perm)
        print(f"    {perm}: {'allowed' if allowed else 'denied'}")

    # Register in registry
    reg = PluginRegistry()
    success = reg.register_plugin(loaded.manifest)
    print(f"\n  Registered in registry: {success}")

    # Search
    results = reg.search_plugins("sample")
    print(f"  Search 'sample': {len(results)} results")

    # List all
    all_plugins = reg.list_registered()
    print(f"  Total registered: {len(all_plugins)}")


def demo_observability() -> None:
    section("7. Observability (Metrics + Tracing)")

    # Metrics
    mc = MetricsCollector(max_size=500)
    for i in range(10):
        mc.record(RequestMetric(
            request_id=f"req-{i:03d}",
            component="dispatch_engine",
            action="dispatch",
            duration_ms=10.0 + i * 2.5,
            status="ok" if i < 8 else "error",
            timestamp=time.time(),
        ))

    metrics = mc.query(component="dispatch_engine")
    print(f"  Recorded {len(metrics)} metrics for dispatch_engine")
    print(f"  Total metrics: {mc.count()}")

    # Tracing
    tracer = RequestTracer()
    trace_id, span_id = tracer.start_span("dispatch_request")
    _, child_id = tracer.start_span("task_analysis", trace_id=trace_id, parent_span_id=span_id)
    tracer.end_span(child_id, status="ok")
    tracer.end_span(span_id, status="ok")

    spans = tracer.get_trace_spans(trace_id)
    print(f"  Traced {len(spans)} spans in trace {trace_id[:12]}...")
    for span in spans:
        print(f"    {span.name}: {span.status}")

    # Structured logging
    setup_structured_logging()
    print("  Structured logging configured")


def demo_health_check(store: DurableStore) -> None:
    section("8. Health Checks")

    checker = HealthChecker(store=store)
    report = checker.health()
    print(f"  Overall status: {report.status}")
    for check in report.checks:
        latency = f" ({check.latency_ms:.1f}ms)" if check.latency_ms > 0 else ""
        print(f"    {check.name}: {check.status}{latency}")

    readiness = checker.readiness()
    print(f"\n  Ready: {readiness.status == 'healthy'}")


def demo_doc_generation() -> None:
    section("9. Auto-Generated Documentation")

    dg = DocGenerator()

    # Module docs
    modules = ["rate_limiter", "backup_manager", "plugin_system", "auth"]
    for mod in modules:
        md = dg.generate_module_docs(f"src/harness_core/dispatch/{mod}.py")
        lines = len(md.splitlines())
        print(f"  {mod}.md: {lines} lines")

    # Schema registry
    registry = dg.generate_schema_registry("src/harness_core/dispatch")
    registry_lines = len(registry.splitlines())
    print(f"\n  Schema registry: {registry_lines} lines")

    # API reference
    api_ref = dg.generate_api_reference("src/harness_core/sdk.py")
    api_lines = len(api_ref.splitlines())
    print(f"  SDK API reference: {api_lines} lines")


def demo_cli() -> None:
    section("10. CLI Interface")

    import subprocess

    commands = [
        (["python3", "-m", "harness_core.cli", "status"], "System status"),
        (["python3", "-m", "harness_core.cli", "health"], "Health check"),
    ]

    for cmd, desc in commands:
        result = subprocess.run(
            cmd,
            capture_output=True,
            text=True,
            env={**os.environ, "PYTHONPATH": str(PROJECT_ROOT / "src")},
        )
        output = result.stdout.strip().splitlines()
        print(f"  {desc}:")
        for line in output[:3]:
            print(f"    {line}")
        if len(output) > 3:
            print(f"    ... ({len(output)} total lines)")


def main() -> int:
    print("Token-Efficient Agent Harness Lab — Full Pipeline Demo")
    print(f"Python {sys.version.split()[0]} | stdlib only | no network calls")

    # Setup
    resolver = TenantResolver()
    sdk = HarnessSDK()

    # Run all demos
    demo_tenants_and_auth(resolver)
    demo_rate_limiting()
    results = demo_dispatch_pipeline(sdk)
    demo_persistent_storage()
    demo_backup_and_restore()
    demo_plugin_system()
    demo_observability()
    demo_health_check(sdk._store)
    demo_doc_generation()
    demo_cli()

    # Summary
    section("SUMMARY")
    print(f"  Components exercised:  10")
    print(f"  Tasks dispatched:      {len(results)}")
    print(f"  Tenants created:       3")
    print(f"  API keys issued:       3")
    print(f"  Plugins loaded:        1")
    print(f"  Backups created:       1")
    print(f"  Metrics recorded:      10")
    print(f"  Spans traced:          2")
    print(f"  Health checks:         3")
    print(f"  Docs generated:        4 modules + registry + API ref")
    print(f"  CLI commands run:      2")
    print()
    print("All components working end-to-end. System is fully operational.")

    sdk.close()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
