from __future__ import annotations

import importlib.util
from pathlib import Path
import tempfile
import unittest

MODULE_PATH = Path(__file__).resolve().parents[1] / "scripts" / "ci" / "rust_cyclonedx.py"
spec = importlib.util.spec_from_file_location("rust_cyclonedx", MODULE_PATH)
assert spec and spec.loader
rust_cyclonedx = importlib.util.module_from_spec(spec)
spec.loader.exec_module(rust_cyclonedx)


class RustCycloneDxTests(unittest.TestCase):
    def test_builds_reachable_graph_and_registry_checksums(self) -> None:
        root_id = "path+file:///repo/apps/desktop#hermes-local@0.18.62"
        core_id = "path+file:///repo/crates/hermes-core#0.18.62"
        serde_id = "registry+https://github.com/rust-lang/crates.io-index#serde@1.0.229"
        unused_id = "registry+https://github.com/rust-lang/crates.io-index#unused@1.0.0"
        metadata = {
            "packages": [
                {
                    "id": root_id,
                    "name": "hermes-local",
                    "version": "0.18.62",
                    "source": None,
                    "license": "MIT",
                    "repository": "https://github.com/xdCloudy/Hermes-Local",
                },
                {
                    "id": core_id,
                    "name": "hermes-core",
                    "version": "0.18.62",
                    "source": None,
                    "license": "MIT",
                    "repository": "https://github.com/xdCloudy/Hermes-Local",
                },
                {
                    "id": serde_id,
                    "name": "serde",
                    "version": "1.0.229",
                    "source": "registry+https://github.com/rust-lang/crates.io-index",
                    "license": "MIT OR Apache-2.0",
                    "repository": "https://github.com/serde-rs/serde",
                },
                {
                    "id": unused_id,
                    "name": "unused",
                    "version": "1.0.0",
                    "source": "registry+https://github.com/rust-lang/crates.io-index",
                },
            ],
            "resolve": {
                "nodes": [
                    {
                        "id": root_id,
                        "deps": [
                            {
                                "name": "hermes_core",
                                "pkg": core_id,
                                "dep_kinds": [{"kind": None, "target": None}],
                            },
                            {
                                "name": "unused",
                                "pkg": unused_id,
                                "dep_kinds": [{"kind": "dev", "target": None}],
                            },
                        ],
                    },
                    {"id": core_id, "deps": [{"name": "serde", "pkg": serde_id}]},
                    {"id": serde_id, "deps": []},
                    {"id": unused_id, "deps": []},
                ]
            },
        }
        with tempfile.TemporaryDirectory() as temp:
            lock = Path(temp) / "Cargo.lock"
            lock.write_text(
                """version = 4

[[package]]
name = "hermes-local"
version = "0.18.62"

[[package]]
name = "hermes-core"
version = "0.18.62"

[[package]]
name = "serde"
version = "1.0.229"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"

[[package]]
name = "unused"
version = "1.0.0"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
""",
                encoding="utf-8",
            )
            sbom = rust_cyclonedx.build_sbom(
                metadata,
                lock,
                "hermes-local",
                "2026-08-11T17:00:00Z",
            )

        self.assertEqual(sbom["bomFormat"], "CycloneDX")
        self.assertEqual(sbom["specVersion"], "1.6")
        self.assertEqual(sbom["metadata"]["component"]["name"], "hermes-local")
        names = [component["name"] for component in sbom["components"]]
        self.assertEqual(names, ["hermes-core", "serde"])
        serde = next(
            component for component in sbom["components"] if component["name"] == "serde"
        )
        self.assertEqual(serde["purl"], "pkg:cargo/serde@1.0.229")
        self.assertEqual(serde["hashes"][0]["content"], "a" * 64)
        self.assertNotIn("unused", names)

        root_ref = rust_cyclonedx._bom_ref(root_id)
        core_ref = rust_cyclonedx._bom_ref(core_id)
        root_dependencies = next(
            item for item in sbom["dependencies"] if item["ref"] == root_ref
        )
        self.assertEqual(root_dependencies["dependsOn"], [core_ref])

    def test_rejects_missing_unique_root(self) -> None:
        metadata = {"packages": [], "resolve": {"nodes": []}}
        with tempfile.TemporaryDirectory() as temp:
            lock = Path(temp) / "Cargo.lock"
            lock.write_text("version = 4\n", encoding="utf-8")
            with self.assertRaises(rust_cyclonedx.SbomError):
                rust_cyclonedx.build_sbom(
                    metadata,
                    lock,
                    "hermes-local",
                    "2026-08-11T17:00:00Z",
                )


if __name__ == "__main__":
    unittest.main()
