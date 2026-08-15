#!/usr/bin/env python3
"""Build and verify the Windows Rust distribution payload for Hermes Local."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import shutil
import sys
import zipfile
from pathlib import Path
from typing import Any

PRODUCT_NAME = "Hermes Local"
PACKAGE_NAME = "hermes-local"
ARCHITECTURE = "x86_64"
APP_ID = "F55A89C1-897E-4A89-9E07-11851CE65E51"
APP_USER_MODEL_ID = "xdCloudy.HermesLocal"
PORTABLE_NAME = "Hermes-Local-Portable-x64.zip"
INSTALLER_NAME = "Hermes-Local-Setup.exe"
INSTALL_STAMP_NAME = "install-stamp.json"
PACKAGE_MANIFEST_NAME = "package-manifest.json"
VERSION_NAME = "VERSION.json"
EXECUTABLE_NAME = "hermes-local.exe"
STAMP_SCHEMA = 1
PACKAGE_SCHEMA = 1
_SHA_RE = re.compile(r"^[0-9a-fA-F]{40}$")


class PackageError(RuntimeError):
    pass


def _read_json(path: Path) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(encoding="utf-8-sig"))
    except (OSError, json.JSONDecodeError) as exc:
        raise PackageError(f"cannot read JSON {path}: {exc}") from exc
    if not isinstance(value, dict):
        raise PackageError(f"{path} must contain a JSON object")
    return value


def _write_json(path: Path, value: dict[str, Any]) -> None:
    path.write_text(
        json.dumps(value, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
        newline="\n",
    )


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _version(version_manifest: dict[str, Any]) -> tuple[str, str]:
    product = version_manifest.get("product")
    if not isinstance(product, dict):
        raise PackageError("VERSION.json is missing product object")
    version = product.get("version")
    channel = product.get("channel")
    if not isinstance(version, str) or not re.fullmatch(
        r"\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?", version
    ):
        raise PackageError("VERSION.json product.version is not a supported semantic version")
    if not isinstance(channel, str) or not channel.strip():
        raise PackageError("VERSION.json product.channel is missing")
    return version, channel.strip()


def _validate_source_commit(source_commit: str) -> str:
    value = source_commit.strip().lower()
    if not _SHA_RE.fullmatch(value):
        raise PackageError("source commit must be a 40-character Git SHA")
    return value


def _validate_binary(path: Path) -> None:
    if not path.is_file():
        raise PackageError(f"Rust executable not found: {path}")
    with path.open("rb") as handle:
        if handle.read(2) != b"MZ":
            raise PackageError(f"{path} is not a Windows PE executable")


def _payload_manifest(stage: Path) -> dict[str, Any]:
    files: dict[str, dict[str, Any]] = {}
    for name in (EXECUTABLE_NAME, VERSION_NAME, INSTALL_STAMP_NAME):
        path = stage / name
        files[name] = {"sha256": _sha256(path), "size": path.stat().st_size}
    return {
        "schemaVersion": PACKAGE_SCHEMA,
        "product": PRODUCT_NAME,
        "architecture": ARCHITECTURE,
        "files": files,
    }


def _assert_payload(root: Path, source_commit: str) -> dict[str, Any]:
    expected = {
        EXECUTABLE_NAME,
        VERSION_NAME,
        INSTALL_STAMP_NAME,
        PACKAGE_MANIFEST_NAME,
    }
    found = {path.name for path in root.iterdir() if path.is_file()}
    missing = expected - found
    if missing:
        raise PackageError(f"payload is missing: {', '.join(sorted(missing))}")

    stamp = _read_json(root / INSTALL_STAMP_NAME)
    if stamp.get("schemaVersion") != STAMP_SCHEMA:
        raise PackageError("install stamp schemaVersion mismatch")
    if stamp.get("product") != PRODUCT_NAME or stamp.get("package") != PACKAGE_NAME:
        raise PackageError("install stamp product identity mismatch")
    if stamp.get("architecture") != ARCHITECTURE:
        raise PackageError("install stamp architecture mismatch")
    if stamp.get("executable") != EXECUTABLE_NAME:
        raise PackageError("install stamp executable mismatch")
    if stamp.get("sourceCommit") != _validate_source_commit(source_commit):
        raise PackageError("install stamp source commit mismatch")

    version_manifest = _read_json(root / VERSION_NAME)
    version, channel = _version(version_manifest)
    if stamp.get("version") != version or stamp.get("channel") != channel:
        raise PackageError("install stamp version/channel does not match VERSION.json")

    manifest = _read_json(root / PACKAGE_MANIFEST_NAME)
    if manifest.get("schemaVersion") != PACKAGE_SCHEMA:
        raise PackageError("package manifest schemaVersion mismatch")
    if (
        manifest.get("product") != PRODUCT_NAME
        or manifest.get("architecture") != ARCHITECTURE
    ):
        raise PackageError("package manifest identity mismatch")
    files = manifest.get("files")
    if not isinstance(files, dict):
        raise PackageError("package manifest files object is missing")
    for name in (EXECUTABLE_NAME, VERSION_NAME, INSTALL_STAMP_NAME):
        record = files.get(name)
        if not isinstance(record, dict):
            raise PackageError(f"package manifest is missing {name}")
        path = root / name
        if record.get("sha256") != _sha256(path):
            raise PackageError(f"package manifest hash mismatch for {name}")
        if record.get("size") != path.stat().st_size:
            raise PackageError(f"package manifest size mismatch for {name}")
    _validate_binary(root / EXECUTABLE_NAME)
    return stamp


def _inno_escape(value: Path | str) -> str:
    return str(value).replace('"', '""')


def _render_inno(stage: Path, output_root: Path, version: str) -> str:
    source = _inno_escape(stage)
    output = _inno_escape(output_root)
    return f"""; Generated by scripts/ci/rust_windows_package.py. Do not hand-edit.
#define AppVersion "{version}"

[Setup]
AppId={{{{{APP_ID}}}}}
AppName={PRODUCT_NAME}
AppVersion={{#AppVersion}}
AppVerName={PRODUCT_NAME} {{#AppVersion}}
AppPublisher=Hermes Local
DefaultDirName={{localappdata}}\\Programs\\Hermes Local
DefaultGroupName=Hermes Local
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir={output}
OutputBaseFilename=Hermes-Local-Setup
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
DisableProgramGroupPage=yes
UsePreviousAppDir=yes
CloseApplications=yes
RestartApplications=no
UninstallDisplayName={PRODUCT_NAME}
UninstallDisplayIcon={{app}}\\{EXECUTABLE_NAME}
VersionInfoVersion={{#AppVersion}}
VersionInfoProductName={PRODUCT_NAME}
VersionInfoDescription={PRODUCT_NAME} Rust desktop installer

[Files]
Source: "{source}\\{EXECUTABLE_NAME}"; DestDir: "{{app}}"; Flags: ignoreversion
Source: "{source}\\{VERSION_NAME}"; DestDir: "{{app}}"; Flags: ignoreversion
Source: "{source}\\{INSTALL_STAMP_NAME}"; DestDir: "{{app}}"; Flags: ignoreversion
Source: "{source}\\{PACKAGE_MANIFEST_NAME}"; DestDir: "{{app}}"; Flags: ignoreversion

[Icons]
Name: "{{autoprograms}}\\Hermes Local"; Filename: "{{app}}\\{EXECUTABLE_NAME}"; WorkingDir: "{{app}}"; AppUserModelID: "{APP_USER_MODEL_ID}"
"""


def prepare(
    binary: Path,
    version_manifest_path: Path,
    work_root: Path,
    output_root: Path,
    source_commit: str,
) -> dict[str, str]:
    source_commit = _validate_source_commit(source_commit)
    _validate_binary(binary)
    version_manifest = _read_json(version_manifest_path)
    version, channel = _version(version_manifest)

    stage = work_root / "stage"
    if work_root.exists():
        shutil.rmtree(work_root)
    stage.mkdir(parents=True)
    output_root.mkdir(parents=True, exist_ok=True)

    shutil.copy2(binary, stage / EXECUTABLE_NAME)
    shutil.copy2(version_manifest_path, stage / VERSION_NAME)
    stamp = {
        "schemaVersion": STAMP_SCHEMA,
        "product": PRODUCT_NAME,
        "package": PACKAGE_NAME,
        "version": version,
        "channel": channel,
        "architecture": ARCHITECTURE,
        "executable": EXECUTABLE_NAME,
        "sourceCommit": source_commit,
    }
    _write_json(stage / INSTALL_STAMP_NAME, stamp)
    _write_json(stage / PACKAGE_MANIFEST_NAME, _payload_manifest(stage))
    _assert_payload(stage, source_commit)

    portable = output_root / PORTABLE_NAME
    if portable.exists():
        portable.unlink()
    with zipfile.ZipFile(
        portable, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9
    ) as archive:
        for name in (
            EXECUTABLE_NAME,
            VERSION_NAME,
            INSTALL_STAMP_NAME,
            PACKAGE_MANIFEST_NAME,
        ):
            archive.write(stage / name, arcname=name)

    inno = work_root / "Hermes-Local.iss"
    inno.write_text(
        _render_inno(stage.resolve(), output_root.resolve(), version),
        encoding="utf-8",
        newline="\n",
    )
    return {
        "version": version,
        "stage": str(stage),
        "portable": str(portable),
        "installer": str(output_root / INSTALLER_NAME),
        "inno": str(inno),
    }


class tempfile_directory:
    def __init__(self) -> None:
        import tempfile

        self._temporary = tempfile.TemporaryDirectory()

    def __enter__(self) -> Path:
        return Path(self._temporary.name)

    def __exit__(self, exc_type, exc, tb) -> None:
        self._temporary.cleanup()


def verify_portable(archive: Path, source_commit: str) -> None:
    if not archive.is_file():
        raise PackageError(f"portable archive not found: {archive}")
    with tempfile_directory() as root:
        with zipfile.ZipFile(archive, "r") as bundle:
            names = set(bundle.namelist())
            expected = {
                EXECUTABLE_NAME,
                VERSION_NAME,
                INSTALL_STAMP_NAME,
                PACKAGE_MANIFEST_NAME,
            }
            if names != expected:
                raise PackageError(
                    f"portable archive entries mismatch: expected {sorted(expected)}, got {sorted(names)}"
                )
            bundle.extractall(root)
        _assert_payload(root, source_commit)


def verify_install(root: Path, source_commit: str) -> None:
    if not root.is_dir():
        raise PackageError(f"install root not found: {root}")
    _assert_payload(root, source_commit)


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)

    prepare_cmd = sub.add_parser("prepare")
    prepare_cmd.add_argument("--binary", type=Path, required=True)
    prepare_cmd.add_argument("--version-manifest", type=Path, required=True)
    prepare_cmd.add_argument("--work-root", type=Path, required=True)
    prepare_cmd.add_argument("--output-root", type=Path, required=True)
    prepare_cmd.add_argument("--source-commit", required=True)

    portable_cmd = sub.add_parser("verify-portable")
    portable_cmd.add_argument("--archive", type=Path, required=True)
    portable_cmd.add_argument("--source-commit", required=True)

    install_cmd = sub.add_parser("verify-install")
    install_cmd.add_argument("--root", type=Path, required=True)
    install_cmd.add_argument("--source-commit", required=True)
    return parser


def main(argv: list[str] | None = None) -> int:
    args = _parser().parse_args(argv)
    try:
        if args.command == "prepare":
            result = prepare(
                args.binary,
                args.version_manifest,
                args.work_root,
                args.output_root,
                args.source_commit,
            )
            print(json.dumps(result, sort_keys=True))
        elif args.command == "verify-portable":
            verify_portable(args.archive, args.source_commit)
        else:
            verify_install(args.root, args.source_commit)
    except PackageError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
