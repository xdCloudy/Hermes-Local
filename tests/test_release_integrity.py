from __future__ import annotations

import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "ci" / "release_integrity.py"
COMMIT = "1" * 40


def write_json(path: Path, value: object) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value), encoding="utf-8")


class ReleaseIntegrityTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.dist = self.root / "dist"
        self.dist.mkdir()
        (self.dist / "Hermes.exe").write_bytes(b"hermes-release")
        write_json(
            self.dist / "sbom" / "launcher.cdx.json",
            {
                "bomFormat": "CycloneDX",
                "specVersion": "1.6",
                "version": 1,
                "components": [],
            },
        )
        self.version = self.root / "VERSION.json"
        write_json(
            self.version,
            {
                "product": {"version": "0.1.0"},
                "sources": {
                    "hermesAgent": {
                        "repository": "https://github.com/NousResearch/hermes-agent.git",
                        "commit": "2" * 40,
                        "integrationCommit": "3" * 40,
                        "integrationBranch": "hermes-local-integration",
                    },
                    "llamaCpp": {
                        "repository": "https://github.com/ggml-org/llama.cpp.git",
                        "commit": "4" * 40,
                        "branch": "master",
                    },
                },
            },
        )
        self.lock = self.root / "package-lock.json"
        self.lock.write_text("{}", encoding="utf-8")
        self.manifest = self.dist / "release-manifest.json"
        self.report = self.root / "verification.json"

    def tearDown(self) -> None:
        self.temp.cleanup()

    def run_cli(self, *args: str) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [sys.executable, str(SCRIPT), *args],
            text=True,
            capture_output=True,
            check=False,
        )

    def create(self) -> subprocess.CompletedProcess[str]:
        return self.run_cli(
            "create",
            "--root", str(self.dist),
            "--output", str(self.manifest),
            "--version-manifest", str(self.version),
            "--repository", "xdCloudy/Hermes-Local",
            "--source-commit", COMMIT,
            "--workflow", "xdCloudy/Hermes-Local/.github/workflows/release-integrity.yml",
            "--run-id", "123",
            "--artifact", "Hermes.exe",
            "--sbom", "launcher=sbom/launcher.cdx.json",
            "--dependency-lock", f"node={self.lock}",
            "--toolchain", "python=3.13",
            "--build-command", "Package-Hermes-Launcher.ps1",
        )

    def verify_without_remote_attestation(self) -> subprocess.CompletedProcess[str]:
        # The manifest intentionally requires provenance. Patch it only for local
        # cryptographic/hash-path unit tests; production callers cannot do this.
        value = json.loads(self.manifest.read_text(encoding="utf-8"))
        value["provenance"]["required"] = True
        self.manifest.write_text(json.dumps(value), encoding="utf-8")
        return self.run_cli(
            "verify",
            "--manifest", str(self.manifest),
            "--artifact-root", str(self.dist),
            "--report", str(self.report),
        )

    def test_create_emits_manifest_and_checksums(self) -> None:
        result = self.create()
        self.assertEqual(result.returncode, 0, result.stderr)
        manifest = json.loads(self.manifest.read_text(encoding="utf-8"))
        self.assertEqual(manifest["schemaVersion"], 1)
        self.assertEqual(manifest["release"]["version"], "0.1.0")
        self.assertEqual(manifest["sources"]["hermesLocal"]["commit"], COMMIT)
        self.assertEqual(manifest["sboms"][0]["format"], "CycloneDX")
        self.assertTrue((self.dist / "SHA256SUMS").is_file())

    def test_tampered_artifact_is_rejected_before_attestation(self) -> None:
        self.assertEqual(self.create().returncode, 0)
        (self.dist / "Hermes.exe").write_bytes(b"tamper-release")
        result = self.run_cli(
            "verify",
            "--manifest", str(self.manifest),
            "--artifact-root", str(self.dist),
            "--report", str(self.report),
        )
        self.assertEqual(result.returncode, 2)
        self.assertIn("SHA-256 mismatch", result.stderr)
        report = json.loads(self.report.read_text(encoding="utf-8"))
        self.assertEqual(report["status"], "failed")
        self.assertIn("SHA-256 mismatch", report["failure"]["message"])

    def test_path_traversal_is_rejected(self) -> None:
        self.assertEqual(self.create().returncode, 0)
        manifest = json.loads(self.manifest.read_text(encoding="utf-8"))
        manifest["artifacts"][0]["name"] = "../outside.exe"
        self.manifest.write_text(json.dumps(manifest), encoding="utf-8")
        result = self.run_cli(
            "verify",
            "--manifest", str(self.manifest),
            "--artifact-root", str(self.dist),
        )
        self.assertEqual(result.returncode, 2)
        self.assertIn("unsafe path", result.stderr)

    def test_invalid_sbom_is_rejected(self) -> None:
        write_json(self.dist / "sbom" / "launcher.cdx.json", {"bomFormat": "SPDX"})
        result = self.create()
        self.assertEqual(result.returncode, 2)
        self.assertIn("not CycloneDX", result.stderr)

    def test_missing_attestation_tool_fails_closed(self) -> None:
        self.assertEqual(self.create().returncode, 0)
        result = self.run_cli(
            "verify",
            "--manifest", str(self.manifest),
            "--artifact-root", str(self.dist),
            "--require-attestation",
        )
        self.assertEqual(result.returncode, 2)
        self.assertTrue(
            "GitHub CLI is required" in result.stderr
            or "Provenance verification failed" in result.stderr
        )

    def test_duplicate_artifact_path_is_rejected(self) -> None:
        self.assertEqual(self.create().returncode, 0)
        manifest = json.loads(self.manifest.read_text(encoding="utf-8"))
        manifest["artifacts"].append(dict(manifest["artifacts"][0]))
        self.manifest.write_text(json.dumps(manifest), encoding="utf-8")
        result = self.run_cli(
            "verify",
            "--manifest", str(self.manifest),
            "--artifact-root", str(self.dist),
        )
        self.assertEqual(result.returncode, 2)
        self.assertIn("Duplicate artifact path", result.stderr)


if __name__ == "__main__":
    unittest.main()
