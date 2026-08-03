from __future__ import annotations

import base64
import hashlib
import json
import shutil
import zipfile
from pathlib import Path

ROOT = Path.cwd()
PAYLOAD = ROOT / ".github" / "issue-25"
EXPECTED_SHA256 = "6f43a5bce5c9377cd022fa7cccc72963b2f2a4ba4efb44a849befe1ece832a2a"
EXPECTED_COMMIT = "2328d03544cfc4cbbccf1e7a6a2ea0742d6adbaa"
EXPECTED_TREE = "facb49e58567e5ef9be8ff10bd1e4d208293c09e"


def replace_once(text: str, old: str, new: str, label: str) -> str:
    if old not in text:
        raise RuntimeError(f"Missing correction point: {label}")
    return text.replace(old, new, 1)


parts = sorted(PAYLOAD.glob("payload.part*.b64"))
if len(parts) != 4:
    raise RuntimeError(f"Expected 4 payload chunks, found {len(parts)}")

archive_bytes = base64.b64decode("".join(path.read_text(encoding="utf-8").strip() for path in parts))
digest = hashlib.sha256(archive_bytes).hexdigest()
if digest != EXPECTED_SHA256:
    raise RuntimeError(f"Payload digest mismatch: {digest}")

archive = PAYLOAD / "payload-v6.zip"
archive.write_bytes(archive_bytes)
extracted = PAYLOAD / "extracted-v6"
if extracted.exists():
    shutil.rmtree(extracted)
with zipfile.ZipFile(archive) as bundle:
    bundle.extractall(extracted)

progress = extracted / "Security-Progress.ps1"
text = progress.read_text(encoding="utf-8")
text = replace_once(
    text,
    ".TrimEnd('\\\\', '/')",
    ".TrimEnd([char[]]@([System.IO.Path]::DirectorySeparatorChar, [System.IO.Path]::AltDirectorySeparatorChar))",
    "path trimming",
)
text = replace_once(
    text,
    "return $relative.Replace('\\\\', '/')",
    "return $relative.Replace([System.IO.Path]::DirectorySeparatorChar, [System.IO.Path]::AltDirectorySeparatorChar)",
    "relative path normalization",
)
text = replace_once(
    text,
    "(?![\\d.])', '[PRIVATE-TARGET]'",
    "(?!\\d|\\.\\d)', '[PRIVATE-TARGET]'",
    "private target sentence boundary",
)

credential_anchor = "    $safe = $safe -replace '(?i)\\b(?:https?|wss?)://"
credential_redaction = (
    "    $safe = $safe -replace '(?i)\\bAuthorization\\s*:\\s*Bearer\\s+[^\\s,;]+', 'Authorization: Bearer [REDACTED]'\n"
    "    $safe = $safe -replace '(?i)\\b(?:token|api[_-]?key|secret|password|credential)\\s*[:=]\\s*[^\\s,;]+', '[REDACTED-CREDENTIAL]'\n"
)
text = replace_once(text, credential_anchor, credential_redaction + credential_anchor, "credential redaction")

log_marker = "        $logLine = '{0} [{1}] {2}: {3}' -f $now.ToString('o'), $Status, $Stage, $safeMessage\n"
log_setup = (
    "        $logDirectory = [System.IO.Path]::GetDirectoryName($script:securityTaskLogPath)\n"
    "        if ($logDirectory) {\n"
    "            [System.IO.Directory]::CreateDirectory($logDirectory) | Out-Null\n"
    "        }\n"
)
text = replace_once(text, log_marker, log_setup + log_marker, "task log directory")
progress.write_text(text, encoding="utf-8", newline="\n")

contract = extracted / "Test-HermesSecurityScanProgress.ps1"
contract_text = contract.read_text(encoding="utf-8")
contract_text = replace_once(
    contract_text,
    "    Assert-True ($cancelled.completedAt) 'Cancelled scan omitted its completion timestamp.'",
    "    Assert-True (-not [string]::IsNullOrWhiteSpace([string]$cancelled.completedAt)) 'Cancelled scan omitted its completion timestamp.'",
    "completion timestamp assertion",
)
contract.write_text(contract_text, encoding="utf-8", newline="\n")

targets = {
    extracted / "Security-Scan-Hermes-Local.ps1": ROOT / "Security-Scan-Hermes-Local.ps1",
    progress: ROOT / "scripts" / "security" / "Security-Progress.ps1",
    contract: ROOT / "tests" / "Test-HermesSecurityScanProgress.ps1",
    extracted / "0036-feat-desktop-instrument-security-scan-task-progress.patch": ROOT
    / "source"
    / "hermes-launcher"
    / "patches"
    / "0036-feat-desktop-instrument-security-scan-task-progress.patch",
}
for source, target in targets.items():
    if not source.is_file():
        raise RuntimeError(f"Payload file is missing: {source}")
    target.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(source, target)

compatibility = ROOT / "scripts" / "ci" / "compatibility.py"
value = compatibility.read_text(encoding="utf-8")
needle = '                    run([vitest, "run", "--project", "electron", "electron/hermes-local-update.test.ts"], cwd=source / "apps/desktop", log=logs / "tests.log")\n'
if "hermes-local-security-progress.test.ts" not in value:
    if needle not in value:
        raise RuntimeError("Compatibility insertion point not found")
    value = value.replace(
        needle,
        needle
        + '                    run([vitest, "run", "--project", "electron", "electron/hermes-local-security-progress.test.ts"], cwd=source / "apps/desktop", log=logs / "tests.log")\n'
        + '                    run([vitest, "run", "src/app/local-workstation/task-centre.test.tsx", "src/app/local-workstation/security-task-view.test.tsx"], cwd=source / "apps/desktop", log=logs / "tests.log")\n',
        1,
    )
    value = value.replace(
        'done.append("desktop:typecheck,lint,focused-electron,project-registry")',
        'done.append("desktop:typecheck,lint,focused-electron,security-task-ui,project-registry")',
        1,
    )
compatibility.write_text(value, encoding="utf-8", newline="\n")

version_path = ROOT / "VERSION.json"
version = json.loads(version_path.read_text(encoding="utf-8"))
version["product"]["version"] = "0.18.18"
version["sources"]["hermesAgent"]["integrationCommit"] = EXPECTED_COMMIT
version["sources"]["hermesAgent"]["integrationTree"] = EXPECTED_TREE
version["recordedAt"] = "2026-08-03T13:52:00Z"
version_path.write_text(json.dumps(version, indent=2) + "\n", encoding="utf-8", newline="\n")
