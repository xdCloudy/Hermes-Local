import { describe, expect, it } from 'vitest'

import {
  activeModelDownloadForTarget,
  isPausedModelDownload,
  modelDownloadCompletionEvidence,
  modelDownloadProgressPath,
  taskProgressFromModelDownload
} from './hermes-local-model-download'
import { createTaskRecord, transitionTask } from './hermes-local-task-model'

const createdAt = '2026-08-05T08:00:00.000Z'

function task(targetIdentity = 'target-a') {
  return transitionTask(
    createTaskRecord(
      'model-download',
      'download-task',
      { kind: 'desktop-child-process', pid: 42 },
      createdAt,
      { targetIdentity }
    ),
    'running',
    '2026-08-05T08:00:01.000Z'
  )
}

describe('durable model download state', () => {
  it('maps backend bytes, rate and ETA without inventing determinate progress', () => {
    expect(
      taskProgressFromModelDownload({
        message: 'Downloading',
        progress: {
          bytesCompleted: 512,
          bytesTotal: 1024,
          etaSeconds: 2,
          mode: 'determinate',
          pauseSupported: true,
          rateBytesPerSecond: 256
        }
      })
    ).toMatchObject({
      bytesCompleted: 512,
      bytesTotal: 1024,
      completedUnits: 512,
      etaSeconds: 2,
      mode: 'determinate',
      pauseSupported: true,
      percent: null,
      rateBytesPerSecond: 256,
      totalUnits: 1024
    })
    expect(taskProgressFromModelDownload({ progress: { bytesCompleted: 7, mode: 'determinate' } }).mode).toBe(
      'indeterminate'
    )
  })

  it('uses one stable task-specific progress path', () => {
    expect(modelDownloadProgressPath('download-task')).toBe('data/runtime/model-downloads/download-task.json')
    expect(() => modelDownloadProgressPath('../escape')).toThrow(/invalid/i)
  })

  it('recovers terminal evidence and paused state from the backend journal', () => {
    expect(
      modelDownloadCompletionEvidence(task(), {
        completedAt: '2026-08-05T08:05:00.000Z',
        result: { report: 'logs/model-downloads/download-task.json' },
        status: 'succeeded',
        taskId: 'download-task'
      })
    ).toMatchObject({ exitCode: 0, result: { path: 'logs/model-downloads/download-task.json' }, status: 'succeeded' })
    expect(isPausedModelDownload({ status: 'paused' })).toBe(true)
  })

  it('joins only the active download that owns the same target identity', () => {
    expect(activeModelDownloadForTarget([task('target-a')], 'target-a')?.id).toBe('download-task')
    expect(activeModelDownloadForTarget([task('target-a')], 'target-b')).toBeNull()
  })
})
