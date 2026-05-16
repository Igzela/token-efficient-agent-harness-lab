"""Model Gateway Stub for Stage 3 — tier-based model routing abstraction."""

from __future__ import annotations

import hashlib
from dataclasses import dataclass
from typing import Any, Protocol


@dataclass(frozen=True)
class ModelTier:
    name: str  # e.g., "strong_planner", "cheap_executor", "verifier"
    provider: str  # "stub" | "anthropic" | "openai" | etc.
    model_id: str  # e.g., "claude-opus-4-7", "gpt-4o", "stub"
    max_tokens: int
    cost_per_1k_tokens: float


@dataclass(frozen=True)
class ModelResponse:
    tier: str
    model_id: str
    content: str
    token_usage: int
    provider: str
    latency_ms: int
    raw_response: dict[str, Any] | None = None


@dataclass(frozen=True)
class ModelCapability:
    tier: str
    supports_tools: bool
    supports_thinking: bool
    supports_caching: bool
    max_context_tokens: int
    cost_per_1k_tokens: float


class ModelProvider(Protocol):
    def invoke(
        self, tier: ModelTier, prompt: str, max_tokens: int
    ) -> ModelResponse: ...


class ModelGatewayUnknownTier(Exception):
    """Raised when an unknown tier is requested."""

    def __init__(self, tier: str, available: tuple[str, ...]):
        super().__init__(f"unknown tier: {tier!r}; available: {available}")
        self.tier = tier
        self.available = available


class StubModelProvider:
    """Deterministic fake provider. Returns fixed content based on prompt hash.
    Never calls external APIs. Always succeeds."""

    def invoke(
        self, tier: ModelTier, prompt: str, max_tokens: int
    ) -> ModelResponse:
        prompt_hash = hashlib.sha256(prompt.encode("utf-8")).hexdigest()[:8]
        seed = int(prompt_hash, 16)

        # Deterministic content based on tier and prompt hash
        content_templates = {
            "strong_planner": f"[plan:{prompt_hash}] Detailed plan for task with {len(prompt)} chars of context.",
            "cheap_executor": f"[exec:{prompt_hash}] Simple execution output.",
            "verifier": f"[verify:{prompt_hash}] Verification result: pass.",
            "advisor": f"[advise:{prompt_hash}] Advisory guidance for task.",
        }
        content = content_templates.get(
            tier.name, f"[{tier.name}:{prompt_hash}] Generic output."
        )

        # Deterministic token usage: fraction of max_tokens based on seed
        usage_ratio = 0.1 + (seed % 50) / 100.0  # 0.10 to 0.59
        token_usage = min(int(max_tokens * usage_ratio), max_tokens)
        token_usage = max(1, token_usage)

        # Deterministic latency
        latency_ms = 10 + (seed % 90)

        return ModelResponse(
            tier=tier.name,
            model_id=tier.model_id,
            content=content,
            token_usage=token_usage,
            provider="stub",
            latency_ms=latency_ms,
        )


_DEFAULT_TIERS: dict[str, tuple[ModelTier, ModelCapability]] = {}


def _init_defaults() -> dict[str, tuple[ModelTier, ModelCapability]]:
    defaults = {
        "strong_planner": (
            ModelTier(
                name="strong_planner",
                provider="stub",
                model_id="stub-planner",
                max_tokens=4096,
                cost_per_1k_tokens=0.015,
            ),
            ModelCapability(
                tier="strong_planner",
                supports_tools=True,
                supports_thinking=True,
                supports_caching=True,
                max_context_tokens=200000,
                cost_per_1k_tokens=0.015,
            ),
        ),
        "cheap_executor": (
            ModelTier(
                name="cheap_executor",
                provider="stub",
                model_id="stub-executor",
                max_tokens=2048,
                cost_per_1k_tokens=0.001,
            ),
            ModelCapability(
                tier="cheap_executor",
                supports_tools=True,
                supports_thinking=False,
                supports_caching=True,
                max_context_tokens=100000,
                cost_per_1k_tokens=0.001,
            ),
        ),
        "verifier": (
            ModelTier(
                name="verifier",
                provider="stub",
                model_id="stub-verifier",
                max_tokens=1024,
                cost_per_1k_tokens=0.003,
            ),
            ModelCapability(
                tier="verifier",
                supports_tools=False,
                supports_thinking=False,
                supports_caching=True,
                max_context_tokens=50000,
                cost_per_1k_tokens=0.003,
            ),
        ),
        "advisor": (
            ModelTier(
                name="advisor",
                provider="stub",
                model_id="stub-advisor",
                max_tokens=2048,
                cost_per_1k_tokens=0.01,
            ),
            ModelCapability(
                tier="advisor",
                supports_tools=False,
                supports_thinking=True,
                supports_caching=False,
                max_context_tokens=100000,
                cost_per_1k_tokens=0.01,
            ),
        ),
    }
    return defaults


class ModelCapabilityRegistry:
    """Register and look up model tiers and capabilities."""

    def __init__(self) -> None:
        self._tiers: dict[str, ModelTier] = {}
        self._capabilities: dict[str, ModelCapability] = {}

    def register(self, tier: ModelTier, capability: ModelCapability) -> None:
        self._tiers[tier.name] = tier
        self._capabilities[tier.name] = capability

    def get_tier(self, name: str) -> ModelTier:
        if name not in self._tiers:
            raise ModelGatewayUnknownTier(name, tuple(self._tiers.keys()))
        return self._tiers[name]

    def get_capability(self, name: str) -> ModelCapability:
        if name not in self._capabilities:
            raise ModelGatewayUnknownTier(name, tuple(self._capabilities.keys()))
        return self._capabilities[name]

    def list_tiers(self) -> tuple[str, ...]:
        return tuple(sorted(self._tiers.keys()))


class ModelGateway:
    """Route model calls to the correct provider by tier."""

    def __init__(
        self,
        registry: ModelCapabilityRegistry,
        provider: ModelProvider | None = None,
    ):
        self._registry = registry
        self._provider = provider or StubModelProvider()

    @property
    def registry(self) -> ModelCapabilityRegistry:
        return self._registry

    def invoke(
        self, tier: str, prompt: str, max_tokens: int = 4096
    ) -> ModelResponse:
        model_tier = self._registry.get_tier(tier)
        return self._provider.invoke(model_tier, prompt, max_tokens)


def create_default_registry() -> ModelCapabilityRegistry:
    """Create a registry pre-populated with default stub tiers."""
    registry = ModelCapabilityRegistry()
    for tier, capability in _init_defaults().values():
        registry.register(tier, capability)
    return registry


def create_default_gateway() -> ModelGateway:
    """Create a gateway with default stub tiers and stub provider."""
    return ModelGateway(create_default_registry(), StubModelProvider())
