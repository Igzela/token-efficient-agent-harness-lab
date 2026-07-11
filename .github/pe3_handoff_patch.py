from pathlib import Path

script = Path("scripts/check_agent_handoff.py")
text = script.read_text()

structural = r'''

PACKET_HEADING_RE = re.compile(
    r"^### Packet (?P<packet>PE\d+-[A-Z0-9-]+)\b.*$", re.MULTILINE
)
PACKET_STATE_RE = re.compile(
    r"^\*\*State:\*\* `(?P<state>[A-Z_]+)`\s*$", re.MULTILINE
)
STAGE_ROW_RE = re.compile(
    r"^\|\s*(?P<stage>PE-\d+)\s*\|[^|]*\|[^|]*\|\s*(?P<summary>[^|]+?)\s*\|$",
    re.MULTILINE,
)
VALID_PACKET_STATES = {
    "READY_FOR_EXECUTION",
    "BLOCKED_PREREQUISITE",
    "DECISION_REQUIRED",
    "IN_PROGRESS",
    "COMPLETE",
}


def _section(text: str, heading: str) -> str:
    start = text.find(heading)
    if start < 0:
        return ""
    start += len(heading)
    end = text.find("\n## ", start)
    return text[start:] if end < 0 else text[start:end]


def _packet_stage(packet_id: str) -> str:
    match = re.match(r"PE(\d+)-", packet_id)
    return f"PE-{match.group(1)}" if match else ""


def parse_packet_contracts(
    text: str, failures: list[str]
) -> dict[str, dict[str, object]]:
    headings = list(PACKET_HEADING_RE.finditer(text))
    packets: dict[str, dict[str, object]] = {}
    for index, match in enumerate(headings):
        packet_id = match.group("packet")
        end = headings[index + 1].start() if index + 1 < len(headings) else len(text)
        block = text[match.start() : end]
        states = PACKET_STATE_RE.findall(block)
        if len(states) != 1:
            failures.append(
                f"{packet_id} must have exactly one structural State field; found {states}"
            )
            continue
        if packet_id in packets:
            failures.append(
                f"{packet_id} is represented more than once and may be simultaneously complete/in progress"
            )
            continue
        prerequisite_match = re.search(
            r"^\*\*Prerequisite:\*\* (?P<value>.+)$", block, re.MULTILINE
        )
        prerequisites = (
            re.findall(r"PE\d+-[A-Z0-9-]+", prerequisite_match.group("value"))
            if prerequisite_match
            else []
        )
        packets[packet_id] = {
            "state": states[0],
            "prerequisites": prerequisites,
        }
    return packets


def active_state_failures(status_text: str, next_text: str) -> list[str]:
    failures: list[str] = []
    packets = parse_packet_contracts(next_text, failures)

    for packet_id, packet in packets.items():
        state = str(packet["state"])
        if state not in VALID_PACKET_STATES:
            failures.append(f"{packet_id} has unknown state {state!r}")
        if state in {"READY_FOR_EXECUTION", "IN_PROGRESS"}:
            incomplete = [
                prerequisite
                for prerequisite in packet["prerequisites"]
                if prerequisite not in packets
                or packets[prerequisite]["state"] != "COMPLETE"
            ]
            if incomplete:
                failures.append(
                    f"{packet_id} is {state} while prerequisites are not complete: {incomplete}"
                )

    routing = _section(next_text, "## Active Routing")
    routed_packets = re.findall(r"PE\d+-[A-Z0-9-]+", routing)
    if not routed_packets:
        failures.append("Active Routing must name at least one packet")
    for packet_id in routed_packets:
        if packet_id not in packets:
            failures.append(f"Active Routing references unknown packet {packet_id}")
        elif packets[packet_id]["state"] == "COMPLETE":
            failures.append(f"Active Routing points to completed packet {packet_id}")
    if routed_packets and routed_packets[0] in packets:
        first = packets[routed_packets[0]]
        incomplete = [
            prerequisite
            for prerequisite in first["prerequisites"]
            if prerequisite not in packets
            or packets[prerequisite]["state"] != "COMPLETE"
        ]
        if incomplete:
            failures.append(
                f"next routed packet {routed_packets[0]} has incomplete prerequisites: {incomplete}"
            )

    next_stages = {
        match.group("stage"): match.group("summary").strip()
        for match in STAGE_ROW_RE.finditer(next_text)
    }
    status_stages = {
        match.group("stage"): match.group("summary").strip()
        for match in STAGE_ROW_RE.finditer(status_text)
    }
    packet_states: dict[str, list[str]] = {}
    for packet_id, packet in packets.items():
        packet_states.setdefault(_packet_stage(packet_id), []).append(str(packet["state"]))

    for stage, summary in next_stages.items():
        states = packet_states.get(stage, [])
        lowered = summary.lower()
        if "complete" in lowered and any(state != "COMPLETE" for state in states):
            failures.append(f"{stage} summary says complete while packet states are {states}")
        if "in progress" in lowered and "IN_PROGRESS" not in states:
            failures.append(f"{stage} summary says in progress but no packet is IN_PROGRESS")
        if "not started" in lowered and any(
            state in {"IN_PROGRESS", "COMPLETE"} for state in states
        ):
            failures.append(
                f"{stage} summary says not started while packet states are {states}"
            )

    for stage in sorted(set(next_stages) & set(status_stages)):
        next_complete = "complete" in next_stages[stage].lower()
        status_complete = "complete" in status_stages[stage].lower()
        if next_complete != status_complete:
            failures.append(
                f"{stage} completion summary contradicts between CURRENT_STATUS and NEXT_DECISION"
            )

    active_status = _section(status_text, "## Active Tracks") + _section(
        status_text, "## Current Gaps"
    )
    for stage, summary in status_stages.items():
        if "complete" not in summary.lower():
            continue
        if re.search(
            rf"{re.escape(stage)}[^\n]*(pending|next|in progress|not yet|has not)",
            active_status,
            re.IGNORECASE,
        ):
            failures.append(
                f"{stage} is complete in the stage table but still described as pending in active status"
            )

    return failures


def check_active_state_consistency(failures: list[str]) -> None:
    status_path = ROOT / "docs" / "CURRENT_STATUS.md"
    next_path = ROOT / "docs" / "NEXT_DECISION.md"
    if not status_path.exists() or not next_path.exists():
        return
    failures.extend(
        active_state_failures(
            status_path.read_text(encoding="utf-8"),
            next_path.read_text(encoding="utf-8"),
        )
    )
'''

if "def check_active_state_consistency" not in text:
    anchor = "\ndef main() -> int:\n"
    if anchor not in text:
        raise SystemExit("main anchor missing")
    text = text.replace(anchor, structural + anchor, 1)

call_anchor = "    check_schema_document_drift(failures)\n    check_phase_handoff(failures)\n"
call = call_anchor + "    check_active_state_consistency(failures)\n"
if call not in text:
    if call_anchor not in text:
        raise SystemExit("check call anchor missing")
    text = text.replace(call_anchor, call, 1)
script.write_text(text)

next_doc = Path("docs/NEXT_DECISION.md")
next_text = next_doc.read_text()
next_text = next_text.replace(
    "| PE-4 | P1/P2 | Trace-backed Policy Replay | Not started beyond merged contract text and the superseded #193 prototype; PE4-CONTRACT-REPAIR-1 is blocked on PE3-CLOSE-1 |",
    "| PE-4 | P1/P2 | Trace-backed Policy Replay | Contract repair blocked on PE3-CLOSE-1; no offline replay, read, shadow, canary, or promotion packet is accepted as started |",
)
next_doc.write_text(next_text)

tests = Path("tools/test_check_agent_handoff.py")
test_text = tests.read_text()
if "test_structural_guard_rejects_completed_active_route" not in test_text:
    insertion = r'''

    def test_structural_guard_rejects_completed_active_route(self) -> None:
        checker = load_handoff_checker()
        next_text = """| Stage | Priority | Goal | Status |
|---|---|---|---|
| PE-3 | P1 | x | Complete |
### Packet PE3-A-1 — a
**State:** `COMPLETE`
## Active Routing
1. Execute PE3-A-1.
"""
        failures = checker.active_state_failures(next_text, next_text)
        self.assertIn("Active Routing points to completed packet PE3-A-1", failures)

    def test_structural_guard_rejects_duplicate_packet_state(self) -> None:
        checker = load_handoff_checker()
        next_text = """### Packet PE3-A-1 — a
**State:** `COMPLETE`
**State:** `IN_PROGRESS`
## Active Routing
1. Execute PE3-A-1.
"""
        failures = checker.active_state_failures(next_text, next_text)
        self.assertTrue(
            any("exactly one structural State" in failure for failure in failures),
            failures,
        )

    def test_structural_guard_rejects_incomplete_prerequisite(self) -> None:
        checker = load_handoff_checker()
        next_text = """| Stage | Priority | Goal | Status |
|---|---|---|---|
| PE-3 | P1 | x | In progress |
### Packet PE3-A-1 — a
**State:** `IN_PROGRESS`
### Packet PE3-B-1 — b
**State:** `READY_FOR_EXECUTION`
**Prerequisite:** PE3-A-1 complete.
## Active Routing
1. Execute PE3-B-1.
"""
        failures = checker.active_state_failures(next_text, next_text)
        self.assertTrue(
            any("prerequisites are not complete" in failure for failure in failures),
            failures,
        )

    def test_structural_guard_rejects_stage_summary_without_owner(self) -> None:
        checker = load_handoff_checker()
        next_text = """| Stage | Priority | Goal | Status |
|---|---|---|---|
| PE-3 | P1 | x | In progress |
### Packet PE3-A-1 — a
**State:** `COMPLETE`
### Packet PE3-B-1 — b
**State:** `BLOCKED_PREREQUISITE`
**Prerequisite:** PE3-A-1 complete.
## Active Routing
1. Execute PE3-B-1.
"""
        failures = checker.active_state_failures(next_text, next_text)
        self.assertIn("PE-3 summary says in progress but no packet is IN_PROGRESS", failures)

    def test_structural_guard_accepts_consistent_packet_routing(self) -> None:
        checker = load_handoff_checker()
        text = """## Active Tracks
| Track | Status |
|---|---|
| x | active |
## Planned Product Evolution Stages
| Stage | Priority | Goal | Status |
|---|---|---|---|
| PE-3 | P1 | x | In progress |
### Packet PE3-A-1 — a
**State:** `COMPLETE`
### Packet PE3-B-1 — b
**State:** `IN_PROGRESS`
**Prerequisite:** PE3-A-1 complete.
## Active Routing
1. Execute PE3-B-1.
"""
        self.assertEqual(checker.active_state_failures(text, text), [])
'''
    anchor = "\n\nif __name__ == \"__main__\":\n"
    if anchor not in test_text:
        raise SystemExit("test anchor missing")
    test_text = test_text.replace(anchor, insertion + anchor, 1)
tests.write_text(test_text)
