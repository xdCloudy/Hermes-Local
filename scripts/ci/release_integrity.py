#!/usr/bin/env python3
"""Create and verify Hermes Local release integrity metadata.

The verifier is deliberately fail-closed: malformed metadata, unsafe paths,
missing artifacts, size/hash mismatches, invalid CycloneDX SBOMs, required
Authenticode failures, or required provenance failures all return non-zero.
"""

from __future__ import annotations

import argparse
from pathlib import Path
import sys

from release_integrity_common import CHANNELS, IntegrityError, _utc_now, _write_json
from release_integrity_create import create_manifest
from release_integrity_verify import verify_manifest


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)

    create = subparsers.add_parser("create", help="Create a release manifest and SHA256SUMS")
    create.add_argument("--root", required=True)
    create.add_argument("--output", required=True)
    create.add_argument("--version-manifest", required=True)
    create.add_argument("--release")
    create.add_argument("--channel", default="stable", choices=sorted(CHANNELS))
    create.add_argument("--repository", required=True)
    create.add_argument("--source-commit", required=True)
    create.add_argument("--workflow", required=True)
    create.add_argument("--run-id", required=True)
    create.add_argument("--artifact", action="append", default=[], required=True)
    create.add_argument("--sbom", action="append", default=[])
    create.add_argument("--dependency-lock", action="append", default=[])
    create.add_argument("--toolchain", action="append", default=[])
    create.add_argument("--build-command", action="append", default=[])
    create.add_argument("--authenticode-required", action="append", default=[])
    create.add_argument("--checksums-name", default="SHA256SUMS")
    create.set_defaults(handler=create_manifest)

    verify = subparsers.add_parser("verify", help="Fail closed unless all release controls verify")
    verify.add_argument("--manifest", required=True)
    verify.add_argument("--artifact-root", required=True)
    verify.add_argument("--require-attestation", action="store_true")
    verify.add_argument("--attestation-bundle-dir")
    verify.add_argument("--trusted-root")
    verify.add_argument("--report")
    verify.set_defaults(handler=verify_manifest)
    return parser


def main(argv: list[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)
    try:
        return int(args.handler(args))
    except IntegrityError as exc:
        if getattr(args, "command", None) == "verify" and getattr(args, "report", None):
            try:
                _write_json(
                    Path(args.report).resolve(),
                    {
                        "schemaVersion": 1,
                        "verifiedAt": _utc_now(),
                        "manifest": str(Path(args.manifest).resolve()),
                        "artifactRoot": str(Path(args.artifact_root).resolve()),
                        "status": "failed",
                        "failure": {"message": str(exc)},
                    },
                )
            except Exception:
                pass
        print(f"release-integrity: {exc}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
