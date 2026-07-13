"""Parse agent and canonical workflows while rejecting duplicate mapping keys."""

from pathlib import Path

import yaml
from yaml.constructor import ConstructorError


class UniqueKeyLoader(yaml.SafeLoader):
    """Safe YAML loader that rejects mappings whose keys would be overwritten."""


def _construct_unique_mapping(loader, node, deep=False):
    mapping = {}
    for key_node, value_node in node.value:
        key = loader.construct_object(key_node, deep=deep)
        if key in mapping:
            raise ConstructorError(
                "while constructing a mapping",
                node.start_mark,
                f"found duplicate key {key!r}",
                key_node.start_mark,
            )
        mapping[key] = loader.construct_object(value_node, deep=deep)
    return mapping


UniqueKeyLoader.add_constructor(
    yaml.resolver.BaseResolver.DEFAULT_MAPPING_TAG, _construct_unique_mapping
)


def main() -> None:
    try:
        yaml.load("duplicate: 1\nduplicate: 2\n", Loader=UniqueKeyLoader)
    except ConstructorError:
        pass
    else:
        raise SystemExit("workflow YAML loader does not reject duplicate keys")
    workflows = sorted(Path(".github/workflows").glob("agent-*.yml"))
    workflows.append(Path(".github/workflows/tests.yml"))
    for workflow in workflows:
        parsed = yaml.load(workflow.read_text(encoding="utf-8"), Loader=UniqueKeyLoader)
        if not isinstance(parsed, dict):
            raise SystemExit(f"workflow is not a mapping: {workflow}")
        print(workflow)


if __name__ == "__main__":
    main()
