# Troubleshooting

## Begin with the health test

```powershell
& 'D:\Hermes-Local\Test-Hermes-Local.ps1' -NonInteractive
```

For a faster service-only check:

```powershell
& 'D:\Hermes-Local\Test-Hermes-Local.ps1' -Quick -NonInteractive
```

Inspect `D:\Hermes-Local\logs\diagnostics\latest-test.json` and the Logs
surface. Never paste the DPAPI token store or raw session database into a bug
report.

## Stack will not start

1. Check `D:\Hermes-Local\data\runtime\status.json`.
2. Inspect `logs\supervisor`, `logs\model-server` and `logs\hermes`.
3. Confirm ports are not owned by unrelated processes:

```powershell
Get-NetTCPConnection -State Listen |
  Where-Object LocalPort -In 8011, 9119 |
  Select-Object LocalAddress, LocalPort, OwningProcess
```

Both addresses must be `127.0.0.1`. Do not kill an unrelated owner blindly.
Resolve its executable first.

Use:

```powershell
& 'D:\Hermes-Local\Restart-Hermes-Local.ps1' -Profile Safe Recovery -NonInteractive
```

Safe Recovery uses an 8K CPU profile to separate GPU/context problems from
general configuration problems.

## CUDA OOM or unstable long context

Switch from Maximum Context or Deep Research to Daily. Close GPU-heavy
applications. Confirm the selected JSON profile has the recorded VRAM reserve.
Do not reduce the model below Q4_K_M just to force a larger context.

If Daily still fails, use Safe Recovery and rerun diagnostics. Rebuild
llama.cpp only from the pinned source:

```powershell
& 'D:\Hermes-Local\Setup-Hermes-Local.ps1' `
  -SkipHermesDependencies -SkipModel -SkipLauncherBuild -NonInteractive
```

## Model integrity failure

Run the full test. If the size or SHA does not match, do not launch the file.
Move the suspect partial file to a quarantine location beneath `temp` and
rerun setup. The downloader resumes a valid partial file and validates the
final 20,274,300,032-byte SHA.

## Missing or damaged Python/Node dependency

```powershell
& 'D:\Hermes-Local\Repair-Hermes-Local.ps1' -NonInteractive
```

Repair force-reinstalls locked dependencies. This is stronger than a normal
`uv sync`, which can consider a damaged package present without restoring a
missing individual file.

## Launcher does not attach

Make sure the stack itself passes the quick test. The portable executable is a
self-extractor, so Playwright/Electron development harnesses must attach to
`apps\desktop\release\win-unpacked\Hermes Launcher.exe`, not launch the
portable wrapper through `_electron.launch`.

For normal use, `dist\Hermes Launcher.exe` is correct.

If the packaged application shows a Windows reputation warning, verify its
SHA against `dist\package-manifest.json`. This local build is not
Authenticode-signed.

## TUI disconnected

Open TUI and use its restart control. If it exits repeatedly, inspect launcher
and Hermes logs, then launch the managed CLI directly:

```powershell
$env:HERMES_HOME = 'D:\Hermes-Local\data\hermes'
& 'D:\Hermes-Local\runtimes\python\hermes\Scripts\hermes.exe'
```

The launcher never grants the renderer an arbitrary shell command.

## Browser tools unavailable

Confirm the local Chromium runtime exists and run Repair. Browser-driven
search/navigation is enabled, but the zero-key `web_search` API tool is not
promoted. Paid search keys are intentionally absent.

Browser failures against loopback, link-local or private destinations can be
the SSRF guard working as intended.

## Memory or skill write rejected

Both writes require approval. Approve the exact operation in Chat/TUI. A
denial is not a dependency failure. Do not change `write_approval` to false to
silence the prompt.

## Offline restart

Core inference is offline-capable. If it tries to reach the network, verify
that no external provider, network voice engine, remote MCP or messaging
plugin was enabled. Built-in themes contain no Google Font dependency.

## Backup or restore failure

Do not edit a backup ZIP in place. Verify the `.sha256` sidecar and ensure the
archive resides under `D:\Hermes-Local\backups`. Restore rejects unsafe
absolute or `..` entries. If a restore stops after creating its safety backup,
preserve both archives and inspect `logs\backup`.

## Export diagnostics

```powershell
& 'D:\Hermes-Local\Export-Hermes-Diagnostics.ps1' -NonInteractive
```

The archive is safe for technical review only after the included privacy
manifest says tokens, environment values, conversations and private files
were omitted.
