# Contributing to Hermes Local

[← Project home](README.md) · [Documentation](docs/README.md) ·
[Development setup](docs/DEVELOPMENT.md) ·
[Roadmap](README.md#roadmap)

Hermes Local welcomes contributions to Windows engineering, local inference,
benchmarking, security, UX, documentation and testing. The best contributions
are focused, evidence-backed and preserve the project's local-first trust model.

## Find work

- Start with
  [good first issues](https://github.com/xdCloudy/Hermes-Local/issues?q=is%3Aissue+is%3Aopen+label%3A%22good+first+issue%22)
  for small, bounded changes.
- Browse
  [help-wanted issues](https://github.com/xdCloudy/Hermes-Local/issues?q=is%3Aissue+is%3Aopen+label%3A%22help+wanted%22)
  for work with defined acceptance criteria.
- Use the [roadmap](README.md#roadmap) to understand sequencing and the
  [GitHub Project](https://github.com/users/xdCloudy/projects/1) for live state.
- Open a feature request before investing in a large behavior or architecture
  change.

If an issue is blocked, marked `needs-design`, security-sensitive or likely to
change persistent data, agree the approach with maintainers before coding.

## Ground rules

- Keep runtime services loopback-only by default.
- Do not add Docker or WSL as a prerequisite.
- Never commit models, runtimes, tokens, credentials, conversations or user
  data.
- Preserve user data during setup, repair, update, rollback and uninstall.
- Change the client directly in `apps/desktop`. Keep upstream Agent runtime
  changes focused and refresh the harness-only series in
  `source/hermes-launcher/patches`; Desktop paths are forbidden there.
- Do not weaken Electron isolation, CSP, navigation checks, write approvals or
  process argument validation.
- Resolve paths from the project root and support paths containing spaces.
- Do not turn historical benchmark hardware into a runtime requirement.

Read [AGENTS.md](AGENTS.md) for the complete engineering boundaries and
[docs/SECURITY.md](docs/SECURITY.md) for trust boundaries.

## Development flow

1. Fork the repository and clone your fork.
2. Create a focused branch from current `main`.
3. Follow the [development guide](docs/DEVELOPMENT.md) to reconstruct the pinned
   Hermes Agent source and install dependencies.
4. Make one coherent change. Update user, architecture, security and benchmark
   documentation when their contracts change.
5. Run the smallest relevant tests, then the broader gates required by the
   change.
6. Open a pull request that explains user impact, risk, exact verification and
   recovery behavior.
7. Address review feedback with additional commits; do not rewrite reviewed
   history unless a maintainer asks.

## Validation guide

| Change | Minimum validation |
|---|---|
| Markdown only | Links, anchors, Mermaid syntax and rendered layout |
| PowerShell | Parser validation plus the affected script's safe test mode |
| Configuration/schema | Parser validation, schema validation and migration/default tests |
| Launcher UI | Typecheck, lint, relevant Vitest and narrow-width visual check |
| Lifecycle/update/data | Full affected workflow, failure path and data-preservation check |
| Inference/performance | Functional test plus reproducible before/after benchmark |
| Security boundary | Regression test, threat-model review and updated security evidence |
| Packaging/release | Packaged executable and installer workflow on native Windows |

Runtime or packaging behavior should also pass:

```powershell
& '.\Test-Hermes-Local.ps1' -NonInteractive
```

The full native QA entrypoint is:

```powershell
& '.\scripts\qa\Invoke-FullFunctionalQA.ps1' -Scope Full
```

## Documentation changes

The root README is a product landing page. Put detailed procedures in
[`docs`](docs/README.md), link them from the appropriate overview, and avoid
duplicating a long explanation in multiple places.

The live README dashboard and roadmap are generated. Edit only content outside:

```text
<!-- BEGIN GENERATED STATUS -->
<!-- END GENERATED STATUS -->
<!-- BEGIN GENERATED ROADMAP -->
<!-- END GENERATED ROADMAP -->
```

Roadmap stage definitions live in `docs/roadmap.json`. Test generation without
writing the README:

```powershell
& '.\scripts\update-readme.ps1' -DryRun -Verbose
```

For an offline, deterministic run, add
`-FixturePath '.\tests\fixtures\readme-api.json'`.

## Pull request checklist

- [ ] The change is focused and linked to an issue when appropriate.
- [ ] Relevant tests pass and exact commands/results are in the PR.
- [ ] Failure, rollback and user-data behavior are documented.
- [ ] No secrets, models, runtimes, private data or unredacted logs are present.
- [ ] Documentation and screenshots reflect user-visible behavior.
- [ ] Security and benchmark evidence are updated where relevant.

Do not attach raw logs to a public issue. Use
`Export-Hermes-Diagnostics.ps1`, inspect its privacy manifest, and share only a
redacted bundle or the minimum relevant excerpt.
