# Model Gateway Stub Spec

## Purpose

Abstract model provider calls behind a uniform interface. Start with a deterministic fake provider for testing. Future extension point for real models.

## Data Structures

### ModelTier

```python
@dataclass(frozen=True)
class ModelTier:
    name: str  # e.g., "strong_planner", "cheap_executor", "verifier"
    provider: str  # "stub" | "anthropic" | "openai" | etc.
    model_id: str  # e.g., "claude-opus-4-7", "gpt-4o", "stub"
    max_tokens: int
    cost_per_1k_tokens: float
```

### ModelResponse

```python
@dataclass(frozen=True)
class ModelResponse:
    tier: str
    model_id: str
    content: str
    token_usage: int
    provider: str
    latency_ms: int
    raw_response: dict[str, Any] | None = None
```

### ModelCapability

```python
@dataclass(frozen=True)
class ModelCapability:
    tier: str
    supports_tools: bool
    supports_thinking: bool
    supports_caching: bool
    max_context_tokens: int
    cost_per_1k_tokens: float
```

## APIs

### ModelProvider Protocol

```python
class ModelProvider(Protocol):
    def invoke(self, tier: ModelTier, prompt: str, max_tokens: int) -> ModelResponse
```

### StubModelProvider

Returns deterministic content based on prompt hash. Never calls external APIs. Always succeeds.

### ModelCapabilityRegistry

```python
class ModelCapabilityRegistry:
    def register(tier: ModelTier, capability: ModelCapability)
    def get_tier(name: str) -> ModelTier
    def get_capability(name: str) -> ModelCapability
    def list_tiers() -> tuple[str, ...]
```

### ModelGateway

```python
class ModelGateway:
    def __init__(self, registry: ModelCapabilityRegistry)
    def invoke(tier: str, prompt: str, max_tokens: int = 4096) -> ModelResponse
```

## Default Tiers

| Tier | Purpose | Stub Behavior |
|------|---------|---------------|
| `strong_planner` | Project-level architecture, complex reasoning | Returns structured plan stub |
| `cheap_executor` | Simple code changes, documentation | Returns simple output stub |
| `verifier` | Test validation, schema checking | Returns pass/fail stub |
| `advisor` | Short correction, risk assessment | Returns advisor-style stub |

## Failure Behavior

- Unknown tier raises `ValueError`
- Budget exceeded returns a structured error
- Stub provider always succeeds with deterministic output
- Token usage is deterministic and <= max_tokens

## Future Extension

When real models are allowed:
1. Implement `AnthropicProvider(ModelProvider)`
2. Implement `OpenAIProvider(ModelProvider)`
3. Register in `ModelCapabilityRegistry`
4. `ModelGateway` routes to correct provider based on tier

## Dependencies

None directly. Consumed by Advisor Broker and Routing Experiment Manager.
