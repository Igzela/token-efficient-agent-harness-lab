from pathlib import Path

next_path = Path("docs/NEXT_DECISION.md")
next_text = next_path.read_text()
next_replacements = {
    "| PE-2 | P0/P1 | Budget Intelligence and Anomaly Auto-Pause | Active; forecast packet ready |": "| PE-2 | P0/P1 | Budget Intelligence and Anomaly Auto-Pause | Active; anomaly packet ready |",
    "### Packet PE2-FORECAST-1 — Deterministic budget forecasts\n\n**State:** `READY_FOR_TERRA`": "### Packet PE2-FORECAST-1 — Deterministic budget forecasts\n\n**State:** `COMPLETE`",
    "### Packet PE2-ANOMALY-1 — Explainable anomaly detector\n\n**State:** `BLOCKED_PREREQUISITE`": "### Packet PE2-ANOMALY-1 — Explainable anomaly detector\n\n**State:** `READY_FOR_TERRA`",
    "1. Execute PE2-FORECAST-1 from latest `main`.": "1. Execute PE2-ANOMALY-1 from latest `main`.",
}
for before, after in next_replacements.items():
    if before not in next_text:
        raise SystemExit(f"missing NEXT_DECISION anchor: {before}")
    next_text = next_text.replace(before, after, 1)
next_path.write_text(next_text)

status_path = Path("docs/CURRENT_STATUS.md")
status_text = status_path.read_text()
status_replacements = {
    "| Post-LGB Product Evolution Plan | PE-1 is complete; PE2-CONTRACT-1 is implemented; PE2-FORECAST-1 is the next eligible packet |": "| Post-LGB Product Evolution Plan | PE-1 is complete; PE2-CONTRACT-1 and PE2-FORECAST-1 are implemented; PE2-ANOMALY-1 is the next eligible packet |",
    "| PE-2 | P0/P1 | Budget Intelligence and Anomaly Auto-Pause | In progress: evidence contract implemented; deterministic forecast packet next |": "| PE-2 | P0/P1 | Budget Intelligence and Anomaly Auto-Pause | In progress: evidence contract and deterministic forecasts implemented; anomaly packet next |",
    "- PE-2 deterministic forecast computation, explainable anomaly detection, read surfaces, and policy-gated high-confidence auto-pause are not implemented.": "- PE-2 explainable anomaly detection, read surfaces, and policy-gated high-confidence auto-pause are not implemented.",
}
for before, after in status_replacements.items():
    if before not in status_text:
        raise SystemExit(f"missing CURRENT_STATUS anchor: {before}")
    status_text = status_text.replace(before, after, 1)

contract_anchor = "- no persistence, API, SDK, Dashboard, provider, reservation, policy, pause, or target-output behavior changes are part of the contract packet.\n\n"
forecast_section = """## PE-2 Forecast Evidence

- deterministic forecasts use only bounded posted observations and explicit half-open evidence windows;
- observed token/cost totals remain separate from linear horizon estimates and exhaustion time;
- sparse, stale, mixed required dimensions, missing dimensions, excessive duplicates, conflicting duplicates, and incomplete pricing return explicit bounded outcomes;
- provider audit adaptation does not invent run, workspace, model, content hash, or non-USD pricing facts;
- focused tests cover zero usage, bursty usage, mixed workloads, boundary time, deterministic ordering, duplicate reconciliation, conflicting evidence, and concurrent reads;
- no persistence, API, SDK, Dashboard, budget/reservation mutation, policy, pause, or target-output behavior changes are part of the forecast packet.

"""
if contract_anchor not in status_text:
    raise SystemExit("missing forecast section insertion anchor")
status_text = status_text.replace(contract_anchor, contract_anchor + forecast_section, 1)
status_path.write_text(status_text)
