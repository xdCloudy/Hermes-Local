[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$root = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$manifest = Get-Content -Raw -LiteralPath (Join-Path $root 'VERSION.json') | ConvertFrom-Json
$checkout = Join-Path $env:RUNNER_TEMP ("hermes-agent-patch-probe-" + [guid]::NewGuid().ToString('N'))
$patchOutput = Join-Path $env:RUNNER_TEMP ("hermes-agent-generated-patch-" + [guid]::NewGuid().ToString('N'))
$patchDirectory = Join-Path $root 'source\hermes-launcher\patches'
$newPatchName = '0019-fix-desktop-allow-start-recovery-during-benchmark.patch'

function Invoke-Native {
    param(
        [Parameter(Mandatory)]
        [string] $FilePath,
        [Parameter(Mandatory)]
        [string[]] $ArgumentList
    )

    & $FilePath @ArgumentList
    if ($LASTEXITCODE -ne 0) {
        throw "$FilePath failed with exit code $LASTEXITCODE."
    }
}

try {
    [System.IO.Directory]::CreateDirectory($checkout) | Out-Null
    [System.IO.Directory]::CreateDirectory($patchOutput) | Out-Null
    Invoke-Native git @('-C', $checkout, 'init')
    Invoke-Native git @('-C', $checkout, 'remote', 'add', 'origin', [string]$manifest.sources.hermesAgent.repository)
    Invoke-Native git @('-C', $checkout, 'fetch', '--depth', '1', 'origin', [string]$manifest.sources.hermesAgent.commit)
    Invoke-Native git @('-C', $checkout, 'checkout', '--detach', 'FETCH_HEAD')
    Invoke-Native git @('-C', $checkout, 'config', 'user.name', 'xdCloudy')
    Invoke-Native git @('-C', $checkout, 'config', 'user.email', '52116030+xdCloudy@users.noreply.github.com')

    $patches = @(
        Get-ChildItem -LiteralPath $patchDirectory -Filter '*.patch' -File |
            Where-Object Name -ne $newPatchName |
            Sort-Object Name
    )
    $amArguments = @('-C', $checkout, 'am', '--committer-date-is-author-date') + @($patches.FullName)
    Invoke-Native git $amArguments

    @'
from pathlib import Path

root = Path.cwd()
control_path = root / "apps/desktop/electron/hermes-local-control.ts"
test_path = root / "apps/desktop/electron/hermes-local-control.test.ts"

control = control_path.read_text(encoding="utf-8")
control_anchor = """function runningTask(
  requestedAction: ActionName,
  actionIds: Map<ActionName, string> = runningActions,
  taskMap: Map<string, ActionTask> = tasks
): ActionTask | null {
"""
conflict_helper = """function actionsConflict(requestedAction: ActionName, activeAction: ActionName): boolean {
  // Start is an idempotent readiness/recovery action. It may join the stack
  // while a benchmark owns model resources, but disruptive actions remain
  // mutually exclusive with the benchmark.
  return !(requestedAction === 'start' && activeAction === 'benchmark')
}

"""
if conflict_helper not in control:
    if control_anchor not in control:
        raise SystemExit("runningTask anchor not found")
    control = control.replace(control_anchor, conflict_helper + control_anchor, 1)

old_condition = """    if (task?.status === 'running') {
      return task
    }
"""
new_condition = """    if (task?.status === 'running' && actionsConflict(requestedAction, task.action)) {
      return task
    }
"""
if new_condition not in control:
    if old_condition not in control:
        raise SystemExit("runningTask condition not found")
    control = control.replace(old_condition, new_condition, 1)
control_path.write_text(control, encoding="utf-8", newline="\n")

tests = test_path.read_text(encoding="utf-8")
old_test = """  it('returns the running task for matching and overlapping action requests', () => {
    const running = {
      action: 'backup',
      status: 'running'
    }

    const taskMap = new Map([['backup-task', running]])
    const actionIds = new Map([['backup', 'backup-task']])

    expect(hermesLocalControlTest.runningTask('backup', actionIds as never, taskMap as never)).toBe(running)
    expect(hermesLocalControlTest.runningTask('restart', actionIds as never, taskMap as never)).toBe(running)
  })
"""
new_test = """  it('returns conflicting tasks but permits idempotent start recovery during a benchmark', () => {
    const backup = {
      action: 'backup',
      status: 'running'
    }

    const backupTaskMap = new Map([['backup-task', backup]])
    const backupActionIds = new Map([['backup', 'backup-task']])

    expect(hermesLocalControlTest.runningTask('backup', backupActionIds as never, backupTaskMap as never)).toBe(backup)
    expect(hermesLocalControlTest.runningTask('restart', backupActionIds as never, backupTaskMap as never)).toBe(backup)

    const benchmark = {
      action: 'benchmark',
      status: 'running'
    }

    const benchmarkTaskMap = new Map([['benchmark-task', benchmark]])
    const benchmarkActionIds = new Map([['benchmark', 'benchmark-task']])

    expect(
      hermesLocalControlTest.runningTask('start', benchmarkActionIds as never, benchmarkTaskMap as never)
    ).toBeNull()
    expect(
      hermesLocalControlTest.runningTask('restart', benchmarkActionIds as never, benchmarkTaskMap as never)
    ).toBe(benchmark)
    expect(
      hermesLocalControlTest.runningTask('benchmark', benchmarkActionIds as never, benchmarkTaskMap as never)
    ).toBe(benchmark)
  })
"""
if new_test not in tests:
    if old_test not in tests:
        raise SystemExit("running task test block not found")
    tests = tests.replace(old_test, new_test, 1)
test_path.write_text(tests, encoding="utf-8", newline="\n")
'@ | python -

    $env:GIT_AUTHOR_DATE = '2026-07-30T19:05:00Z'
    $env:GIT_COMMITTER_DATE = '2026-07-30T19:05:00Z'
    Invoke-Native git @('-C', $checkout, 'add', '--',
        'apps/desktop/electron/hermes-local-control.ts',
        'apps/desktop/electron/hermes-local-control.test.ts')
    Invoke-Native git @('-C', $checkout, 'commit', '-m', 'fix(desktop): allow start recovery during benchmarks')

    Push-Location $checkout
    try {
        Invoke-Native npm @('ci', '--ignore-scripts')
        Invoke-Native npm @(
            'exec', '--workspace', 'apps/desktop', '--',
            'vitest', 'run', 'electron/hermes-local-control.test.ts'
        )
    } finally {
        Pop-Location
    }

    Invoke-Native git @('-C', $checkout, 'format-patch', '-1', '--no-signature', '--output-directory', $patchOutput)
    $generatedPatch = Get-ChildItem -LiteralPath $patchOutput -Filter '*.patch' -File | Select-Object -Single
    $patchBase64 = [Convert]::ToBase64String([System.IO.File]::ReadAllBytes($generatedPatch.FullName))
    Write-Host 'PATCH_BASE64_BEGIN'
    for ($offset = 0; $offset -lt $patchBase64.Length; $offset += 2000) {
        $length = [Math]::Min(2000, $patchBase64.Length - $offset)
        Write-Host $patchBase64.Substring($offset, $length)
    }
    Write-Host 'PATCH_BASE64_END'

    $integrationCommit = (& git -C $checkout rev-parse HEAD).Trim()
    $integrationTree = (& git -C $checkout rev-parse 'HEAD^{tree}').Trim()
    $marker = "PATCH_SERIES_RESULT commit=$integrationCommit tree=$integrationTree"
    Write-Host $marker
    throw $marker
} finally {
    Remove-Item -LiteralPath $checkout -Recurse -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $patchOutput -Recurse -Force -ErrorAction SilentlyContinue
}
