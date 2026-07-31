#requires -Version 7.0
<#
.SYNOPSIS
    Changes Hermes Agent tool-loop guardrails from block-and-finish to bounded
    block-and-continue recovery.

.DESCRIPTION
    Recoverable guardrail decisions (action="block") now:
      1. Skip the blocked tool call.
      2. Keep the synthetic tool result in conversation history.
      3. Clear only the terminal signal, preserving failure/cap counters.
      4. Continue the model loop so it can change strategy, use another tool,
         complete independent work, or return a partial answer.

    Terminal decisions (action="halt") retain the existing stop behavior.

    To prevent a model from repeatedly ignoring blocked-tool results, only
    three recoveries are allowed per user turn by default. A later recoverable
    block is promoted to a controlled terminal stop.

.PARAMETER RepositoryRoot
    Hermes-Local repository root.

.PARAMETER RecoveryLimit
    Number of recoverable guardrail blocks allowed in one user turn.
#>

[CmdletBinding()]
param(
    [string]$RepositoryRoot = 'D:\Hermes-Local',
    [ValidateRange(1, 10)]
    [int]$RecoveryLimit = 3
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Resolve-ExistingPath {
    param(
        [Parameter(Mandatory)]
        [string]$Path,
        [Parameter(Mandatory)]
        [string]$Description
    )

    if (-not (Test-Path -LiteralPath $Path)) {
        throw "$Description not found: $Path"
    }

    return (Resolve-Path -LiteralPath $Path).Path
}

function Invoke-Checked {
    param(
        [Parameter(Mandatory)]
        [string]$FilePath,
        [Parameter()]
        [string[]]$ArgumentList = @(),
        [Parameter()]
        [string]$Description = $FilePath
    )

    Write-Verbose "Executing: $FilePath $($ArgumentList -join ' ')"
    & $FilePath @ArgumentList
    if ($LASTEXITCODE -ne 0) {
        throw "$Description failed with exit code $LASTEXITCODE."
    }
}

$RepositoryRoot = Resolve-ExistingPath `
    -Path $RepositoryRoot `
    -Description 'Repository root'

Set-Location -LiteralPath $RepositoryRoot

$CommonModule = Resolve-ExistingPath `
    -Path (Join-Path $RepositoryRoot 'scripts\Common-Hermes.psm1') `
    -Description 'Hermes-Local common module'

Import-Module -Name $CommonModule -Force
Set-HermesProcessEnvironment

$PythonExe = Resolve-ExistingPath `
    -Path (Join-Path $RepositoryRoot 'runtimes\python\hermes\Scripts\python.exe') `
    -Description 'Hermes Python runtime'

$ModuleProbe = @'
import agent.conversation_loop as conversation_loop
import agent.tool_guardrails as tool_guardrails
import agent.turn_context as turn_context

print(tool_guardrails.__file__)
print(turn_context.__file__)
print(conversation_loop.__file__)
'@

$ResolvedModules = @(& $PythonExe -c $ModuleProbe)
if ($LASTEXITCODE -ne 0 -or $ResolvedModules.Count -ne 3) {
    throw 'Could not resolve the three active Hermes Agent module paths.'
}

$ToolGuardrailsPath = Resolve-ExistingPath `
    -Path $ResolvedModules[0].Trim() `
    -Description 'Active tool_guardrails.py'

$TurnContextPath = Resolve-ExistingPath `
    -Path $ResolvedModules[1].Trim() `
    -Description 'Active turn_context.py'

$ConversationLoopPath = Resolve-ExistingPath `
    -Path $ResolvedModules[2].Trim() `
    -Description 'Active conversation_loop.py'

$Stamp = Get-Date -Format 'yyyyMMdd-HHmmss'
$BackupRoot = Join-Path $RepositoryRoot "artifacts\guardrail-block-continue-$Stamp"
New-Item -ItemType Directory -Path $BackupRoot -Force | Out-Null

foreach ($Path in @(
    $ToolGuardrailsPath,
    $TurnContextPath,
    $ConversationLoopPath
) | Sort-Object -Unique) {
    $SafeName = ($Path -replace '[:\\\/]', '_').Trim('_')
    Copy-Item `
        -LiteralPath $Path `
        -Destination (Join-Path $BackupRoot $SafeName) `
        -Force
}

Write-Host "Backup created: $BackupRoot" -ForegroundColor Cyan
Write-Host "tool_guardrails.py: $ToolGuardrailsPath"
Write-Host "turn_context.py: $TurnContextPath"
Write-Host "conversation_loop.py: $ConversationLoopPath"

$PatcherPath = Join-Path $BackupRoot 'patch_block_and_continue.py'

$Patcher = @'
from __future__ import annotations

import pathlib
import sys

tool_guardrails_path = pathlib.Path(sys.argv[1])
turn_context_path = pathlib.Path(sys.argv[2])
conversation_loop_path = pathlib.Path(sys.argv[3])
recovery_limit = int(sys.argv[4])


def read(path: pathlib.Path) -> str:
    return path.read_text(encoding="utf-8")


def write(path: pathlib.Path, value: str) -> None:
    path.write_text(value, encoding="utf-8", newline="\n")


# 1. Clear only the stop signal while retaining counters.
tool_text = read(tool_guardrails_path)
tool_marker = "HERMES-LOCAL: recoverable guardrail clear"

if tool_marker not in tool_text:
    old = """    @property
    def halt_decision(self) -> ToolGuardrailDecision | None:
        return self._halt_decision

    def before_call(self, tool_name: str, args: Mapping[str, Any] | None) -> ToolGuardrailDecision:
"""
    new = """    @property
    def halt_decision(self) -> ToolGuardrailDecision | None:
        return self._halt_decision

    def clear_halt_decision(self) -> None:
        # HERMES-LOCAL: recoverable guardrail clear
        #
        # Clear only the terminal signal. Failure counts, no-progress hashes,
        # and per-turn caps remain intact so an exhausted strategy continues
        # to be blocked if the model tries it again.
        self._halt_decision = None

    def before_call(self, tool_name: str, args: Mapping[str, Any] | None) -> ToolGuardrailDecision:
"""
    if old not in tool_text:
        raise SystemExit(
            f"tool_guardrails.py patch anchor not found: {tool_guardrails_path}"
        )
    tool_text = tool_text.replace(old, new, 1)
    write(tool_guardrails_path, tool_text)
    print(f"patched: {tool_guardrails_path}")
else:
    print(f"already patched: {tool_guardrails_path}")


# 2. Reset the recovery counter at the beginning of every user turn.
turn_text = read(turn_context_path)
turn_marker = "HERMES-LOCAL: reset block-and-continue recovery budget"

if turn_marker not in turn_text:
    old = """    agent._tool_guardrails.reset_for_turn()
    agent._tool_guardrail_halt_decision = None
"""
    new = """    agent._tool_guardrails.reset_for_turn()
    agent._tool_guardrail_halt_decision = None
    # HERMES-LOCAL: reset block-and-continue recovery budget
    agent._tool_guardrail_recovery_count = 0
"""
    if old not in turn_text:
        raise SystemExit(
            f"turn_context.py patch anchor not found: {turn_context_path}"
        )
    turn_text = turn_text.replace(old, new, 1)
    write(turn_context_path, turn_text)
    print(f"patched: {turn_context_path}")
else:
    print(f"already patched: {turn_context_path}")


# 3. Recover from action=block; retain action=halt as terminal.
loop_text = read(conversation_loop_path)
loop_marker = "HERMES-LOCAL: bounded block-and-continue recovery"

if loop_marker not in loop_text:
    start_marker = (
        "                if agent._tool_guardrail_halt_decision is not None:\n"
    )
    start = loop_text.find(start_marker)
    if start < 0:
        raise SystemExit(
            f"conversation_loop.py start anchor not found: {conversation_loop_path}"
        )

    end_marker = "                    break\n"
    end = loop_text.find(end_marker, start)
    if end < 0:
        raise SystemExit(
            f"conversation_loop.py end anchor not found: {conversation_loop_path}"
        )
    end += len(end_marker)

    replacement = f"""                if agent._tool_guardrail_halt_decision is not None:
                    decision = agent._tool_guardrail_halt_decision

                    # HERMES-LOCAL: bounded block-and-continue recovery
                    #
                    # `block` means the specific call/strategy is refused, not
                    # that the whole task is over. The synthetic tool result is
                    # already in `messages`, so clear only the stop signal and
                    # let the model read the blocker on the next iteration.
                    #
                    # `halt` stays terminal. The bounded recovery budget also
                    # stops a model that repeatedly ignores blocked results.
                    if decision.action == "block":
                        recovery_count = (
                            int(
                                getattr(
                                    agent,
                                    "_tool_guardrail_recovery_count",
                                    0,
                                )
                            )
                            + 1
                        )
                        agent._tool_guardrail_recovery_count = recovery_count
                        recovery_limit = {recovery_limit}

                        if recovery_count <= recovery_limit:
                            agent._emit_status(
                                f"⚠️ Tool guardrail blocked "
                                f"{{decision.tool_name}}: {{decision.code}}; "
                                f"continuing recovery "
                                f"{{recovery_count}}/{{recovery_limit}}"
                            )
                            agent._tool_guardrail_halt_decision = None
                            try:
                                agent._tool_guardrails.clear_halt_decision()
                            except Exception:
                                logger.debug(
                                    "Could not clear recoverable tool "
                                    "guardrail state",
                                    exc_info=True,
                                )

                            # Continue the normal model loop. It can switch
                            # source/tool, finish independent work, or return
                            # the best validated partial answer.
                            continue

                    _turn_exit_reason = "guardrail_halt"
                    if decision.action == "block":
                        tool_name = decision.tool_name or "a tool"
                        final_response = (
                            f"I stopped retrying {{tool_name}} after "
                            f"{{recovery_count}} guardrail blocks in this "
                            "turn. The model repeatedly ignored blocked-tool "
                            "results; it must return the best validated "
                            "partial result instead of continuing tool use."
                        )
                    else:
                        final_response = (
                            agent._toolguard_controlled_halt_response(decision)
                        )

                    agent._emit_status(
                        f"⚠️ Tool guardrail halted "
                        f"{{decision.tool_name}}: {{decision.code}}"
                    )
                    messages.append(
                        {{"role": "assistant", "content": final_response}}
                    )

                    if final_response:
                        agent._safe_print(f"\\n{{final_response}}\\n")
                        if agent.stream_delta_callback:
                            try:
                                agent.stream_delta_callback(final_response)
                                agent.stream_delta_callback(None)
                            except Exception:
                                pass
                    break
"""

    loop_text = loop_text[:start] + replacement + loop_text[end:]
    write(conversation_loop_path, loop_text)
    print(f"patched: {conversation_loop_path}")
else:
    print(f"already patched: {conversation_loop_path}")
'@

Set-Content `
    -LiteralPath $PatcherPath `
    -Value $Patcher `
    -Encoding utf8

Invoke-Checked `
    -FilePath $PythonExe `
    -ArgumentList @(
        $PatcherPath,
        $ToolGuardrailsPath,
        $TurnContextPath,
        $ConversationLoopPath,
        $RecoveryLimit.ToString()
    ) `
    -Description 'Applying block-and-continue patch'

foreach ($Path in @(
    $ToolGuardrailsPath,
    $TurnContextPath,
    $ConversationLoopPath
)) {
    Invoke-Checked `
        -FilePath $PythonExe `
        -ArgumentList @('-m', 'py_compile', $Path) `
        -Description "Compiling $Path"
}

$SmokeTestPath = Join-Path $BackupRoot 'test_block_and_continue.py'

$SmokeTest = @'
from __future__ import annotations

import json
from types import SimpleNamespace
from unittest.mock import MagicMock, patch

from run_agent import AIAgent


def tool_defs(*names: str) -> list[dict]:
    return [
        {
            "type": "function",
            "function": {
                "name": name,
                "description": f"{name} tool",
                "parameters": {"type": "object", "properties": {}},
            },
        }
        for name in names
    ]


def tool_call(name: str, arguments: dict, call_id: str):
    return SimpleNamespace(
        id=call_id,
        type="function",
        function=SimpleNamespace(
            name=name,
            arguments=json.dumps(arguments),
        ),
    )


def response(*, content: str = "", tool_calls=None):
    finish_reason = "tool_calls" if tool_calls else "stop"
    message = SimpleNamespace(content=content, tool_calls=tool_calls)
    choice = SimpleNamespace(
        message=message,
        finish_reason=finish_reason,
    )
    return SimpleNamespace(
        choices=[choice],
        model="test/model",
        usage=None,
    )


def config() -> dict:
    return {
        "tool_loop_guardrails": {
            "warnings_enabled": True,
            "hard_stop_enabled": True,
            "hard_stop_after": {
                "exact_failure": 2,
                "same_tool_failure": 8,
                "idempotent_no_progress": 5,
            },
            "loop_caps": {
                "max_web_searches": 15,
                "max_subagents": 10,
                "max_browser_navigations": 15,
                "max_terminal_calls": 30,
                "max_total_tool_calls": 50,
            },
        }
    }


def make_agent(max_iterations: int = 12) -> AIAgent:
    cfg = config()
    with (
        patch(
            "run_agent.get_tool_definitions",
            return_value=tool_defs("web_search"),
        ),
        patch(
            "run_agent.check_toolset_requirements",
            return_value={},
        ),
        patch(
            "hermes_cli.config.load_config",
            return_value=cfg,
        ),
        patch(
            "hermes_cli.config.load_config_readonly",
            return_value=cfg,
        ),
        patch("run_agent.OpenAI"),
    ):
        agent = AIAgent(
            api_key="test-key-1234567890",
            base_url="https://openrouter.ai/api/v1",
            max_iterations=max_iterations,
            quiet_mode=True,
            skip_context_files=True,
            skip_memory=True,
        )

    agent.client = MagicMock()
    agent._cached_system_prompt = "You are helpful."
    agent._use_prompt_caching = False
    agent.compression_enabled = False
    agent.save_trajectories = False
    return agent


def run_recovery_test() -> None:
    agent = make_agent()
    args = {"query": "same"}

    agent.client.chat.completions.create.side_effect = [
        response(tool_calls=[tool_call("web_search", args, "c1")]),
        response(tool_calls=[tool_call("web_search", args, "c2")]),
        response(tool_calls=[tool_call("web_search", args, "c3")]),
        response(content="Recovered by changing strategy."),
    ]

    with (
        patch(
            "run_agent.handle_function_call",
            return_value=json.dumps({"error": "boom"}),
        ) as dispatch,
        patch.object(agent, "_persist_session"),
        patch.object(agent, "_save_trajectory"),
        patch.object(agent, "_cleanup_task_resources"),
    ):
        result = agent.run_conversation("search repeatedly")

    assert dispatch.call_count == 2, dispatch.call_count
    assert result["final_response"] == "Recovered by changing strategy."
    assert result["turn_exit_reason"].startswith("text_response")
    assert agent._tool_guardrail_recovery_count == 1

    tool_results = [
        message["content"]
        for message in result["messages"]
        if message.get("role") == "tool"
    ]
    assert any(
        "repeated_exact_failure_block" in content
        for content in tool_results
    )


def run_bounded_stop_test() -> None:
    agent = make_agent(max_iterations=14)
    args = {"query": "same"}

    agent.client.chat.completions.create.side_effect = [
        response(
            tool_calls=[
                tool_call(
                    "web_search",
                    args,
                    f"c{index}",
                )
            ]
        )
        for index in range(1, 13)
    ]

    deltas = []
    agent.stream_delta_callback = lambda value: deltas.append(value)
    agent._disable_streaming = True

    with (
        patch(
            "run_agent.handle_function_call",
            return_value=json.dumps({"error": "boom"}),
        ) as dispatch,
        patch.object(agent, "_persist_session"),
        patch.object(agent, "_save_trajectory"),
        patch.object(agent, "_cleanup_task_resources"),
    ):
        result = agent.run_conversation("ignore every block")

    assert dispatch.call_count == 2, dispatch.call_count
    assert result["turn_exit_reason"] == "guardrail_halt"
    assert agent._tool_guardrail_recovery_count == 4
    assert "best validated partial result" in result["final_response"]
    assert result["final_response"] in [
        value for value in deltas if isinstance(value, str)
    ]


run_recovery_test()
run_bounded_stop_test()
print("BLOCK-AND-CONTINUE SMOKE TEST: PASS")
'@

Set-Content `
    -LiteralPath $SmokeTestPath `
    -Value $SmokeTest `
    -Encoding utf8

Invoke-Checked `
    -FilePath $PythonExe `
    -ArgumentList @($SmokeTestPath) `
    -Description 'Block-and-continue runtime smoke tests'

Write-Host ''
Write-Host 'Block-and-continue guardrail recovery installed.' -ForegroundColor Green
Write-Host "Recovery limit per user turn: $RecoveryLimit"
Write-Host "Backup: $BackupRoot"
Write-Host ''
Write-Host 'Restart Hermes Local and fully reopen the TUI/Desktop client.' -ForegroundColor Yellow
