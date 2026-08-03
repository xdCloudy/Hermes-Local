#!/usr/bin/env python3
"""Generate issue #24 Desktop changes against a reconstructed integration tree."""
from __future__ import annotations

import sys
from pathlib import Path


def replace_once(text: str, old: str, new: str, label: str) -> str:
    count = text.count(old)
    if count != 1:
        raise RuntimeError(f"Expected one {label} anchor, found {count}")
    return text.replace(old, new, 1)


def update_control(source: Path) -> None:
    path = source / "apps/desktop/electron/hermes-local-control.ts"
    text = path.read_text(encoding="utf-8")

    text = replace_once(
        text,
        """  const stageMatches = [...text.matchAll(/::hermes-model-switch-stage::([a-z0-9-]+)::([^\\r\\n]+)/gi)]

  task.output = result.output
  task.outputTruncated ||= result.truncated
  task.stage = stageMatches.at(-1)?.[1] || task.stage
""",
        """  const modelStageMatches = [...text.matchAll(/::hermes-model-switch-stage::([a-z0-9-]+)::([^\\r\\n]+)/gi)]
  const benchmarkStageMatches = [...text.matchAll(/::hermes-benchmark-stage::([a-z0-9-]+)::([^\\r\\n]+)/gi)]
  const stage = benchmarkStageMatches.at(-1)?.[1] || modelStageMatches.at(-1)?.[1]

  task.output = result.output
  task.outputTruncated ||= result.truncated
  task.stage = stage || task.stage
""",
        "task output stage parser",
    )

    helper = """function benchmarkProgress(task: TaskRecord): null | Record<string, any> {
  const progress = safeReadJson<Record<string, any>>('data\\\\runtime\\\\benchmark-progress.json')

  return progress && String(progress.taskId || '') === task.id ? progress : null
}

function benchmarkProgressSummary(progress: Record<string, any>): string {
  const completed = Number.isFinite(Number(progress.completedUnits)) ? Number(progress.completedUnits) : null
  const total = Number.isFinite(Number(progress.totalUnits)) ? Number(progress.totalUnits) : null
  const units = completed !== null && total !== null ? ` ${completed}/${total}` : ''

  return `Benchmark progress: ${String(progress.stage || 'running')}${units} · ${String(progress.message || '')}`.trim()
}

"""
    text = replace_once(
        text,
        "function taskCompletionEvidence(task: TaskRecord): null | TaskCompletionEvidence {\n",
        helper + "function taskCompletionEvidence(task: TaskRecord): null | TaskCompletionEvidence {\n",
        "benchmark progress helper insertion",
    )

    text = replace_once(
        text,
        """  if (task.action === 'benchmark') {
    return jsonEvidence('benchmarks\\\\results\\\\latest.json', document => {
""",
        """  if (task.action === 'benchmark') {
    const progress = benchmarkProgress(task)
    const status = String(progress?.status || '')

    if (progress && ['cancelled', 'failed', 'succeeded'].includes(status)) {
      const reportPath = String(progress.result?.report || 'benchmarks/reports/LATEST.md').replaceAll('\\\\', '/')
      const failed = status === 'failed'

      return {
        exitCode: status === 'succeeded' ? 0 : status === 'cancelled' ? 130 : 1,
        failure: failed
          ? {
              code: String(progress.failure?.code || 'benchmark-failed'),
              message: String(progress.failure?.message || 'Recovered benchmark progress records a failure')
            }
          : null,
        observedAt: String(progress.completedAt || progress.updatedAt || new Date().toISOString()),
        result: { kind: 'report', path: reportPath },
        status: status as TaskCompletionEvidence['status']
      }
    }

    return jsonEvidence('benchmarks\\\\results\\\\latest.json', document => {
""",
        "benchmark terminal evidence",
    )

    progress_reconcile = """    if (task.action === 'benchmark') {
      const progress = benchmarkProgress(task)
      const observedAt = Date.parse(String(progress?.updatedAt || ''))

      if (
        progress &&
        Number.isFinite(observedAt) &&
        observedAt > Date.parse(task.updatedAt) &&
        !['cancelled', 'failed', 'succeeded'].includes(String(progress.status || ''))
      ) {
        const summary = benchmarkProgressSummary(progress)
        const output = boundedTaskOutput(task.output, `\\n${summary}\\n`, MAX_TASK_OUTPUT)

        task.output = output.output
        task.outputTruncated ||= output.truncated
        task.stage = String(progress.stage || task.stage || '') || null
        task.updatedAt = new Date(observedAt).toISOString()
        changed = true
      }
    }

"""
    text = replace_once(
        text,
        "    const recovered = reconcileRecoveredTask(\n",
        progress_reconcile + "    const recovered = reconcileRecoveredTask(\n",
        "benchmark reconciliation",
    )

    text = replace_once(
        text,
        "        HERMES_LOCAL_ROOT: localRoot()\n",
        "        HERMES_LOCAL_ROOT: localRoot(),\n        HERMES_LOCAL_TASK_ID: task.id\n",
        "benchmark task environment",
    )

    cancellation = """  if (task.action === 'benchmark') {
    const cancellationPath = resolveUnderRoot('data\\\\runtime\\\\benchmark-cancel.json')
    const temporaryPath = `${cancellationPath}.${process.pid}.${Date.now()}.tmp`
    const request = {
      schemaVersion: 1,
      taskId: task.id,
      ownerPid: task.owner.pid,
      requestedAt: new Date().toISOString(),
      requestedBy: 'desktop'
    }

    fs.mkdirSync(path.dirname(cancellationPath), { recursive: true })
    fs.writeFileSync(temporaryPath, `${JSON.stringify(request, null, 2)}\\n`, 'utf8')
    fs.rmSync(cancellationPath, { force: true })
    fs.renameSync(temporaryPath, cancellationPath)
    replaceTaskRecord(task, requestTaskCancellation(task, new Date().toISOString()))
    appendTaskOutput(
      task,
      '\\nCancellation requested. The active native case will finish before the benchmark restores the model stack.\\n'
    )
    flushScheduledTaskPersistence()

    return publicTask(task)
  }

"""
    anchor = """  replaceTaskRecord(task, requestTaskCancellation(task, new Date().toISOString()))
  flushScheduledTaskPersistence()

  try {
"""
    text = replace_once(text, anchor, cancellation + anchor, "cooperative benchmark cancellation")

    path.write_text(text, encoding="utf-8", newline="\n")


def update_task_model(source: Path) -> None:
    path = source / "apps/desktop/electron/hermes-local-task-model.ts"
    text = path.read_text(encoding="utf-8")
    text = replace_once(
        text,
        "  status: 'failed' | 'succeeded'\n",
        "  status: 'cancelled' | 'failed' | 'succeeded'\n",
        "completion evidence status",
    )
    text = replace_once(
        text,
        """  if (evidence && Number.isFinite(evidenceTime) && evidenceTime >= taskStart) {
    return transitionTask(task, evidence.status, at, {
""",
        """  if (evidence && Number.isFinite(evidenceTime) && evidenceTime >= taskStart) {
    const source =
      evidence.status === 'cancelled' && task.status === 'running'
        ? transitionTask(task, 'cancelling', at)
        : task

    return transitionTask(source, evidence.status, at, {
""",
        "cancelled recovery transition",
    )
    path.write_text(text, encoding="utf-8", newline="\n")


def add_test(source: Path) -> None:
    path = source / "apps/desktop/electron/hermes-local-benchmark-progress.test.ts"
    path.write_text(
        """import { describe, expect, it } from 'vitest'

import {
  createTaskRecord,
  reconcileRecoveredTask,
  transitionTask,
  type TaskCompletionEvidence
} from './hermes-local-task-model'

describe('Hermes Local benchmark task recovery', () => {
  it('reconstructs authoritative benchmark cancellation after Desktop restarts', () => {
    const created = createTaskRecord(
      'benchmark',
      'benchmark-task',
      { kind: 'desktop-child-process', pid: 4242 },
      '2026-08-03T04:00:00.000Z'
    )
    const running = transitionTask(created, 'running', '2026-08-03T04:00:01.000Z', {
      owner: { kind: 'desktop-child-process', pid: 4242 }
    })
    const evidence: TaskCompletionEvidence = {
      exitCode: 130,
      failure: null,
      observedAt: '2026-08-03T04:00:10.000Z',
      result: { kind: 'report', path: 'benchmarks/results/latest.json' },
      status: 'cancelled'
    }

    const recovered = reconcileRecoveredTask(running, false, evidence, '2026-08-03T04:00:11.000Z')

    expect(recovered.status).toBe('cancelled')
    expect(recovered.exitCode).toBe(130)
    expect(recovered.result?.path).toBe('benchmarks/results/latest.json')
  })
})
""",
        encoding="utf-8",
        newline="\n",
    )


def main() -> int:
    if len(sys.argv) != 2:
        raise SystemExit("usage: generate_issue24_patch.py <integrated-source>")
    source = Path(sys.argv[1]).resolve()
    update_control(source)
    update_task_model(source)
    add_test(source)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
