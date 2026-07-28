"""Validate tracked defaults, model manifests, and optional per-user settings."""

from __future__ import annotations

import json
import sys
from pathlib import Path
from typing import Any

from jsonschema import Draft202012Validator
from referencing import Registry, Resource


def load(path: Path) -> Any:
    with path.open(encoding="utf-8") as handle:
        return json.load(handle)


def main() -> int:
    root = Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else Path(__file__).resolve().parents[1]
    schemas = {
        "https://local.hermes.invalid/schemas/workstation.schema.json": root / "config/schemas/workstation.schema.json",
        "https://local.hermes.invalid/schemas/model.schema.json": root / "config/schemas/model.schema.json",
        "https://local.hermes.invalid/schemas/profiles.schema.json": root / "config/profiles/profiles.schema.json",
    }
    registry = Registry()
    for uri, schema_path in schemas.items():
        registry = registry.with_resource(uri, Resource.from_contents(load(schema_path)))

    workstation_validator = Draft202012Validator(load(schemas[next(iter(schemas))]), registry=registry)
    profile_validator = Draft202012Validator(
        load(schemas["https://local.hermes.invalid/schemas/profiles.schema.json"]),
        registry=registry,
    )
    model_validator = Draft202012Validator(
        load(schemas["https://local.hermes.invalid/schemas/model.schema.json"]),
        registry=registry,
    )

    workstation_validator.validate(load(root / "config/defaults/workstation.json"))
    profile_validator.validate(load(root / "config/profiles/profiles.json"))
    for manifest in sorted((root / "models/manifests").glob("*.json")):
        model_validator.validate(load(manifest))

    user_settings = root / "config/launcher/user-settings.json"
    if user_settings.exists():
        workstation_validator.validate(load(user_settings))

    print("Hermes Local configuration is valid.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
