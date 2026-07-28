# Contributing

Contributions that preserve the Windows-native, local-first security model are
welcome.

## Ground rules

- Keep runtime services loopback-only by default.
- Do not add Docker or WSL as a prerequisite.
- Never commit models, runtimes, tokens, credentials, conversations or user
  data.
- Preserve user data during setup, repair, update, rollback and uninstall.
- Keep changes to upstream Hermes as focused commits and refresh the ordered
  patch series in `source/hermes-launcher/patches`.
- Do not weaken Electron isolation, CSP, navigation checks, write approvals or
  process argument validation.

## Development workflow

1. Fork the repository and create a focused branch.
2. Follow [the development guide](docs/DEVELOPMENT.md).
3. Run the relevant PowerShell, Hermes and Desktop tests.
4. Run `Test-Hermes-Local.ps1` when changing runtime or packaging behavior.
5. Update documentation, security findings and benchmark evidence when the
   behavior or performance envelope changes.
6. Open a pull request explaining the user impact and verification performed.

Do not attach raw logs to a public issue. Use
`Export-Hermes-Diagnostics.ps1`, confirm its privacy manifest, and share only
the redacted bundle or the minimum relevant excerpt.
