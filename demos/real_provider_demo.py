#!/usr/bin/env python3
"""Real provider demo: dispatches tasks to mimo via Anthropic-compatible API.

Requires ANTHROPIC_AUTH_TOKEN environment variable to be set.
API key is read from env only — never saved to disk.

Usage:
    PYTHONPATH=src python3 demos/real_provider_demo.py
"""

from __future__ import annotations

import json
import os
import sys
import time
from pathlib import Path

PROJECT_ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(PROJECT_ROOT / "src"))

from harness_core.dispatch.dispatch_engine import DispatchEngine
from harness_core.dispatch.provider.anthropic_provider import (
    AnthropicProvider,
    anthropic_urlopen,
    AnthropicProviderRequest,
)
from harness_core.dispatch.provider.audit_recorder import ProviderAuditRecorder
from harness_core.dispatch.provider.credential_boundary import CredentialBoundary
from harness_core.dispatch.provider.provider_config import CredentialRef, ProviderConfig
from harness_core.dispatch.provider.retry_manager import RetryFallbackManager


def setup_real_transport() -> None:
    """Wire up urllib.request as the real HTTP transport."""
    import json as _json
    import urllib.request

    class RealTransport:
        def __init__(self) -> None:
            self._opener = urllib.request.build_opener()

        def __call__(self, req: AnthropicProviderRequest, timeout: float | None = None) -> object:
            headers = {k: v for k, v in req.headers.items()}
            urllib_req = urllib.request.Request(
                req.url,
                data=req.data,
                headers=headers,
                method=req.method,
            )
            resp = self._opener.open(urllib_req, timeout=timeout)

            class Response:
                def __init__(self, resp: object) -> None:
                    self._resp = resp
                    self.status = resp.status  # type: ignore[attr-defined]

                def read(self) -> bytes:
                    return self._resp.read()  # type: ignore[union-attr]

                def __enter__(self) -> "Response":
                    return self

                def __exit__(self, *args: object) -> None:
                    pass

            return Response(resp)

    import harness_core.dispatch.provider.anthropic_provider as ap_mod
    ap_mod.anthropic_urlopen = RealTransport()  # type: ignore[misc]


def main() -> int:
    api_key = os.environ.get("ANTHROPIC_AUTH_TOKEN")
    if not api_key:
        print("ERROR: ANTHROPIC_AUTH_TOKEN environment variable not set.")
        print("Set it with: export ANTHROPIC_AUTH_TOKEN='your-key-here'")
        return 1

    print("Token-Efficient Agent Harness — Real Provider Demo")
    print(f"Provider: mimo (Anthropic-compatible)")
    print(f"Base URL: https://token-plan-cn.xiaomimimo.com/anthropic")
    print(f"API key:  {'*' * 8}{api_key[-4:]} (from env, not saved)")
    print()

    setup_real_transport()

    config = ProviderConfig(
        provider_id="mimo-v2.5",
        provider_type="anthropic",
        base_url="https://token-plan-cn.xiaomimimo.com/anthropic",
        model_id="mimo-v2.5",
        credential_ref="ANTHROPIC_AUTH_TOKEN",
        enabled=True,
        timeout_ms=60_000,
        input_cost_per_1k=0.003,
        output_cost_per_1k=0.015,
    )

    boundary = CredentialBoundary()
    cred_ref = CredentialRef(
        credential_ref_id="ANTHROPIC_AUTH_TOKEN",
        storage_backend="env",
        redacted_display=f"****{api_key[-4:]}",
        scope="api",
    )
    audit = ProviderAuditRecorder()
    provider = AnthropicProvider(config, boundary, cred_ref, audit_recorder=audit)

    engine = DispatchEngine(executor=provider)

    tasks = [
        "Explain what a dispatch kernel is in 2 sentences.",
        "Write a Python function that checks if a number is prime.",
        "What are the 3 main principles of event sourcing?",
    ]

    print("=" * 60)
    print("  Dispatching real tasks to mimo")
    print("=" * 60)

    bundles = []
    for i, task in enumerate(tasks, 1):
        print(f"\n--- Task {i} ---")
        print(f"Request: {task}")
        print()

        start = time.time()
        bundle = engine.dispatch(task, request_source="real-provider-demo")
        elapsed = time.time() - start
        bundles.append(bundle)

        exec_result = bundle.execution_result
        print(f"Status:    {exec_result.status}")
        print(f"Latency:   {exec_result.latency_ms}ms (wall: {elapsed:.2f}s)")
        print(f"Tokens:    in={exec_result.input_tokens}, out={exec_result.output_tokens}")
        if exec_result.estimated_cost is not None:
            print(f"Cost:      ${exec_result.estimated_cost:.6f}")
        if exec_result.error_domain:
            print(f"Error:     {exec_result.error_domain}: {exec_result.error_message}")
        print()
        print("Response:")
        output = exec_result.output or "(no output)"
        for line in output.splitlines()[:10]:
            print(f"  {line}")
        if len(output.splitlines()) > 10:
            print(f"  ... ({len(output.splitlines())} total lines)")

    print()
    print("=" * 60)
    print("  Summary")
    print("=" * 60)

    total_events = 0
    for b in bundles:
        dispatch_id = b.record.dispatch_id
        events = audit.list_events(dispatch_id)
        total_events += len(events)
        for event in events:
            print(f"  {event.event_type}: {event.provider_id} ({event.latency_ms or 0}ms)")
    print(f"Total audit events: {total_events}")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
