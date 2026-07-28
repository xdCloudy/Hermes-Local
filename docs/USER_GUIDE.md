# User guide

## Start and stop

Start the default workstation:

```powershell
& 'D:\Hermes-Local\Start-Hermes-Local.ps1' -Profile Daily -NonInteractive
& 'D:\Hermes-Local\dist\Hermes Launcher.exe'
```

Use **Start all**, **Stop all**, **Restart all** or **Recovery mode** from
Home, or use the PowerShell scripts. Startup validates configuration and model
integrity, starts the model server, waits for its structured health and model
inventory, then starts Hermes and its dashboard. Shutdown is the reverse.

## Home and Services

Home shows the actual supervisor state, active profile, context limit, service
PIDs, resource metrics, benchmark summary and recent errors. Services adds
executable, working directory, port, uptime, health, exit and restart details.
An open port alone is not considered healthy.

If a managed process crashes three consecutive health polls, the supervisor
stops the stack and restarts it with exponential backoff. A bounded restart
window prevents a permanent crash loop.

## Chat

Chat is the official Hermes Desktop conversation surface using the local
Laguna endpoint. It supports streaming, tool activity, reasoning display,
sessions, attachments and previews where supported, interrupt/stop, approval
prompts and the active model profile.

The local model is intentionally configured with native tool-call enforcement.
Do not disable enforcement to work around a malformed request; use Safe
Recovery, capture the error and retest the template instead.

## TUI and CLI

TUI opens the real Hermes TUI inside a Windows pseudo-terminal. The connection
indicator includes the child PID. Resize, ANSI colours, keyboard input,
copy/paste and scrollback are supported. If the TUI exits, use **Restart TUI**.

A standalone CLI/TUI can be opened with the managed executable:

```powershell
$env:HERMES_HOME = 'D:\Hermes-Local\data\hermes'
& 'D:\Hermes-Local\runtimes\python\hermes\Scripts\hermes.exe'
```

The default terminal working directory is `D:\Hermes-Local\data\user`, not the
installation source.

## Web Dashboard

The unified Hermes backend serves the official dashboard at
`http://127.0.0.1:9119`. Use Web Dashboard in the launcher to embed it or open
it in the default browser. The launcher validates the exact loopback URL and
will not navigate a privileged view to an arbitrary remote page.

## Profiles

Profiles are structured in `config\profiles\profiles.json` and validated
against `profiles.schema.json`.

| Profile | Use |
|---|---|
| Daily | Measured 64K quality-first default |
| Research | Stable 64K research with prompt-cache reuse |
| Deep Research | Measured 80K context with extra VRAM reserve |
| Coding | Responsive 48K tool-heavy work |
| Maximum Context | Experimental 128K; never selected automatically |
| Benchmark | Deterministic 32K measurement with seed 3407 |
| Safe Recovery | Conservative 8K CPU diagnostic mode |

Switching profile restarts the model so the context, KV, batch and placement
settings actually take effect. Export or back up the JSON before substantial
manual edits.

## Research and coding

Use Research for normal dossiers and multi-source analysis. Use Deep Research
only when the larger live context is worth its extra prefill cost. Keep a
source ledger in the project and ask Hermes to distinguish sourced facts,
conflicts and inferences.

Coding provides a smaller context budget for fast tool loops. File and terminal
writes remain approval-gated. Commands display their working directory and run
locally; elevation is not hidden.

## Tasks, cron and delegation

Tasks shows the local release/task ledger and provides entry points into the
official structured task surfaces. Cron jobs can be created, listed, run,
paused, resumed, edited and removed through the Hermes cron tool/UI. Schedules
and outcomes remain in `D:\Hermes-Local\data`.

Delegation is limited to one child and one spawn level. Subagents are not
auto-approved and do not inherit arbitrary MCP toolsets.

## Skills and memory

Skills are local documents and supporting files. Skill writes require
approval. On Windows, inline skill shell expansion explicitly selects native
Git Bash and rejects the WSL launcher; supporting file references remain
portable while executable paths remain native.

Built-in local memory and session search are installed. Memory writes require
approval. External memory providers are disabled unless the user deliberately
configures an account-backed plugin.

## Logs and diagnostics

Logs provides redacted, bounded views of supervisor, model, Hermes, dashboard,
security and launcher output. Raw files are under `D:\Hermes-Local\logs`.

Export a privacy-preserving diagnostic archive with:

```powershell
& 'D:\Hermes-Local\Export-Hermes-Diagnostics.ps1' -NonInteractive
```

Tokens, passwords, cookies, complete environment values, conversations and
private file contents are excluded.

## Security

Security shows scan currency, accepted residuals, loopback status, Electron
hardening, SBOM/report paths and the scan action. To re-run:

```powershell
& 'D:\Hermes-Local\Security-Scan-Hermes-Local.ps1' -NonInteractive
```

Never publish ports 8011 or 9119 through router forwarding, a reverse proxy or
a permissive firewall rule. There is no supported LAN mode in this release.

## Backup, update and recovery

Create a backup:

```powershell
& 'D:\Hermes-Local\Backup-Hermes-Local.ps1' -Name before-change -NonInteractive
```

Restore a selected archive only after reading its contents:

```powershell
& 'D:\Hermes-Local\Restore-Hermes-Local.ps1' -BackupPath 'D:\Hermes-Local\backups\<archive>.zip' -NonInteractive
```

Restore automatically creates a pre-restore safety backup and restarts the
stack. See [UPDATE_AND_ROLLBACK.md](UPDATE_AND_ROLLBACK.md) for component
updates and [TROUBLESHOOTING.md](TROUBLESHOOTING.md) for recovery cases.
