# Operations reference

[← Documentation home](README.md) · [Project home](../README.md) ·
[Troubleshooting](TROUBLESHOOTING.md) ·
[Update and rollback](UPDATE_AND_ROLLBACK.md)

This page collects the detailed runtime and maintenance reference formerly held
on the project landing page.

## Runtime layout

| Component | Portable location |
|---|---|
| Project root | Directory containing `README.md` |
| Hermes checkout | `source\hermes-agent` |
| Hermes source pin override | `config\launcher\source-overrides.json` |
| Hermes user state | `data\hermes` |
| Default workspace | `data\user` |
| User settings | `config\launcher\user-settings.json` |
| Model catalog | `models\manifests\*.json` plus user registrations |
| llama.cpp build | `runtimes\llama.cpp\build` |
| Packaged launcher | `dist\Hermes Launcher.exe` |

The repository has no required drive letter, model, GPU, CUDA architecture,
port pair or fixed CPU tuning. Machine-dependent defaults are resolved on the
current workstation. User selections live in the ignored
`config\launcher\user-settings.json` and are included in backups.

## Service boundaries

The default endpoints are:

| Service | Default endpoint |
|---|---|
| OpenAI-compatible model API | `http://127.0.0.1:8011/v1` |
| Hermes gateway | `http://127.0.0.1:9119` |

Both ports are configurable. Hosts are restricted to IPv4 or IPv6 loopback.
The API token is random, protected for the current Windows user with DPAPI and
passed to owned processes without command-line exposure.

## Lifecycle commands

Run commands from the repository root in a normal, non-elevated PowerShell 7
session.

| Action | Command |
|---|---|
| Start selected configuration | `& '.\Start-Hermes-Local.ps1' -NonInteractive` |
| Start one profile | `& '.\Start-Hermes-Local.ps1' -Profile 'Coding' -NonInteractive` |
| Stop | `& '.\Stop-Hermes-Local.ps1' -NonInteractive` |
| Restart | `& '.\Restart-Hermes-Local.ps1' -NonInteractive` |
| Health and integration test | `& '.\Test-Hermes-Local.ps1' -NonInteractive` |
| Repair | `& '.\Repair-Hermes-Local.ps1' -NonInteractive` |
| Uninstall | `& '.\Uninstall-Hermes-Local.ps1' -NonInteractive` |

## Backup, restore and diagnostics

| Action | Command |
|---|---|
| Create named backup | `& '.\Backup-Hermes-Local.ps1' -Name manual -NonInteractive` |
| Restore | `& '.\Restore-Hermes-Local.ps1' -NonInteractive` |
| Export redacted diagnostics | `& '.\Export-Hermes-Diagnostics.ps1' -NonInteractive` |
| Run security scan | `& '.\Security-Scan-Hermes-Local.ps1' -NonInteractive` |

Review a diagnostic bundle's privacy manifest before sharing it. Never publish
raw tokens, conversations, paths containing personal identifiers or unredacted
logs.

## Updates

| Action | Command |
|---|---|
| Check all component updates | `& '.\Update-Hermes-Local.ps1' -Mode Check -NonInteractive` |
| Check Hermes Agent | `& '.\Update-Hermes-Agent.ps1' -Mode Check` |
| Apply Hermes Agent update | `& '.\Update-Hermes-Agent.ps1' -Mode Apply` |
| Roll back Hermes Agent | `& '.\Update-Hermes-Agent.ps1' -Mode Rollback` |

Windows users can also run `Update-Hermes-Agent.cmd`. The updater stages
upstream code away from the active installation, applies the Hermes Local patch
series, backs up state, rebuilds dependencies and the launcher, runs health
checks, and restores the previous installation automatically if promotion
fails.

Do not use Hermes Agent's in-chat `/update` command for a Hermes Local checkout;
it does not understand the integration patch layer. See
[Update and rollback](UPDATE_AND_ROLLBACK.md) for the complete contract.

## Validation and evidence

| Scope | Command or evidence |
|---|---|
| PowerShell syntax | `& '.\scripts\qa\Test-PowerShellSyntax.ps1'` |
| Full functional QA | `& '.\scripts\qa\Invoke-FullFunctionalQA.ps1' -Scope Full` |
| Benchmark suite | `& '.\Benchmark-Hermes-Local.ps1'` |
| Current QA report | [Full functional QA report](../reports/qa/FULL_FUNCTIONAL_QA_REPORT.md) |
| Acceptance summary | [Acceptance results](ACCEPTANCE_RESULTS.md) |

Full functional QA writes timestamped JSON, Markdown, JUnit, stdout and stderr
evidence beneath `temp\qa-runs` and refreshes machine-readable inventories in
`reports\qa`.
