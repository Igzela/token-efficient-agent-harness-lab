# Harness App MVP2 — Local Read-Only Control Plane

## Purpose

MVP2 turns the MVP0 auditor and MVP1 dashboard into the first app architecture
slice:

```text
repo registry
  -> local control-plane API
  -> dashboard API mode
  -> local repo audit
  -> remote repo metadata-only
```

This is not a full autonomous app runtime. It is a local, reviewable control
plane that keeps target repositories read-only.

## Components

- `src/harness_core/app_registry.py`
  - stores app-visible repository references
  - supports `local` and `remote` repo metadata
  - canonicalizes local paths before saving
  - writes only the app registry JSON

- `src/harness_core/app_api.py`
  - pure HTTP-shaped API logic
  - starts no server
  - returns structured JSON errors

- `tools/harness_app_server.py`
  - thin `http.server` wrapper
  - serves `web/dashboard/*`
  - serves `/api/*` from the same local origin

- `web/dashboard/*`
  - renders sample JSON when opened statically
  - connects to `/api/repos` and `/api/audit` when served by the local server
  - can register local and remote repo references

## API

```text
GET  /api/health
GET  /api/repos
POST /api/repos
GET  /api/audit?repo_id=<id>
```

`POST /api/repos` writes app registry state. It does not write to a target
repository.

Remote repositories are metadata-only in MVP2. The server does not clone,
fetch, call GitHub APIs, or audit remote URLs. To audit a remote repository,
register a local checkout as a `local` repo.

## Usage

Run the local app server:

```bash
python3 tools/harness_app_server.py --registry /tmp/harness-app-registry.json
```

Open:

```text
http://127.0.0.1:8765/
```

Register a local repo, for example:

```json
{
  "id": "alters-lab",
  "name": "alters-lab",
  "kind": "local",
  "path": "/home/igzela/Projects/alters-lab"
}
```

Registering a remote repo is allowed as metadata:

```json
{
  "id": "remote-harness",
  "name": "remote harness",
  "kind": "remote",
  "url": "https://github.com/example/project.git"
}
```

Auditing `remote-harness` returns `remote_audit_unsupported`.

## Boundaries

- Server binds only `127.0.0.1` or `localhost`.
- No `0.0.0.0` binding.
- No CORS wildcard.
- No provider calls.
- No sandbox execution.
- No autonomous worker.
- No production deployment.
- No target repository writes.
- No Git clone or fetch.
- No GitHub API calls.
- No approval, activation, merge, or deployment actions.
- API errors are structured JSON and do not expose tracebacks.

## State Model

The target repository remains read-only. The only MVP2 write is the app registry
JSON selected by `--registry`.

The registry schema is:

```json
{
  "schema_version": "app_registry.v1",
  "repos": []
}
```

Local repo audit is only available through `repo_id`. The API does not accept a
direct `path` query parameter for audits.
