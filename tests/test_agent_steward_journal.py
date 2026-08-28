"""Journal contract tests for the provider-free Steward."""

from __future__ import annotations

import json
from pathlib import Path
import sqlite3
import sys
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts" / "agent-control"))

from steward_journal import (  # noqa: E402
    IdempotencyConflict,
    JournalCorrupt,
    StewardJournal,
    TransitionRejected,
    transition_allowed,
)


MISSION = "AUTONOMOUS-STEWARD-MIGRATION-2026-08-27"
STAGE = "stage-1"
CARD = "card-1"


class StewardJournalTests(unittest.TestCase):
    def make_journal(self):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        return StewardJournal(Path(temp.name) / "steward.sqlite3")

    def append(self, journal, **overrides):
        values = {
            "event": "CARD_QUEUED",
            "idempotency_key": "queue:card-1:1",
            "mission_id": MISSION,
            "stage_id": STAGE,
            "card_id": CARD,
            "attempt": 1,
            "state": "QUEUED",
            "detail": "bounded",
        }
        values.update(overrides)
        return journal.append(**values)

    def test_append_is_hash_chained_and_projection_is_rebuildable(self):
        journal = self.make_journal()
        first = self.append(journal)
        second = self.append(
            journal,
            event="WORKER_STARTED",
            idempotency_key="start:card-1:1",
            state="RUNNING",
        )
        self.assertEqual((first.seq, second.seq), (1, 2))
        self.assertEqual(second.prev_sha256, first.sha256)
        self.assertEqual(journal.projection()["card_states"], {CARD: "RUNNING"})
        self.assertEqual(journal.projection()["active_cards"], [CARD])
        self.assertEqual(len(journal.replay()), 2)

    def test_idempotent_retry_returns_original_record(self):
        journal = self.make_journal()
        first = self.append(journal)
        duplicate = self.append(journal)
        self.assertEqual(duplicate, first)
        self.assertEqual(journal.projection()["event_count"], 1)

    def test_idempotency_key_cannot_change_transition_facts(self):
        journal = self.make_journal()
        self.append(journal)
        with self.assertRaises(IdempotencyConflict):
            self.append(journal, detail="different")

    def test_invalid_transition_is_rejected_against_live_tail(self):
        journal = self.make_journal()
        self.append(journal)
        with self.assertRaises(TransitionRejected):
            self.append(
                journal,
                event="CARD_COMPLETE",
                idempotency_key="complete:card-1:1",
                state="COMPLETE",
            )

    def test_heartbeat_is_global_and_idempotent(self):
        journal = self.make_journal()
        first = journal.heartbeat(mission_id=MISSION, idempotency_key="heartbeat:1")
        second = journal.heartbeat(mission_id=MISSION, idempotency_key="heartbeat:1")
        self.assertEqual(first, second)
        projection = journal.projection()
        self.assertEqual(projection["last_heartbeat"], first.timestamp)
        self.assertEqual(projection["card_states"], {})

    def test_corrupt_record_is_refused_not_repaired(self):
        journal = self.make_journal()
        self.append(journal)
        connection = sqlite3.connect(journal.path)
        row = connection.execute(
            "SELECT record_json FROM steward_journal_events WHERE seq = 1"
        ).fetchone()
        record = json.loads(row[0])
        record["detail"] = "tampered"
        connection.execute(
            "UPDATE steward_journal_events SET record_json = ? WHERE seq = 1",
            (json.dumps(record),),
        )
        connection.commit()
        connection.close()
        with self.assertRaises(JournalCorrupt):
            journal.replay()
        with self.assertRaises(JournalCorrupt):
            self.append(journal, idempotency_key="queue:card-1:2")

    def test_duplicate_and_unknown_state_transitions_are_not_allowed(self):
        self.assertTrue(transition_allowed(None, "QUEUED"))
        self.assertTrue(transition_allowed("RUNNING", "VERIFYING"))
        self.assertFalse(transition_allowed("OUTCOME_UNKNOWN", "RUNNING"))
        self.assertFalse(transition_allowed("COMPLETE", "RUNNING"))

    def test_journal_does_not_accept_multiline_or_oversized_details(self):
        journal = self.make_journal()
        with self.assertRaisesRegex(Exception, "journal_detail_invalid"):
            self.append(journal, detail="bad\nrecord")
        with self.assertRaisesRegex(Exception, "journal_detail_invalid"):
            self.append(journal, detail="x" * 513)


if __name__ == "__main__":
    unittest.main()
