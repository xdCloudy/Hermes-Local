"""Merge Hermes Local's selected runtime into the user's Hermes YAML config."""

from __future__ import annotations

import argparse
import os
from pathlib import Path
from typing import Any

import yaml


def load_mapping(path: Path) -> dict[str, Any]:
    if not path.exists():
        return {}
    value = yaml.safe_load(path.read_text(encoding="utf-8"))
    if value is None:
        return {}
    if not isinstance(value, dict):
        raise ValueError(f"{path} must contain a YAML mapping")
    return value


def mapping(parent: dict[str, Any], key: str) -> dict[str, Any]:
    value = parent.get(key)
    if not isinstance(value, dict):
        value = {}
        parent[key] = value
    return value


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--config", required=True, type=Path)
    parser.add_argument("--template", required=True, type=Path)
    parser.add_argument("--provider", required=True)
    parser.add_argument("--model", required=True)
    parser.add_argument("--base-url", required=True)
    parser.add_argument("--context", required=True, type=int)
    parser.add_argument("--cwd", required=True)
    parser.add_argument("--root", required=True)
    args = parser.parse_args()

    document = load_mapping(args.config) if args.config.exists() else load_mapping(args.template)

    model = mapping(document, "model")
    model.update(
        {
            "provider": args.provider,
            "default": args.model,
            "base_url": args.base_url,
            "context_length": args.context,
            "max_tokens": min(16_384, max(1_024, args.context // 4)),
        }
    )

    terminal = mapping(document, "terminal")
    terminal["backend"] = "local"
    terminal["cwd"] = str(Path(args.cwd).resolve())

    providers = mapping(document, "providers")
    for name, provider in list(providers.items()):
        if name == args.provider:
            continue
        if isinstance(provider, dict) and provider.get("key_env") == "HERMES_LOCAL_API_TOKEN":
            del providers[name]
    providers[args.provider] = {
        "api": args.base_url,
        "name": args.provider,
        "key_env": "HERMES_LOCAL_API_TOKEN",
        "models": {args.model: {"context_length": args.context}},
        "default_model": args.model,
        "transport": "chat_completions",
    }

    approvals = mapping(document, "approvals")
    existing_deny = approvals.get("deny")
    deny = [value for value in existing_deny if isinstance(value, str)] if isinstance(existing_deny, list) else []
    deny = [
        value
        for value in deny
        if not (
            "hermes-local" in value.lower()
            and ("remove-item" in value.lower() or value.lower().startswith("*rm "))
        )
    ]
    root = str(Path(args.root).resolve())
    for value in (f"*Remove-Item*{root}*", f"*rm *{root.replace(os.sep, '/')}*"):
        if value not in deny:
            deny.append(value)
    approvals["deny"] = deny

    args.config.parent.mkdir(parents=True, exist_ok=True)
    temporary = args.config.with_name(f"{args.config.name}.{os.getpid()}.tmp")
    temporary.write_text(
        yaml.safe_dump(document, sort_keys=False, allow_unicode=True),
        encoding="utf-8",
    )
    os.replace(temporary, args.config)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
