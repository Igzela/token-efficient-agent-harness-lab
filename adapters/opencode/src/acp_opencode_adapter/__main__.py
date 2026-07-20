from __future__ import annotations

import json
import sys

from .adapter import handle_request


def main() -> int:
    raw = sys.stdin.buffer.read()
    if len(raw) > 256 * 1024:
        sys.stdout.write(
            json.dumps(
                {
                    "schema_version": "opencode_external_error.v1",
                    "code": "request_oversized",
                    "message": "request exceeds bounded input cap",
                },
                sort_keys=True,
            )
        )
        return 2
    try:
        request = json.loads(raw.decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError):
        sys.stdout.write(
            json.dumps(
                {
                    "schema_version": "opencode_external_error.v1",
                    "code": "request_invalid_json",
                    "message": "request is not valid UTF-8 JSON",
                },
                sort_keys=True,
            )
        )
        return 2
    result, code = handle_request(request)
    sys.stdout.write(json.dumps(result, sort_keys=True, separators=(",", ":")))
    return code


if __name__ == "__main__":
    raise SystemExit(main())
