"""Tests for dispatch/community_profiles.py — schema, CRUD, search, validation, thread safety."""

import sys
import threading
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

import unittest
from harness_core.dispatch.community_profiles import (
    COMMUNITY_PROFILE_SCHEMA_VERSION,
    ModelProfile,
    CommunityProfileRegistry,
    make_profile,
)


class SchemaVersionTests(unittest.TestCase):
    def test_schema_version(self):
        self.assertEqual(COMMUNITY_PROFILE_SCHEMA_VERSION, "community_profile.v1")

    def test_default_schema_version(self):
        p = make_profile()
        self.assertEqual(p.schema_version, COMMUNITY_PROFILE_SCHEMA_VERSION)


class DataclassTests(unittest.TestCase):
    def test_fields(self):
        p = make_profile(
            profile_id="p1", name="GPT-4", provider="openai",
            model_name="gpt-4", capabilities=("chat", "code"),
            cost_per_1k_tokens=0.03, max_context=8192,
            created_at=1000.0, author="alice",
            tags=("general", "coding"),
        )
        self.assertEqual(p.profile_id, "p1")
        self.assertEqual(p.name, "GPT-4")
        self.assertEqual(p.provider, "openai")
        self.assertEqual(p.model_name, "gpt-4")
        self.assertEqual(p.capabilities, ("chat", "code"))
        self.assertAlmostEqual(p.cost_per_1k_tokens, 0.03)
        self.assertEqual(p.max_context, 8192)
        self.assertEqual(p.created_at, 1000.0)
        self.assertEqual(p.author, "alice")
        self.assertEqual(p.tags, ("general", "coding"))

    def test_frozen(self):
        p = make_profile()
        with self.assertRaises(AttributeError):
            p.name = "changed"


class RegisterUnregisterTests(unittest.TestCase):
    def test_register_valid(self):
        reg = CommunityProfileRegistry()
        self.assertTrue(reg.register_profile(make_profile()))
        self.assertEqual(len(reg.list_profiles()), 1)

    def test_register_duplicate_rejected(self):
        reg = CommunityProfileRegistry()
        self.assertTrue(reg.register_profile(make_profile()))
        self.assertFalse(reg.register_profile(make_profile()))
        self.assertEqual(len(reg.list_profiles()), 1)

    def test_register_invalid_rejected(self):
        reg = CommunityProfileRegistry()
        self.assertFalse(reg.register_profile(make_profile(profile_id="")))
        self.assertEqual(len(reg.list_profiles()), 0)

    def test_unregister_existing(self):
        reg = CommunityProfileRegistry()
        reg.register_profile(make_profile())
        self.assertTrue(reg.unregister_profile("test-profile"))
        self.assertEqual(len(reg.list_profiles()), 0)

    def test_unregister_nonexistent(self):
        reg = CommunityProfileRegistry()
        self.assertFalse(reg.unregister_profile("nope"))

    def test_get_profile(self):
        reg = CommunityProfileRegistry()
        reg.register_profile(make_profile())
        self.assertIsNotNone(reg.get_profile("test-profile"))
        self.assertIsNone(reg.get_profile("nope"))

    def test_list_profiles(self):
        reg = CommunityProfileRegistry()
        reg.register_profile(make_profile(profile_id="a", name="A"))
        reg.register_profile(make_profile(profile_id="b", name="B"))
        profiles = reg.list_profiles()
        self.assertEqual(len(profiles), 2)
        ids = {p.profile_id for p in profiles}
        self.assertEqual(ids, {"a", "b"})

    def test_list_returns_copy(self):
        reg = CommunityProfileRegistry()
        reg.register_profile(make_profile())
        profiles = reg.list_profiles()
        profiles.clear()
        self.assertEqual(len(reg.list_profiles()), 1)


class SearchTests(unittest.TestCase):
    def test_search_by_provider(self):
        reg = CommunityProfileRegistry()
        reg.register_profile(make_profile(profile_id="p1", provider="openai"))
        reg.register_profile(make_profile(profile_id="p2", provider="anthropic"))
        results = reg.search_by_provider("openai")
        self.assertEqual(len(results), 1)
        self.assertEqual(results[0].profile_id, "p1")

    def test_search_by_provider_case_insensitive(self):
        reg = CommunityProfileRegistry()
        reg.register_profile(make_profile(profile_id="p1", provider="OpenAI"))
        results = reg.search_by_provider("openai")
        self.assertEqual(len(results), 1)

    def test_search_by_provider_no_results(self):
        reg = CommunityProfileRegistry()
        reg.register_profile(make_profile(provider="openai"))
        self.assertEqual(reg.search_by_provider("anthropic"), [])

    def test_search_by_tag(self):
        reg = CommunityProfileRegistry()
        reg.register_profile(make_profile(profile_id="p1", tags=("coding", "fast")))
        reg.register_profile(make_profile(profile_id="p2", tags=("slow",)))
        results = reg.search_by_tag("coding")
        self.assertEqual(len(results), 1)
        self.assertEqual(results[0].profile_id, "p1")

    def test_search_by_tag_case_insensitive(self):
        reg = CommunityProfileRegistry()
        reg.register_profile(make_profile(profile_id="p1", tags=("Coding",)))
        results = reg.search_by_tag("coding")
        self.assertEqual(len(results), 1)

    def test_search_by_tag_no_results(self):
        reg = CommunityProfileRegistry()
        reg.register_profile(make_profile(tags=("fast",)))
        self.assertEqual(reg.search_by_tag("slow"), [])

    def test_search_by_tag_multiple_matches(self):
        reg = CommunityProfileRegistry()
        reg.register_profile(make_profile(profile_id="p1", tags=("coding", "ai")))
        reg.register_profile(make_profile(profile_id="p2", tags=("ai",)))
        results = reg.search_by_tag("ai")
        self.assertEqual(len(results), 2)


class ValidationTests(unittest.TestCase):
    def test_valid_profile_no_errors(self):
        reg = CommunityProfileRegistry()
        errors = reg.validate_profile(make_profile())
        self.assertEqual(errors, [])

    def test_missing_profile_id(self):
        reg = CommunityProfileRegistry()
        errors = reg.validate_profile(make_profile(profile_id=""))
        self.assertTrue(any("profile_id" in e for e in errors))

    def test_missing_name(self):
        reg = CommunityProfileRegistry()
        errors = reg.validate_profile(make_profile(name=""))
        self.assertTrue(any("name" in e for e in errors))

    def test_missing_provider(self):
        reg = CommunityProfileRegistry()
        errors = reg.validate_profile(make_profile(provider=""))
        self.assertTrue(any("provider" in e for e in errors))

    def test_missing_model_name(self):
        reg = CommunityProfileRegistry()
        errors = reg.validate_profile(make_profile(model_name=""))
        self.assertTrue(any("model_name" in e for e in errors))

    def test_missing_author(self):
        reg = CommunityProfileRegistry()
        errors = reg.validate_profile(make_profile(author=""))
        self.assertTrue(any("author" in e for e in errors))

    def test_negative_cost(self):
        reg = CommunityProfileRegistry()
        errors = reg.validate_profile(make_profile(cost_per_1k_tokens=-0.01))
        self.assertTrue(any("cost" in e for e in errors))

    def test_zero_context(self):
        reg = CommunityProfileRegistry()
        errors = reg.validate_profile(make_profile(max_context=0))
        self.assertTrue(any("max_context" in e for e in errors))

    def test_negative_context(self):
        reg = CommunityProfileRegistry()
        errors = reg.validate_profile(make_profile(max_context=-100))
        self.assertTrue(any("max_context" in e for e in errors))

    def test_invalid_schema_version(self):
        reg = CommunityProfileRegistry()
        errors = reg.validate_profile(make_profile(schema_version="wrong.v1"))
        self.assertTrue(any("schema_version" in e for e in errors))

    def test_zero_cost_valid(self):
        reg = CommunityProfileRegistry()
        errors = reg.validate_profile(make_profile(cost_per_1k_tokens=0.0))
        self.assertEqual(errors, [])


class ThreadSafetyTests(unittest.TestCase):
    def test_concurrent_register_and_search(self):
        reg = CommunityProfileRegistry()
        errors = []

        def register_many(start: int, count: int) -> None:
            try:
                for i in range(start, start + count):
                    profile = make_profile(
                        profile_id=f"p-{i}",
                        name=f"Profile {i}",
                        provider="openai" if i % 2 == 0 else "anthropic",
                        tags=("tag-a",) if i % 3 == 0 else ("tag-b",),
                    )
                    reg.register_profile(profile)
            except Exception as e:
                errors.append(str(e))

        def search_loop() -> None:
            try:
                for _ in range(50):
                    reg.search_by_provider("openai")
                    reg.search_by_tag("tag-a")
                    reg.list_profiles()
            except Exception as e:
                errors.append(str(e))

        threads = [
            threading.Thread(target=register_many, args=(0, 50)),
            threading.Thread(target=register_many, args=(50, 50)),
            threading.Thread(target=search_loop),
            threading.Thread(target=search_loop),
        ]
        for t in threads:
            t.start()
        for t in threads:
            t.join()

        self.assertEqual(errors, [])
        profiles = reg.list_profiles()
        self.assertEqual(len(profiles), 100)


if __name__ == "__main__":
    unittest.main()
