from pathlib import Path


def replace_once(path: Path, before: str, after: str) -> None:
    text = path.read_text()
    if before not in text:
        raise SystemExit(f"missing active-doc anchor in {path}:\n{before[:200]}")
    path.write_text(text.replace(before, after, 1))


next_decision = Path("docs/NEXT_DECISION.md")
replace_once(
    next_decision,
    "| PE-2 | P0/P1 | Budget Intelligence and Anomaly Auto-Pause | Active; anomaly packet ready |",
    "| PE-2 | P0/P1 | Budget Intelligence and Anomaly Auto-Pause | Active; anomaly packet complete, read surfaces ready |",
)
replace_once(
    next_decision,
    "### Packet PE2-ANOMALY-1 — Explainable anomaly detector\n\n**State:** `READY_FOR_TERRA`",
    "### Packet PE2-ANOMALY-1 — Explainable anomaly detector\n\n**State:** `COMPLETE`",
)
replace_once(
    next_decision,
    "**Acceptance:** Normal, spike, gradual drift, mixed workloads, sparse history, false-positive boundaries, duplicated evidence, out-of-order evidence, deterministic recomputation, and `insufficient_evidence` tests.\n\n### Packet PE2-READ-1",
    "**Acceptance:** Normal, spike, gradual drift, mixed workloads, sparse history, false-positive boundaries, duplicated evidence, out-of-order evidence, deterministic recomputation, exact coverage metadata, invalid-evidence preservation, and `insufficient_evidence` tests.\n\n### Packet PE2-READ-1",
)
replace_once(
    next_decision,
    "### Packet PE2-READ-1 — Persistence, API, SDK, and Dashboard read surfaces\n\n**State:** `BLOCKED_PREREQUISITE`",
    "### Packet PE2-READ-1 — Persistence, API, SDK, and Dashboard read surfaces\n\n**State:** `READY_FOR_TERRA`",
)
replace_once(
    next_decision,
    "## Active Routing\n\n1. Execute PE2-ANOMALY-1 from latest `main`.\n2. Merge only after focused validation, full CI, architecture/authority review, and no unresolved objection.\n3. Refresh `main`, re-read active docs/code, and continue PE2-FORECAST-1, PE2-ANOMALY-1, PE2-READ-1, PE2-PAUSE-1, then PE2-CLOSE-1.\n4. After PE-2 closeout, mark PE-3 next but do not start it in the PE-1-to-PE-2 effort.",
    "## Active Routing\n\n1. Execute PE2-READ-1 from latest `main`.\n2. Merge only after focused validation, full CI, architecture/authority review, and no unresolved objection.\n3. Refresh `main`, re-read active docs/code, and continue PE2-PAUSE-1, then PE2-CLOSE-1.\n4. After PE-2 closeout, mark PE-3 next but do not start it in the PE-1-to-PE-2 effort.",
)

current_status = Path("docs/CURRENT_STATUS.md")
replace_once(
    current_status,
    "| Post-LGB Product Evolution Plan | PE-1 is complete; PE2-CONTRACT-1 and PE2-FORECAST-1 are implemented; PE2-ANOMALY-1 is the next eligible packet |",
    "| Post-LGB Product Evolution Plan | PE-1 is complete; PE2-CONTRACT-1, PE2-FORECAST-1, and PE2-ANOMALY-1 are implemented; PE2-READ-1 is the next eligible packet |",
)
replace_once(
    current_status,
    "| PE-2 | P0/P1 | Budget Intelligence and Anomaly Auto-Pause | In progress: evidence contract and deterministic forecasts implemented; anomaly packet next |",
    "| PE-2 | P0/P1 | Budget Intelligence and Anomaly Auto-Pause | In progress: evidence contract, deterministic forecasts, and explainable anomaly detection implemented; read surfaces next |",
)
replace_once(
    current_status,
    "- no persistence, API, SDK, Dashboard, budget/reservation mutation, policy, pause, or target-output behavior changes are part of the forecast packet.\n\n## Current Gaps",
    "- no persistence, API, SDK, Dashboard, budget/reservation mutation, policy, pause, or target-output behavior changes are part of the forecast packet.\n\n## PE-2 Anomaly Evidence\n\n- deterministic rules cover cost, token, retry, latency, context-growth, and model-mix findings over explicit equal-duration windows;\n- supported normal evidence remains `detected=false`, while sparse, stale, mixed, incomplete, or excessive-duplicate evidence remains explicitly insufficient;\n- coverage metadata derives `missing_fields` only from fields absent in filtered evidence and preserves observed dimensions on applicable invalid-evidence paths;\n- conflicting duplicates and malformed metric evidence fail closed as versioned `invalid_evidence` findings with bounded references and reason codes;\n- thresholds are explicit and deterministic; equality does not create a false positive and no adaptive or opaque score is introduced;\n- no persistence, API, SDK, Dashboard, provider substitution, budget/reservation mutation, policy, pause, termination, or target-output behavior changes are part of the anomaly packet.\n\n## Current Gaps",
)
replace_once(
    current_status,
    "- PE-2 explainable anomaly detection, read surfaces, and policy-gated high-confidence auto-pause are not implemented.",
    "- PE-2 persistence/read surfaces and policy-gated high-confidence auto-pause are not implemented.",
)
