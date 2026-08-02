$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $true

$branch = [string]$env:GITHUB_HEAD_REF
if ([string]::IsNullOrWhiteSpace($branch)) {
    throw 'GITHUB_HEAD_REF is required for the Issue 84 finalizer.'
}

$markerPath = Join-Path $PWD 'scripts\ci\issue84-desktop-patch-v3\validation-trigger.txt'
if (-not (Test-Path -LiteralPath $markerPath)) {
    Write-Host 'Issue 84 finalizer marker is absent; nothing to do.'
    exit 0
}

$marker = (Get-Content -LiteralPath $markerPath -Raw).Trim()
if ($marker -ne 'regenerate final patch 0032') {
    Write-Host "Issue 84 finalizer marker is '$marker'; nothing to do."
    exit 0
}

python -m pip install --disable-pip-version-check --no-input PyYAML==6.0.3
python -m py_compile '.\scripts\verify_model_identity.py' '.\tests\test_verify_model_identity.py'
python -m unittest '.\tests\test_verify_model_identity.py' -v

$parseFailures = [System.Collections.Generic.List[string]]::new()
foreach ($path in @(
    '.\Start-Hermes-Local.ps1',
    '.\Switch-Hermes-Model.ps1',
    '.\scripts\Hermes-Configuration.psm1'
)) {
    $tokens = $null
    $errors = $null
    [void][System.Management.Automation.Language.Parser]::ParseFile(
        (Resolve-Path $path),
        [ref]$tokens,
        [ref]$errors
    )
    foreach ($error in @($errors)) {
        $parseFailures.Add("$path`:$($error.Extent.StartLineNumber): $($error.Message)")
    }
}
if ($parseFailures.Count -gt 0) {
    throw ($parseFailures -join [Environment]::NewLine)
}

$sourcePath = Join-Path $PWD 'temp\hermes-agent'
$verifyPath = Join-Path $PWD 'temp\verify'
Remove-Item -LiteralPath $sourcePath, $verifyPath -Recurse -Force -ErrorAction SilentlyContinue

$manifest = Get-Content -LiteralPath '.\VERSION.json' -Raw | ConvertFrom-Json
$candidate = [string]$manifest.sources.hermesAgent.commit

git clone --filter=blob:none https://github.com/NousResearch/hermes-agent.git $sourcePath
git -C $sourcePath checkout --detach $candidate
git -C $sourcePath config user.name 'Hermes Local CI'
git -C $sourcePath config user.email 'hermes-local-ci@localhost'

$patches = @(
    Get-ChildItem '.\source\hermes-launcher\patches\*.patch' |
        Where-Object Name -ne '0032-fix-desktop-activate-selected-models-through-managed-restart.patch' |
        Sort-Object Name |
        ForEach-Object FullName
)
git -C $sourcePath am --3way --committer-date-is-author-date @patches

@'
import base64
import gzip
import hashlib
from pathlib import Path

parts = sorted(Path('scripts/ci/issue84-desktop-patch-v3').glob('part-*.txt'))
if len(parts) != 7:
    raise SystemExit(f'Expected 7 patch chunks, found {len(parts)}')
encoded = ''.join(path.read_text(encoding='ascii').strip() for path in parts)
if hashlib.sha256((encoded + '\n').encode('ascii')).hexdigest() != '7b81b81956f725986ce669d8e959b689a6c1e9cfc7bb0c46cd8d883ee6d05758':
    raise SystemExit('Issue 84 encoded patch checksum mismatch')
patch = gzip.decompress(base64.b64decode(encoded, validate=True))
if hashlib.sha256(patch).hexdigest() != '1450e46b805d642be20509783ecc20e787f24a09a8cc1ac38984f3a662885473':
    raise SystemExit('Issue 84 decoded patch checksum mismatch')
Path('temp/issue84-desktop.patch').write_bytes(patch)
'@ | python -

$desktopPatch = (Resolve-Path '.\temp\issue84-desktop.patch').Path
git -C $sourcePath apply --check --directory='apps/desktop' $desktopPatch
git -C $sourcePath apply --directory='apps/desktop' $desktopPatch

@'
from pathlib import Path

types_path = Path('temp/hermes-agent/apps/desktop/src/app/local-workstation/types.ts')
types = types_path.read_text(encoding='utf-8')
type_marker = '  stage: null | string\n'
if type_marker not in types:
    raise SystemExit('LocalActionTask.stage declaration was not found')
types_path.write_text(types.replace(type_marker, '  stage?: null | string\n', 1), encoding='utf-8')

model_path = Path('temp/hermes-agent/apps/desktop/electron/hermes-local-task-model.ts')
model = model_path.read_text(encoding='utf-8')
old = """  const stage = candidate.stage === null || candidate.stage === undefined ? null : candidate.stage

  if (stage !== null && (typeof stage !== 'string' || stage.length > 128)) {
    return null
  }
"""
new = """  const stageValue = candidate.stage

  if (
    stageValue !== null &&
    stageValue !== undefined &&
    (typeof stageValue !== 'string' || stageValue.length > 128)
  ) {
    return null
  }

  const stage = typeof stageValue === 'string' ? stageValue : null
"""
if old not in model:
    raise SystemExit('Task stage parser block was not found')
model_path.write_text(model.replace(old, new, 1), encoding='utf-8')
'@ | python -

Push-Location $sourcePath
try {
    npx.cmd --yes npm@12.0.0 install --ignore-scripts --no-audit --fund=false
    Push-Location '.\apps\desktop'
    try {
        npx.cmd --no-install vitest run --project electron `
            electron/hermes-local-model-switch.test.ts `
            electron/hermes-local-control.test.ts `
            electron/hermes-local-task-model.test.ts `
            electron/hermes-local-task-store.test.ts `
            electron/hermes-local-settings.test.ts `
            electron/hermes-local-dashboard-view.test.ts
    }
    finally {
        Pop-Location
    }
    npx.cmd --yes npm@12.0.0 run typecheck --workspace hermes-launcher
    npx.cmd --yes npm@12.0.0 run lint --workspace hermes-launcher
}
finally {
    Pop-Location
}

git -C $sourcePath add apps/desktop package-lock.json
git -C $sourcePath commit -m 'fix(desktop): activate selected models through managed restart'

$destination = (Resolve-Path '.\source\hermes-launcher\patches').Path
$generated = (git -C $sourcePath format-patch -1 --zero-commit --output-directory $destination).Trim()
$finalPatch = Join-Path $destination '0032-fix-desktop-activate-selected-models-through-managed-restart.patch'
if (Test-Path -LiteralPath $finalPatch) {
    Remove-Item -LiteralPath $finalPatch -Force
}
Move-Item -LiteralPath $generated -Destination $finalPatch -Force

$integrationCommit = (git -C $sourcePath rev-parse HEAD).Trim()
$integrationTree = (git -C $sourcePath rev-parse 'HEAD^{tree}').Trim()

@'
import json
from datetime import datetime, timezone
from pathlib import Path

path = Path('VERSION.json')
document = json.loads(path.read_text(encoding='utf-8-sig'))
document['sources']['hermesAgent']['integrationCommit'] = r'__COMMIT__'
document['sources']['hermesAgent']['integrationTree'] = r'__TREE__'
document['recordedAt'] = datetime.now(timezone.utc).isoformat().replace('+00:00', 'Z')
path.write_text(json.dumps(document, indent=2) + '\n', encoding='utf-8')
'@.Replace('__COMMIT__', $integrationCommit).Replace('__TREE__', $integrationTree) | python -

$manifest = Get-Content -LiteralPath '.\VERSION.json' -Raw | ConvertFrom-Json
git clone --filter=blob:none https://github.com/NousResearch/hermes-agent.git $verifyPath
git -C $verifyPath checkout --detach $candidate
git -C $verifyPath config user.name 'Hermes Local CI'
git -C $verifyPath config user.email 'hermes-local-ci@localhost'
$allPatches = @(Get-ChildItem '.\source\hermes-launcher\patches\*.patch' | Sort-Object Name | ForEach-Object FullName)
git -C $verifyPath am --3way --committer-date-is-author-date @allPatches
$verifiedTree = (git -C $verifyPath rev-parse 'HEAD^{tree}').Trim()
if ($verifiedTree -ne [string]$manifest.sources.hermesAgent.integrationTree) {
    throw "Generated integration tree mismatch: $verifiedTree"
}

Remove-Item -LiteralPath $markerPath -Force

git config user.name 'Hermes Local CI'
git config user.email 'hermes-local-ci@localhost'
git add VERSION.json source/hermes-launcher/patches/0032-fix-desktop-activate-selected-models-through-managed-restart.patch scripts/ci/issue84-desktop-patch-v3/validation-trigger.txt
if (-not (git diff --cached --quiet)) {
    git commit -m 'fix: activate selected models through managed stack restart'
    git push origin "HEAD:$branch"
}
