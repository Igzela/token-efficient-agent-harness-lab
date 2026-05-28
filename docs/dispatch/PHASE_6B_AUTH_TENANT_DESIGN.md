# Phase 6B: Production Hardening — Auth/Tenant Design

## Status

**Phase 6B-0: Design Freeze** — 2026-05-28
**Phase 6B-1: Per-server Route Isolation** — 2026-05-28

## Scope

Phase 6B adds production-ready API hardening to the local stdlib HTTP server introduced in Phase 6A. It is split into four sub-phases:

| Sub-phase | Scope | Status |
|---|---|---|
| 6B-0 | Design freeze — schemas only, no runtime changes | Done |
| 6B-1 | Per-server route isolation — routes/store/config per server instance | Done |
| 6B-2 | Local API key + tenant boundary — deterministic local auth | Not started |
| 6B-3 | Scoped authorization enforcement + rate limiting + observability integration | Not started |

## Boundaries

- **stdlib only** — no FastAPI, no PostgreSQL, no third-party packages
- **No OAuth, no external identity systems** — local API key hash only
- **No public deployment** — local development/testing only
- **No Phase 7/plugin work**

## Schema Definitions

### ServerContext (6B-1)

Per-server state container, attached to `HTTPServer` instance.

```python
@dataclass
class ServerContext:
    config: ServerConfig
    routes: dict[tuple[str, str], RequestHandler] = field(default_factory=dict)
    store: Any = None
```

### RequestContext (6B-2, future)

Per-request auth context passed through the handler pipeline.

```python
@dataclass(frozen=True)
class RequestContext:
    tenant_id: str
    api_key_id: str
    scopes: frozenset[str]
    request_id: str
```

### Tenant (6B-2, future)

```python
@dataclass(frozen=True)
class Tenant:
    tenant_id: str
    name: str
    scopes: frozenset[str]
    rate_limit: int | None = None
```

### APIKey (6B-2, future)

```python
@dataclass(frozen=True)
class APIKey:
    key_id: str
    tenant_id: str
    key_hash: str  # SHA-256 hash, never raw key
    scopes: frozenset[str]
    created_at: float
    expires_at: float | None = None
```

### AuthDecision (6B-2, future)

```python
@dataclass(frozen=True)
class AuthDecision:
    allowed: bool
    tenant_id: str | None
    scopes: frozenset[str]
    reason: str
```

## Acceptance Criteria

### 6B-1: Per-server Route Isolation

1. Two servers created with different configs have independent route registries
2. Adding a route to server A does not appear on server B
3. Each server has its own store attribute
4. Two servers can use different `api_prefix` values
5. `clear_routes(server_a)` does not affect server B's routes
6. Existing public API (`register_route`, `clear_routes`, `create_server`, `start_server_in_thread`) keeps working with optional server parameter
7. No auth behavior introduced

### 6B-2: Local API Key + Tenant Boundary (future)

1. API keys stored as SHA-256 hashes, never raw
2. Tenant resolver maps key to tenant
3. Scope checker enforces per-route scope requirements
4. All protected routes must pass through RequestContext

### 6B-3: Authorization Enforcement (future)

1. Rate limiting per tenant
2. Observability integration with Phase 6A MetricsCollector
3. Audit trail for auth decisions

## Migration Notes

- 6B-1 changes `register_route()` and `clear_routes()` signatures to accept optional `server` parameter
- Default behavior (no server parameter) falls back to last-created server for backward compatibility
- Class-level `HarnessHTTPHandler.routes` / `.store` / `.config` removed
- Tests updated to register routes against specific server instances
