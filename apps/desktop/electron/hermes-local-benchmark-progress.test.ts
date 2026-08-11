import { describe, expect, it } from 'vitest'

import {
  createTaskRecord,
  reconcileRecoveredTask,
  type TaskCompletionEvidence,
  transitionTask
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
