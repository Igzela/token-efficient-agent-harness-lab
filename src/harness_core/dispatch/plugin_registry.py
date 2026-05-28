"""Phase 7: PluginRegistry — register, discover, validate, and search plugins."""

from __future__ import annotations

import json
import threading
from pathlib import Path

from .plugin_system import (
    ALL_KNOWN_PERMISSIONS,
    VALID_TRUST_LEVELS,
    PluginManifest,
    _parse_manifest,
)


PLUGIN_REGISTRY_SCHEMA_VERSION = "plugin_registry.v1"


class PluginRegistry:
    """In-memory registry for plugin manifests with discovery and validation."""

    def __init__(self) -> None:
        self._registered: dict[str, PluginManifest] = {}
        self._lock = threading.Lock()

    def register_plugin(self, manifest: PluginManifest) -> bool:
        errors = self.validate_manifest(manifest)
        if errors:
            return False
        if manifest.plugin_id in self._registered:
            return False
        self._registered[manifest.plugin_id] = manifest
        return True

    def unregister_plugin(self, plugin_id: str) -> bool:
        if plugin_id in self._registered:
            del self._registered[plugin_id]
            return True
        return False

    def discover_plugins(self, plugin_dir: str | Path) -> list[PluginManifest]:
        plugin_dir = Path(plugin_dir)
        if not plugin_dir.exists() or not plugin_dir.is_dir():
            return []

        manifests: list[PluginManifest] = []
        for path in sorted(plugin_dir.glob("*.json")):
            try:
                with open(path, "r") as f:
                    raw = json.load(f)
                manifest = _parse_manifest(raw)
                errors = self.validate_manifest(manifest)
                if not errors:
                    manifests.append(manifest)
            except (json.JSONDecodeError, KeyError):
                continue
        return manifests

    def validate_manifest(self, manifest: PluginManifest) -> list[str]:
        errors: list[str] = []

        if not manifest.plugin_id:
            errors.append("plugin_id is required")
        if not manifest.name:
            errors.append("name is required")
        if not manifest.version:
            errors.append("version is required")
        if not manifest.author:
            errors.append("author is required")

        if manifest.trust_level not in VALID_TRUST_LEVELS:
            errors.append(f"invalid trust_level: '{manifest.trust_level}'")

        for perm in manifest.permissions:
            if perm not in ALL_KNOWN_PERMISSIONS:
                errors.append(f"unknown permission '{perm}'")

        return errors

    def list_registered(self) -> list[PluginManifest]:
        return list(self._registered.values())

    def get_plugin(self, plugin_id: str) -> PluginManifest | None:
        return self._registered.get(plugin_id)

    def search_plugins(self, query: str) -> list[PluginManifest]:
        query_lower = query.lower()
        results: list[PluginManifest] = []
        for manifest in self._registered.values():
            if query_lower in manifest.name.lower() or query_lower in manifest.author.lower():
                results.append(manifest)
        return results
