#!/usr/bin/env python3
"""Regenerate the issue 107 patch tail against the pinned Hermes Agent integration."""
from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import subprocess
from dataclasses import dataclass
from pathlib import Path


@dataclass
class Hunk:
    old_start: int
    old_lines: list[str]
    new_lines: list[str]


@dataclass
class FilePatch:
    path: str
    new_file: bool
    hunks: list[Hunk]


def run(args: list[str], cwd: Path, env: dict[str, str] | None = None) -> str:
    completed = subprocess.run(
        args,
        cwd=cwd,
        env={**os.environ, **(env or {})},
        check=False,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
    )
    if completed.returncode != 0:
        raise RuntimeError(f"Command failed ({completed.returncode}): {' '.join(args)}\n{completed.stdout}")
    return completed.stdout


def parse_subject(text: str) -> str:
    lines = text.splitlines()
    for index, line in enumerate(lines):
        if not line.startswith('Subject: '):
            continue
        subject = line.removeprefix('Subject: ').strip()
        cursor = index + 1
        while cursor < len(lines) and lines[cursor].startswith(' '):
            subject += ' ' + lines[cursor].strip()
            cursor += 1
        return re.sub(r'^\[PATCH(?: [^]]+)?\]\s*', '', subject)
    raise ValueError('Patch subject is missing')


def parse_author_date(text: str) -> str | None:
    for line in text.splitlines():
        if line.startswith('Date: '):
            return line.removeprefix('Date: ').strip()
    return None


def parse_patch(text: str) -> list[FilePatch]:
    lines = text.splitlines()
    result: list[FilePatch] = []
    index = 0
    while index < len(lines):
        if not lines[index].startswith('diff --git '):
            index += 1
            continue
        header = lines[index]
        match = re.match(r'diff --git a/(.+) b/(.+)$', header)
        if not match or match.group(1) != match.group(2):
            raise ValueError(f'Unsupported diff header: {header}')
        path = match.group(2)
        index += 1
        new_file = False
        hunks: list[Hunk] = []
        while index < len(lines) and not lines[index].startswith('diff --git '):
            line = lines[index]
            if line == 'new file mode 100644':
                new_file = True
                index += 1
                continue
            if not line.startswith('@@ '):
                index += 1
                continue
            hunk_match = re.match(r'@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@', line)
            if not hunk_match:
                raise ValueError(f'Unsupported hunk header: {line}')
            old_start = int(hunk_match.group(1))
            index += 1
            old_lines: list[str] = []
            new_lines: list[str] = []
            while index < len(lines):
                body = lines[index]
                if body.startswith('@@ ') or body.startswith('diff --git ') or body == '-- ':
                    break
                if body.startswith('\\ No newline at end of file'):
                    index += 1
                    continue
                if body.startswith(' '):
                    old_lines.append(body[1:])
                    new_lines.append(body[1:])
                elif body.startswith('-'):
                    old_lines.append(body[1:])
                elif body.startswith('+'):
                    new_lines.append(body[1:])
                else:
                    break
                index += 1
            hunks.append(Hunk(old_start=old_start, old_lines=old_lines, new_lines=new_lines))
        result.append(FilePatch(path=path, new_file=new_file, hunks=hunks))
    return result


def find_sequence(lines: list[str], needle: list[str], start: int) -> int | None:
    if not needle:
        return start
    maximum = len(lines) - len(needle)
    for index in range(max(0, start), maximum + 1):
        if lines[index:index + len(needle)] == needle:
            return index
    for index in range(0, max(0, start)):
        if lines[index:index + len(needle)] == needle:
            return index
    return None


def apply_file_patch(root: Path, patch: FilePatch) -> None:
    target = root / patch.path
    if patch.new_file:
        lines: list[str] = []
    else:
        if not target.is_file():
            raise FileNotFoundError(f'Patch target is missing: {patch.path}')
        lines = target.read_text(encoding='utf-8').splitlines()

    cursor = 0
    offset = 0
    for number, hunk in enumerate(patch.hunks, start=1):
        position = find_sequence(lines, hunk.old_lines, cursor)
        if position is None:
            expected = max(0, hunk.old_start - 1 + offset)
            preview_start = max(0, expected - 4)
            preview_end = min(len(lines), expected + max(len(hunk.old_lines), 4) + 4)
            expected_preview = '\n'.join(hunk.old_lines[:20])
            actual_preview = '\n'.join(lines[preview_start:preview_end])
            raise RuntimeError(
                f'Could not apply {patch.path} hunk {number}.\n'
                f'Expected sequence:\n{expected_preview}\n\n'
                f'Nearby source ({preview_start + 1}-{preview_end}):\n{actual_preview}'
            )
        lines[position:position + len(hunk.old_lines)] = hunk.new_lines
        cursor = position + len(hunk.new_lines)
        offset += len(hunk.new_lines) - len(hunk.old_lines)

    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text('\n'.join(lines) + '\n', encoding='utf-8', newline='\n')


def apply_relaxed_patch(root: Path, patch_path: Path) -> tuple[str, str | None]:
    text = patch_path.read_text(encoding='utf-8')
    for file_patch in parse_patch(text):
        apply_file_patch(root, file_patch)
    return parse_subject(text), parse_author_date(text)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument('--repository-root', type=Path, default=Path.cwd())
    parser.add_argument('--work-dir', type=Path, required=True)
    parser.add_argument('--output-dir', type=Path, required=True)
    args = parser.parse_args()

    repository_root = args.repository_root.resolve()
    work_dir = args.work_dir.resolve()
    output_dir = args.output_dir.resolve()
    manifest = json.loads((repository_root / 'VERSION.json').read_text(encoding='utf-8'))
    source = manifest['sources']['hermesAgent']
    patch_dir = repository_root / source['patchSeries']
    patches = sorted(patch_dir.glob('*.patch'), key=lambda item: item.name)
    base_patches = [item for item in patches if item.name < '0055-']
    tail_patches = [item for item in patches if item.name >= '0055-']
    expected_tail = [
        '0055-feat-desktop-define-durable-model-download-tasks.patch',
        '0056-feat-desktop-run-downloads-through-task-centre.patch',
        '0057-feat-desktop-expose-download-lifecycle-controls.patch',
        '0058-feat-desktop-add-managed-model-download-surface.patch',
    ]
    if [item.name for item in tail_patches] != expected_tail:
        raise RuntimeError('Unexpected issue 107 patch tail')

    shutil.rmtree(work_dir, ignore_errors=True)
    shutil.rmtree(output_dir, ignore_errors=True)
    output_dir.mkdir(parents=True, exist_ok=True)
    run(['git', 'clone', '--no-checkout', source['repository'], str(work_dir)], repository_root)
    run(['git', 'checkout', source['commit']], work_dir)
    run(['git', 'config', 'user.name', 'Hermes Local Patch Regenerator'], work_dir)
    run(['git', 'config', 'user.email', 'hermes-local-ci@localhost'], work_dir)
    run(
        ['git', 'am', '--3way', '--committer-date-is-author-date', *[str(item) for item in base_patches]],
        work_dir,
    )

    for patch_path in tail_patches:
        subject, date = apply_relaxed_patch(work_dir, patch_path)
        run(['git', 'add', '-A'], work_dir)
        environment: dict[str, str] = {}
        if date:
            environment['GIT_AUTHOR_DATE'] = date
            environment['GIT_COMMITTER_DATE'] = date
        run(['git', 'commit', '-m', subject], work_dir, environment)
        generated = run(['git', 'format-patch', '-1', '--stdout', 'HEAD'], work_dir)
        (output_dir / patch_path.name).write_text(generated, encoding='utf-8', newline='\n')
        commit = run(['git', 'rev-parse', 'HEAD'], work_dir).strip()
        print(f'{patch_path.name}: {commit}')

    print(f'Generated patches in {output_dir}')
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
