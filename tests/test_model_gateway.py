import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

from harness_core.model_gateway import (
    ModelCapability,
    ModelCapabilityRegistry,
    ModelGateway,
    ModelGatewayUnknownTier,
    ModelResponse,
    ModelTier,
    StubModelProvider,
    create_default_gateway,
    create_default_registry,
)


class ModelCapabilityRegistryTests(unittest.TestCase):
    def test_default_tiers_exist(self):
        registry = create_default_registry()
        tiers = registry.list_tiers()
        self.assertIn("strong_planner", tiers)
        self.assertIn("cheap_executor", tiers)
        self.assertIn("verifier", tiers)
        self.assertIn("advisor", tiers)

    def test_get_tier(self):
        registry = create_default_registry()
        tier = registry.get_tier("strong_planner")
        self.assertEqual("strong_planner", tier.name)
        self.assertEqual("stub", tier.provider)

    def test_get_capability(self):
        registry = create_default_registry()
        cap = registry.get_capability("strong_planner")
        self.assertTrue(cap.supports_tools)
        self.assertTrue(cap.supports_thinking)

    def test_unknown_tier_raises(self):
        registry = create_default_registry()
        with self.assertRaises(ModelGatewayUnknownTier):
            registry.get_tier("nonexistent")

    def test_unknown_capability_raises(self):
        registry = create_default_registry()
        with self.assertRaises(ModelGatewayUnknownTier):
            registry.get_capability("nonexistent")

    def test_register_custom_tier(self):
        registry = create_default_registry()
        tier = ModelTier(
            name="custom",
            provider="stub",
            model_id="stub-custom",
            max_tokens=512,
            cost_per_1k_tokens=0.005,
        )
        cap = ModelCapability(
            tier="custom",
            supports_tools=False,
            supports_thinking=False,
            supports_caching=False,
            max_context_tokens=10000,
            cost_per_1k_tokens=0.005,
        )
        registry.register(tier, cap)
        self.assertIn("custom", registry.list_tiers())
        self.assertEqual("custom", registry.get_tier("custom").name)

    def test_list_tiers_sorted(self):
        registry = create_default_registry()
        tiers = registry.list_tiers()
        self.assertEqual(tiers, tuple(sorted(tiers)))


class StubModelProviderTests(unittest.TestCase):
    def test_stub_response_deterministic(self):
        provider = StubModelProvider()
        tier = ModelTier(
            name="strong_planner",
            provider="stub",
            model_id="stub-planner",
            max_tokens=4096,
            cost_per_1k_tokens=0.015,
        )
        r1 = provider.invoke(tier, "test prompt", 4096)
        r2 = provider.invoke(tier, "test prompt", 4096)
        self.assertEqual(r1, r2)

    def test_prompt_variation_changes_content(self):
        provider = StubModelProvider()
        tier = ModelTier(
            name="strong_planner",
            provider="stub",
            model_id="stub-planner",
            max_tokens=4096,
            cost_per_1k_tokens=0.015,
        )
        r1 = provider.invoke(tier, "prompt A", 4096)
        r2 = provider.invoke(tier, "prompt B", 4096)
        self.assertNotEqual(r1.content, r2.content)

    def test_token_usage_within_budget(self):
        provider = StubModelProvider()
        tier = ModelTier(
            name="cheap_executor",
            provider="stub",
            model_id="stub-executor",
            max_tokens=2048,
            cost_per_1k_tokens=0.001,
        )
        for i in range(10):
            resp = provider.invoke(tier, f"prompt {i}", 100)
            self.assertLessEqual(resp.token_usage, 100)
            self.assertGreater(resp.token_usage, 0)

    def test_provider_is_stub(self):
        provider = StubModelProvider()
        tier = ModelTier(
            name="verifier",
            provider="stub",
            model_id="stub-verifier",
            max_tokens=1024,
            cost_per_1k_tokens=0.003,
        )
        resp = provider.invoke(tier, "check this", 1024)
        self.assertEqual("stub", resp.provider)

    def test_latency_deterministic(self):
        provider = StubModelProvider()
        tier = ModelTier(
            name="advisor",
            provider="stub",
            model_id="stub-advisor",
            max_tokens=2048,
            cost_per_1k_tokens=0.01,
        )
        r1 = provider.invoke(tier, "same prompt", 2048)
        r2 = provider.invoke(tier, "same prompt", 2048)
        self.assertEqual(r1.latency_ms, r2.latency_ms)


class ModelGatewayTests(unittest.TestCase):
    def test_invoke_default_tier(self):
        gateway = create_default_gateway()
        resp = gateway.invoke("strong_planner", "plan this task")
        self.assertEqual("strong_planner", resp.tier)
        self.assertEqual("stub", resp.provider)
        self.assertIn("plan", resp.content.lower())

    def test_invoke_all_default_tiers(self):
        gateway = create_default_gateway()
        for tier_name in ("strong_planner", "cheap_executor", "verifier", "advisor"):
            resp = gateway.invoke(tier_name, "test prompt")
            self.assertEqual(tier_name, resp.tier)

    def test_unknown_tier_raises(self):
        gateway = create_default_gateway()
        with self.assertRaises(ModelGatewayUnknownTier):
            gateway.invoke("nonexistent", "test")

    def test_custom_max_tokens(self):
        gateway = create_default_gateway()
        resp = gateway.invoke("cheap_executor", "test", max_tokens=50)
        self.assertLessEqual(resp.token_usage, 50)

    def test_deterministic_across_invocations(self):
        gateway = create_default_gateway()
        r1 = gateway.invoke("verifier", "verify this")
        r2 = gateway.invoke("verifier", "verify this")
        self.assertEqual(r1, r2)

    def test_registry_accessible(self):
        gateway = create_default_gateway()
        self.assertIsInstance(gateway.registry, ModelCapabilityRegistry)
        self.assertIn("strong_planner", gateway.registry.list_tiers())


if __name__ == "__main__":
    unittest.main()
