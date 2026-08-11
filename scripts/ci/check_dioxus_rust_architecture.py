#!/usr/bin/env python3
"""Guard the Rust/Dioxus ownership boundary during the Desktop migration."""

from __future__ import annotations

import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]

EXPECTED_MEMBERS = {
    "apps/desktop",
    "crates/hermes-agent-client",
    "crates/hermes-core",
    "crates/hermes-desktop",
    "crates/hermes-protocol",
    "crates/hermes-ui",
}

UI_FORBIDDEN_DEPENDENCIES = {
    "hermes-desktop",
    "keyring",
    "open",
    "portable-pty",
    "rfd",
    "tokio-tungstenite",
    "trash",
    "windows",
    "windows-sys",
}

UI_FORBIDDEN_SOURCE_MARKERS = {
    "std::process::Command": "process execution",
    "tokio::process::Command": "async process execution",
    "std::fs::": "direct filesystem authority",
    "portable_pty": "PTY authority",
    "keyring::": "secret-store authority",
    "trash::": "filesystem deletion authority",
    "windows::": "Windows API authority",
    "windows_sys::": "Windows API authority",
}


def load_toml(path: Path) -> dict:
    with path.open("rb") as handle:
        return tomllib.load(handle)


def dependency_names(manifest: dict) -> set[str]:
    names = set(manifest.get("dependencies", {}))
    target = manifest.get("target", {})
    if isinstance(target, dict):
        for target_table in target.values():
            if isinstance(target_table, dict):
                names.update(target_table.get("dependencies", {}))
    return names


def fail(messages: list[str]) -> None:
    print("Rust/Dioxus architecture guard failed:", file=sys.stderr)
    for message in messages:
        print(f"  - {message}", file=sys.stderr)
    raise SystemExit(1)


def main() -> None:
    errors: list[str] = []

    workspace = load_toml(ROOT / "Cargo.toml")
    members = set(workspace.get("workspace", {}).get("members", []))
    missing_members = EXPECTED_MEMBERS - members
    if missing_members:
        errors.append(f"workspace is missing members: {sorted(missing_members)}")

    ui_manifest = load_toml(ROOT / "crates/hermes-ui/Cargo.toml")
    ui_dependencies = dependency_names(ui_manifest)
    forbidden_dependencies = UI_FORBIDDEN_DEPENDENCIES & ui_dependencies
    if forbidden_dependencies:
        errors.append(
            "hermes-ui directly depends on privileged/native crates: "
            f"{sorted(forbidden_dependencies)}"
        )

    desktop_manifest = load_toml(ROOT / "apps/desktop/Cargo.toml")
    desktop_dependencies = dependency_names(desktop_manifest)
    for required in {"hermes-core", "hermes-desktop", "hermes-ui"}:
        if required not in desktop_dependencies:
            errors.append(f"apps/desktop must compose {required}")

    ui_root = ROOT / "crates/hermes-ui"
    for path in sorted(ui_root.rglob("*.rs")):
        text = path.read_text(encoding="utf-8")
        for marker, authority in UI_FORBIDDEN_SOURCE_MARKERS.items():
            if marker in text:
                errors.append(
                    f"{path.relative_to(ROOT)} contains {authority} marker {marker!r}"
                )

    main_source = (ROOT / "apps/desktop/src/main.rs").read_text(encoding="utf-8")
    if "NativeApp::new" not in main_source:
        errors.append("apps/desktop is no longer composing the native service authority")
    if "hermes_ui::App" not in main_source:
        errors.append("apps/desktop is no longer composing the shared Dioxus UI")

    dioxus_config = load_toml(ROOT / "apps/desktop/Dioxus.toml")
    bundle = dioxus_config.get("bundle", {})
    if bundle.get("identifier") != "com.nousresearch.hermes.local":
        errors.append("Dioxus bundle identifier drifted from the Hermes Desktop identity")

    if errors:
        fail(errors)

    print(
        "Rust/Dioxus architecture guard passed: shared UI has no direct native authority; "
        "Desktop composition and bundle identity are intact."
    )


if __name__ == "__main__":
    main()
