#!/usr/bin/env python3
"""Validate that launcher selection, supervisor state, and Hermes provider identity agree."""
from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Any

import yaml


def load_json(path: Path) -> dict[str, Any]:
    value = json.loads(path.read_text(encoding="utf-8-sig"))
    if not isinstance(value, dict):
        raise ValueError(f"{path} must contain a JSON object")
    return value


def load_yaml(path: Path) -> dict[str, Any]:
    value = yaml.safe_load(path.read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        raise ValueError(f"{path} must contain a YAML mapping")
    return value


def mapping(parent: dict[str, Any], key: str) -> dict[str, Any]:
    value = parent.get(key)
    if not isinstance(value, dict):
        raise ValueError(f"Missing mapping: {key}")
    return value


def require(condition: bool, message: str, mismatches: list[str]) -> None:
    if not condition:
        mismatches.append(message)


def validate(status: dict[str, Any], config: dict[str, Any], expected_id: str, expected_alias: str) -> dict[str, Any]:
    mismatches: list[str] = []
    model_state = mapping(status, "model")
    hermes_state = mapping(status, "hermes")
    gateway_state = status.get("gateway") if isinstance(status.get("gateway"), dict) else {}
    model_config = mapping(config, "model")
    providers = mapping(config, "providers")
    local_provider = mapping(providers, "local-llama")
    provider_models = mapping(local_provider, "models")

    require(status.get("phase") == "running", f"runtime phase is {status.get('phase')!r}, expected 'running'", mismatches)
    require(status.get("selectedModelId") == expected_id, "runtime selectedModelId does not match launcher selection", mismatches)
    require(model_state.get("alias") == expected_alias, "runtime model alias does not match launcher selection", mismatches)
    require(model_state.get("healthy") is True, "runtime model health is not ready", mismatches)
    require(hermes_state.get("healthy") is True, "Hermes provider health is not ready", mismatches)
    if gateway_state.get("required") is True:
        require(gateway_state.get("healthy") is True, "required gateway health is not ready", mismatches)

    require(model_config.get("provider") == "local-llama", "Hermes model.provider is not local-llama", mismatches)
    require(model_config.get("default") == expected_alias, "Hermes model.default does not match selected alias", mismatches)
    require(local_provider.get("default_model") == expected_alias, "local-llama default_model does not match selected alias", mismatches)
    require(list(provider_models) == [expected_alias], "local-llama models must contain only the selected alias", mismatches)

    return {
        "expectedModelId": expected_id,
        "expectedAlias": expected_alias,
        "runtimeModelId": status.get("selectedModelId"),
        "runtimeAlias": model_state.get("alias"),
        "providerDefault": local_provider.get("default_model"),
        "providerModels": list(provider_models),
        "mismatches": mismatches,
        "ok": not mismatches,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--status", required=True, type=Path)
    parser.add_argument("--config", required=True, type=Path)
    parser.add_argument("--expected-id", required=True)
    parser.add_argument("--expected-alias", required=True)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()

    result = validate(load_json(args.status), load_yaml(args.config), args.expected_id, args.expected_alias)
    rendered = json.dumps(result, indent=2, sort_keys=True)
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(rendered + "\n", encoding="utf-8")
    print(rendered)
    if result["ok"]:
        return 0
    for mismatch in result["mismatches"]:
        print(f"identity mismatch: {mismatch}")
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
