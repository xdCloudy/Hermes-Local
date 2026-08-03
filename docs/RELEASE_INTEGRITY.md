# Release integrity, SBOMs and provenance

Hermes Local releases use a fail-closed integrity chain. Release artifacts are not eligible for installer or Update Centre promotion until their manifest, size, SHA-256 digest, required Authenticode signature, CycloneDX SBOM references and GitHub artifact attestations have verified.

## Release assets

A release publishes:

- every installer, portable launcher, integration package, runtime bundle, offline bundle and updater package;
- `release-manifest.json`, validated against `config/schemas/release-manifest.schema.json`;
- `SHA256SUMS` covering every artifact and SBOM;
- scoped CycloneDX JSON SBOMs for launcher/Node, Python, native runtime and aggregate bundle content;
- GitHub build-provenance attestations for the manifest and every release artifact;
- SBOM attestations where the workflow can bind a scoped SBOM to its artifact;
- Authenticode signatures and trusted timestamps when the protected release environment has a sustainable certificate configured.

Model weights remain separate. Their source repository, revision, size and SHA-256 are recorded by model manifests and `VERSION.json`; they are not presented as if they were built by Hermes Local.

## Trusted build workflow

`.github/workflows/release-integrity.yml` has two paths:

1. Pull requests run only the manifest/verifier unit tests and schema checks. They never receive signing credentials or OIDC attestation permission.
2. Tag/manual release jobs run only from `main` or a `v*` tag whose commit is reachable from `main`, inside the protected `stable-release` environment with least-privilege `contents`, `id-token`, `attestations` and `artifact-metadata` permissions.

Stable releases should require environment approval and deployment branch/tag restrictions. The optional PFX and password secrets are exposed only to the Authenticode signing step. The decoded certificate is deleted after signing. Compromise response is to disable the environment, revoke/rotate the certificate, remove affected release assets/attestations, and publish a replacement release with a new manifest.

## Creating metadata locally

After artifacts and SBOMs exist under `dist`:

```powershell
python .\scripts\ci\release_integrity.py create `
  --root .\dist `
  --output .\dist\release-manifest.json `
  --version-manifest .\VERSION.json `
  --repository xdCloudy/Hermes-Local `
  --source-commit (git rev-parse HEAD) `
  --workflow xdCloudy/Hermes-Local/.github/workflows/release-integrity.yml `
  --run-id local `
  --artifact 'Hermes-Launcher-*.exe' `
  --artifact 'Hermes-Launcher-*.blockmap' `
  --artifact 'Hermes Launcher.exe' `
  --artifact package-manifest.json `
  --sbom launcher=sbom/launcher.cdx.json `
  --dependency-lock node=.\source\hermes-agent\package-lock.json `
  --dependency-lock python=.\source\hermes-agent\uv.lock
```

Local metadata is not a release until the protected workflow has attested it.

## Verification and promotion

Online verification:

```powershell
python .\scripts\ci\release_integrity.py verify `
  --manifest .\staging\release-manifest.json `
  --artifact-root .\staging `
  --require-attestation `
  --report .\build\release-integrity\LATEST.json
```

The verifier constrains every manifest path to the staging root, validates size before hashing, validates SHA-256 and `SHA256SUMS`, parses each CycloneDX document, verifies required Authenticode signatures, and invokes `gh attestation verify` with the exact repository, signer workflow and source commit. Self-hosted provenance is rejected.

For offline installations, use the `dist/attestations` JSONL bundles published by the release workflow and the current trusted root, then use `--attestation-bundle-dir` and `--trusted-root`. Missing bundles, tools, metadata, signatures or hashes are hard failures.

`Update-Hermes-Local.ps1` accepts `-ReleaseManifestPath`, `-ArtifactRoot`, `-AttestationBundleDirectory` and `-TrustedRootPath`. Installer applies require release metadata. Verification completes before the transactional update orchestrator starts, so a failed package never replaces the active installation.

## Authenticode fallback

Until `HERMES_SIGNING_CERTIFICATE_PFX` and `HERMES_SIGNING_CERTIFICATE_PASSWORD` are configured in the protected environment, releases use SHA-256 plus GitHub attestations and must be described as **integrity/provenance verified but not publisher-signed**. Windows may show an unknown-publisher warning. Once a certificate is configured, executable artifacts are marked `authenticodeRequired: true`; verification then fails closed unless the signature is valid and timestamped.

## Diagnostics

The verifier writes `build/release-integrity/LATEST.json` on both success and failure, including a safe failure reason when verification is blocked. Diagnostic exports include the release manifest, checksum file and latest verification report without including signing credentials, private keys or environment values.


## Credential rotation and revocation

Keep signing secrets only in the protected `stable-release` environment. Rotate the PFX before expiry or immediately after suspected exposure, retain old public certificates so historical releases remain verifiable, and document the first release signed by each new thumbprint. On compromise, disable the environment, revoke the certificate with its issuer, remove affected release assets, publish a security advisory and replacement release, and preserve the revoked manifest/attestation evidence for incident review rather than silently rewriting history.
