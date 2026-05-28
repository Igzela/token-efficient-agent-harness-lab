"""Phase 7: PluginSystem — load, validate, and enforce permissions for plugins."""

from __future__ import annotations

import json
import threading
import time
from dataclasses import dataclass, field
from enum import Enum
from pathlib import Path
from typing import Any


PLUGIN_SYSTEM_SCHEMA_VERSION = "plugin_system.v1"

TRUST_LEVEL_COMMUNITY = "community"
TRUST_LEVEL_VERIFIED = "verified"
TRUST_LEVEL_OFFICIAL = "official"

VALID_TRUST_LEVELS = (TRUST_LEVEL_COMMUNITY, TRUST_LEVEL_VERIFIED, TRUST_LEVEL_OFFICIAL)

TRUST_PERMISSIONS = {
    TRUST_LEVEL_COMMUNITY: frozenset({"dispatch:read"}),
    TRUST_LEVEL_VERIFIED: frozenset({"dispatch:read", "dispatch:write"}),
    # OFFICIAL: empty set means unrestricted — check is bypassed in _trust_permission_allowed
    TRUST_LEVEL_OFFICIAL: frozenset(),
}

ALL_KNOWN_PERMISSIONS = frozenset({
    "dispatch:read",
    "dispatch:write",
    "provider:execute",
    "config:read",
    "config:write",
    "ledger:read",
    "ledger:write",
})


class TrustLevel(Enum):
    COMMUNITY = TRUST_LEVEL_COMMUNITY
    VERIFIED = TRUST_LEVEL_VERIFIED
    OFFICIAL = TRUST_LEVEL_OFFICIAL


REQUIRED_MANIFEST_FIELDS = (
    "schema_version",
    "plugin_id",
    "name",
    "version",
    "author",
    "permissions",
    "entrypoints",
    "compatible_dispatcher_versions",
    "required_env",
    "network_access",
    "filesystem_access",
    "trust_level",
)


@dataclass(frozen=True)
class PluginManifest:
    plugin_id: str
    name: str
    version: str
    author: str
    permissions: tuple[str, ...] = ()
    entrypoints: tuple[str, ...] = ()
    compatible_dispatcher_versions: tuple[str, ...] = ()
    required_env: tuple[str, ...] = ()
    network_access: bool = False
    filesystem_access: bool = False
    signature: str | None = None
    trust_level: str = TRUST_LEVEL_COMMUNITY
    schema_version: str = "plugin_manifest.v1"

    def to_dict(self) -> dict[str, Any]:
        return {
            "schema_version": self.schema_version,
            "plugin_id": self.plugin_id,
            "name": self.name,
            "version": self.version,
            "author": self.author,
            "permissions": list(self.permissions),
            "entrypoints": list(self.entrypoints),
            "compatible_dispatcher_versions": list(self.compatible_dispatcher_versions),
            "required_env": list(self.required_env),
            "network_access": self.network_access,
            "filesystem_access": self.filesystem_access,
            "signature": self.signature,
            "trust_level": self.trust_level,
        }


@dataclass(frozen=True)
class LoadedPlugin:
    manifest: PluginManifest
    module_name: str
    loaded_at: float = field(default_factory=time.time)
    status: str = "loaded"


class PluginSystem:
    """Loads plugins from JSON manifests and enforces trust-level permissions."""

    def __init__(self) -> None:
        self._plugins: dict[str, LoadedPlugin] = {}
        self._lock = threading.RLock()

    def load_plugin(
        self,
        manifest_path: str | Path,
        plugin_dir: str | Path,
    ) -> LoadedPlugin:
        manifest_path = Path(manifest_path)
        plugin_dir = Path(plugin_dir)

        if not manifest_path.exists():
            raise FileNotFoundError(f"Manifest not found: {manifest_path}")
        if not plugin_dir.exists():
            raise NotADirectoryError(f"Plugin directory not found: {plugin_dir}")

        with open(manifest_path, "r") as f:
            raw = json.load(f)

        manifest = _parse_manifest(raw)
        errors = _validate_manifest_fields(manifest)
        if errors:
            raise ValueError(f"Invalid manifest: {'; '.join(errors)}")

        if not _trust_permission_allowed(manifest.trust_level, manifest.permissions):
            raise PermissionError(
                f"Trust level '{manifest.trust_level}' not allowed for permissions: "
                f"{[p for p in manifest.permissions if p not in TRUST_PERMISSIONS.get(manifest.trust_level, frozenset())]}"
            )

        module_name = f"harness_plugin.{manifest.plugin_id}"

        with self._lock:
            if manifest.plugin_id in self._plugins:
                del self._plugins[manifest.plugin_id]

            loaded = LoadedPlugin(manifest=manifest, module_name=module_name)
            self._plugins[manifest.plugin_id] = loaded
            return loaded

    def unload_plugin(self, plugin_id: str) -> bool:
        with self._lock:
            if plugin_id in self._plugins:
                del self._plugins[plugin_id]
                return True
            return False

    def check_permission(self, plugin_id: str, permission: str) -> bool:
        with self._lock:
            loaded = self._plugins.get(plugin_id)
        if loaded is None:
            return False
        if permission not in ALL_KNOWN_PERMISSIONS:
            return False
        return permission in loaded.manifest.permissions

    def list_plugins(self) -> list[LoadedPlugin]:
        with self._lock:
            return list(self._plugins.values())

    def get_plugin(self, plugin_id: str) -> LoadedPlugin | None:
        with self._lock:
            return self._plugins.get(plugin_id)


def _parse_manifest(raw: dict[str, Any]) -> PluginManifest:
    perms = raw.get("permissions", [])
    if isinstance(perms, list):
        perms = tuple(str(p) for p in perms)
    else:
        perms = ()

    entrypoints = raw.get("entrypoints", [])
    if isinstance(entrypoints, list):
        entrypoints = tuple(str(e) for e in entrypoints)
    else:
        entrypoints = ()

    compat = raw.get("compatible_dispatcher_versions", [])
    if isinstance(compat, list):
        compat = tuple(str(c) for c in compat)
    else:
        compat = ()

    req_env = raw.get("required_env", [])
    if isinstance(req_env, list):
        req_env = tuple(str(r) for r in req_env)
    else:
        req_env = ()

    return PluginManifest(
        plugin_id=str(raw.get("plugin_id", "")),
        name=str(raw.get("name", "")),
        version=str(raw.get("version", "")),
        author=str(raw.get("author", "")),
        permissions=perms,
        entrypoints=entrypoints,
        compatible_dispatcher_versions=compat,
        required_env=req_env,
        network_access=bool(raw.get("network_access", False)),
        filesystem_access=bool(raw.get("filesystem_access", False)),
        signature=raw.get("signature"),
        trust_level=str(raw.get("trust_level", TRUST_LEVEL_COMMUNITY)),
        schema_version=str(raw.get("schema_version", "plugin_manifest.v1")),
    )


def _validate_manifest_fields(manifest: PluginManifest) -> list[str]:
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
        errors.append(f"trust_level must be one of {VALID_TRUST_LEVELS}, got '{manifest.trust_level}'")
    for perm in manifest.permissions:
        if perm not in ALL_KNOWN_PERMISSIONS:
            errors.append(f"unknown permission '{perm}'")
    return errors


def _trust_permission_allowed(trust_level: str, permissions: tuple[str, ...]) -> bool:
    if trust_level == TRUST_LEVEL_OFFICIAL:
        return True
    allowed = TRUST_PERMISSIONS.get(trust_level, frozenset())
    for perm in permissions:
        if perm not in allowed:
            return False
    return True
