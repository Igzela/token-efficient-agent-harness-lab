"""Stable file entrypoint used by the Rust managed external-runtime invoker."""

from acp_langgraph_adapter.adapter import main


if __name__ == "__main__":
    raise SystemExit(main())
