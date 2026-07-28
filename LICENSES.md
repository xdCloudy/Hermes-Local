# Hermes Local dependency licences

Generated/reviewed: 2026-07-28

Hermes Agent and the Hermes Launcher source are MIT-licensed. The local integration code is maintained as a patch series against that upstream work. Model licensing and provenance are recorded separately in the model manifest and `VERSION.json`.

## Machine-readable inventories

- `security\sbom\node-launcher.cdx.json` — CycloneDX 1.6, 616 Node components.
- `security\sbom\python-runtime.cdx.json` — CycloneDX 1.6, 127 Python dependency components.
- `security\sbom\node-licenses.json` — direct production Node inventory.
- `security\sbom\python-licenses.json` — installed Python licence metadata.

## Summary

The installed Python environment is predominantly MIT, Apache-2.0, BSD, PSF, and MPL-2.0 software. One metadata-unknown distribution, `agent-client-protocol` 0.9.0, was verified against its upstream repository as Apache-2.0.

The packaged Node graph is predominantly MIT (539 components), ISC (38), MPL-2.0 (12), Apache-2.0 (8), and BSD (11). Three entries merit explicit notice:

- `@vscode/codicons` 0.0.45 — CC-BY-4.0.
- `gsap` 3.15.0 — GSAP Standard “no charge” licence; this is not an OSI licence. The official upstream Desktop dependency is retained for the local build. Recheck its redistribution terms before commercial redistribution or bundling into a separately sold product.
- `robust-predicates` 3.0.3 — Unlicense.

Two Node components omit licence metadata in their package records but inherit the repository licence:

- private `hermes-launcher` 0.18.0 — MIT via the NousResearch/hermes-agent repository.
- private `@hermes/shared` 0.0.0 — MIT via the same repository.

`khroma` 2.1.0 omits package licence metadata; its upstream repository licence is MIT.

This file is an engineering inventory, not legal advice. Preserve upstream licence/notice files when redistributing the installer.
