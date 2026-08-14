from __future__ import annotations

import importlib.util
import json
import tempfile
import unittest
import zipfile
from pathlib import Path

MODULE_PATH = Path(__file__).resolve().parents[1] / "scripts" / "ci" / "rust_windows_package.py"
SPEC = importlib.util.spec_from_file_location("rust_windows_package", MODULE_PATH)
assert SPEC and SPEC.loader
package = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(package)

SOURCE_COMMIT = "a" * 40


def fake_pe(path: Path) -> None:
    path.write_bytes(b"MZ" + b"\0" * 2048)


def version_manifest(path: Path, version: str = "0.18.62") -> None:
    path.write_text(
        json.dumps(
            {
                "schemaVersion": 2,
                "product": {
                    "name": "Hermes Launcher",
                    "version": version,
                    "channel": "development",
                },
            }
        ),
        encoding="utf-8",
    )


class RustWindowsPackageTests(unittest.TestCase):
    def test_prepare_builds_exact_portable_payload_and_inno_identity(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            binary = root / "input.exe"
            version = root / "VERSION.json"
            fake_pe(binary)
            version_manifest(version)
            result = package.prepare(
                binary,
                version,
                root / "work",
                root / "out",
                SOURCE_COMMIT,
            )

            portable = Path(result["portable"])
            self.assertEqual(portable.name, package.PORTABLE_NAME)
            package.verify_portable(portable, SOURCE_COMMIT)

            with zipfile.ZipFile(portable) as archive:
                self.assertEqual(
                    set(archive.namelist()),
                    {
                        package.EXECUTABLE_NAME,
                        package.VERSION_NAME,
                        package.INSTALL_STAMP_NAME,
                        package.PACKAGE_MANIFEST_NAME,
                    },
                )
                stamp = json.loads(archive.read(package.INSTALL_STAMP_NAME))
                manifest = json.loads(archive.read(package.PACKAGE_MANIFEST_NAME))

            self.assertEqual(stamp["version"], "0.18.62")
            self.assertEqual(stamp["sourceCommit"], SOURCE_COMMIT)
            self.assertEqual(stamp["architecture"], "x86_64")
            self.assertEqual(manifest["files"]["hermes-local.exe"]["size"], 2050)

            iss = Path(result["inno"]).read_text(encoding="utf-8")
            self.assertIn("PrivilegesRequired=lowest", iss)
            self.assertIn("DefaultDirName={localappdata}\\Programs\\Hermes Local", iss)
            self.assertIn('Name: "{autoprograms}\\Hermes Local"', iss)
            self.assertIn('AppUserModelID: "xdCloudy.HermesLocal"', iss)
            self.assertEqual(package.APP_USER_MODEL_ID, "xdCloudy.HermesLocal")
            self.assertIn("OutputBaseFilename=Hermes-Local-Setup", iss)
            self.assertIn('Source: "', iss)
            self.assertNotIn("electron", iss.lower())
            self.assertNotIn("node.exe", iss.lower())

    def test_payload_verifier_detects_binary_tampering(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            binary = root / "input.exe"
            version = root / "VERSION.json"
            fake_pe(binary)
            version_manifest(version)
            result = package.prepare(
                binary,
                version,
                root / "work",
                root / "out",
                SOURCE_COMMIT,
            )
            stage = Path(result["stage"])
            (stage / package.EXECUTABLE_NAME).write_bytes(b"MZtampered")
            with self.assertRaisesRegex(package.PackageError, "hash mismatch"):
                package.verify_install(stage, SOURCE_COMMIT)

    def test_rejects_non_pe_binary_and_invalid_source_commit(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            binary = root / "input.exe"
            version = root / "VERSION.json"
            binary.write_bytes(b"not-pe")
            version_manifest(version)
            with self.assertRaisesRegex(package.PackageError, "Windows PE"):
                package.prepare(binary, version, root / "work", root / "out", SOURCE_COMMIT)
            fake_pe(binary)
            with self.assertRaisesRegex(package.PackageError, "40-character Git SHA"):
                package.prepare(binary, version, root / "work", root / "out", "short")


if __name__ == "__main__":
    unittest.main()
