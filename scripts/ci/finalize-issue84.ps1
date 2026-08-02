$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $true

$sourcePath = Join-Path $PWD 'temp\hermes-agent'
Remove-Item -LiteralPath $sourcePath -Recurse -Force -ErrorAction SilentlyContinue
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
from pathlib import Path
parts = sorted(Path('scripts/ci/issue84-desktop-patch-v3').glob('part-*.txt'))
encoded = ''.join(path.read_text(encoding='ascii').strip() for path in parts)
Path('temp/issue84-desktop.patch').write_bytes(gzip.decompress(base64.b64decode(encoded, validate=True)))
'@ | python -

$desktopPatch = (Resolve-Path '.\temp\issue84-desktop.patch').Path
git -C $sourcePath apply --directory='apps/desktop' $desktopPatch

@'
from pathlib import Path
path = Path('temp/hermes-agent/apps/desktop/electron/hermes-local-control.ts')
for number, line in enumerate(path.read_text(encoding='utf-8').splitlines(), 1):
    if 'hermes-model-switch-stage' in line:
        print(f'ISSUE84-LINE {number}: {line!r}')
raise SystemExit('Issue 84 source line inspection complete')
'@ | python -
