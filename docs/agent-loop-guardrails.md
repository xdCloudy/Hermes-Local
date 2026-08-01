# Hermes Agent loop guardrails

Hermes Agent already detects repeated tool failures, but the upstream runtime treats a hard guardrail decision as a terminal event for the entire user turn. That prevents an unbounded tool loop, but it can also end a large autonomous task before the model completes independent work or returns a useful partial result.

Hermes-Local includes a maintenance installer that changes this behavior to bounded **block-and-continue** recovery.

## Behavior

When a recoverable guardrail returns `action="block"`:

1. The blocked tool call is not executed.
2. The synthetic guardrail result remains in the conversation.
3. Failure counters and tool caps remain active.
4. Hermes gives the model another inference turn so it can change strategy, use another tool, finish independent work, or return a partial result.

A true `action="halt"` remains terminal. To prevent the model from repeatedly ignoring blocked-tool results, the installer permits three recoveries per user turn by default. A fourth recoverable block is promoted to a controlled halt.

## Install

Stop Hermes Local and close any active TUI or Desktop session before applying the runtime patch:

```powershell
Set-Location 'D:\Hermes-Local'

pwsh.exe -NoProfile -ExecutionPolicy Bypass `
    -File '.\scripts\maintenance\Install-Hermes-GuardrailBlockContinue.ps1' `
    -RepositoryRoot 'D:\Hermes-Local' `
    -RecoveryLimit 3 `
    -Verbose
```

The installer resolves the active editable Hermes Agent modules, creates timestamped backups under `artifacts\guardrail-block-continue-*`, patches the runtime, compiles the affected Python files, and runs two mocked end-to-end smoke tests.

Expected validation output:

```text
BLOCK-AND-CONTINUE SMOKE TEST: PASS
```

Restart Hermes Local afterward:

```powershell
Set-Location 'D:\Hermes-Local'

& '.\Stop-Hermes-Local.ps1' -NonInteractive
& '.\Start-Hermes-Local.ps1' -NonInteractive
```

Fully reopen the TUI or Desktop client so Python reloads the modified modules.

## Expected runtime status

A recoverable block should produce a status similar to:

```text
Tool guardrail blocked browser_navigate: loop_browser_navigation_cap; continuing recovery 1/3
```

The model should then receive another inference turn instead of ending the request immediately.

If it ignores four block results in the same user turn, Hermes stops tool execution and asks for the best validated partial result.

## Compatibility and rollback

The installer is anchor-based and fails closed when the installed Hermes Agent source no longer matches the expected structure. It does not force an uncertain patch onto an incompatible upstream version.

Every run creates a timestamped backup containing the original copies of:

- `agent/tool_guardrails.py`
- `agent/turn_context.py`
- `agent/conversation_loop.py`

Hermes Agent updates may replace these runtime files. Re-run the installer after an update until the behavior is represented in the maintained Hermes-Local integration patch series or accepted upstream.
