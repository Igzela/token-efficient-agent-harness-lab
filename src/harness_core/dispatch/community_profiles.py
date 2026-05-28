"""Phase 7: CommunityProfileRegistry — register, validate, and search model profiles."""

from __future__ import annotations

import threading
import time
from dataclasses import dataclass

COMMUNITY_PROFILE_SCHEMA_VERSION = "community_profile.v1"


@dataclass(frozen=True)
class ModelProfile:
    profile_id: str
    name: str
    provider: str
    model_name: str
    capabilities: tuple[str, ...]
    cost_per_1k_tokens: float
    max_context: int
    created_at: float
    author: str
    tags: tuple[str, ...]
    schema_version: str = COMMUNITY_PROFILE_SCHEMA_VERSION


class CommunityProfileRegistry:
    """In-memory registry for community model profiles with validation and search."""

    def __init__(self) -> None:
        self._registered: dict[str, ModelProfile] = {}
        self._lock = threading.Lock()

    def register_profile(self, profile: ModelProfile) -> bool:
        errors = self.validate_profile(profile)
        if errors:
            return False
        with self._lock:
            if profile.profile_id in self._registered:
                return False
            self._registered[profile.profile_id] = profile
            return True

    def unregister_profile(self, profile_id: str) -> bool:
        with self._lock:
            if profile_id in self._registered:
                del self._registered[profile_id]
                return True
            return False

    def get_profile(self, profile_id: str) -> ModelProfile | None:
        with self._lock:
            return self._registered.get(profile_id)

    def list_profiles(self) -> list[ModelProfile]:
        with self._lock:
            return list(self._registered.values())

    def search_by_provider(self, provider: str) -> list[ModelProfile]:
        provider_lower = provider.lower()
        with self._lock:
            return [p for p in self._registered.values() if p.provider.lower() == provider_lower]

    def search_by_tag(self, tag: str) -> list[ModelProfile]:
        tag_lower = tag.lower()
        with self._lock:
            return [p for p in self._registered.values() if tag_lower in [t.lower() for t in p.tags]]

    def validate_profile(self, profile: ModelProfile) -> list[str]:
        errors: list[str] = []

        if not profile.profile_id:
            errors.append("profile_id is required")
        if not profile.name:
            errors.append("name is required")
        if not profile.provider:
            errors.append("provider is required")
        if not profile.model_name:
            errors.append("model_name is required")
        if not profile.author:
            errors.append("author is required")
        if profile.cost_per_1k_tokens < 0:
            errors.append("cost_per_1k_tokens must be non-negative")
        if profile.max_context <= 0:
            errors.append("max_context must be positive")
        if profile.schema_version != COMMUNITY_PROFILE_SCHEMA_VERSION:
            errors.append(f"invalid schema_version: '{profile.schema_version}'")

        return errors


def make_profile(**kwargs) -> ModelProfile:
    defaults = dict(
        profile_id="test-profile",
        name="Test Profile",
        provider="openai",
        model_name="gpt-4",
        capabilities=("chat", "code"),
        cost_per_1k_tokens=0.03,
        max_context=8192,
        created_at=time.time(),
        author="test_author",
        tags=("general", "coding"),
    )
    defaults.update(kwargs)
    return ModelProfile(**defaults)
