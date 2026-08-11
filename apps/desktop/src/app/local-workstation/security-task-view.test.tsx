import { describe, expect, it } from 'vitest'

import { latestSecurityTask } from './security-task'
import type { LocalActionTask } from './types'

function task(id: string, createdAt: string, action: LocalActionTask['action'] = 'security'): LocalActionTask {
  return {
    action,
    capabilities: { cancel: true, pause: false, resume: false, retry: false },
    completedAt: null,
    conflictPolicy: 'reject',
    createdAt,
    exitCode: null,
    failure: null,
    id,
    output: '',
    outputTruncated: false,
    owner: { kind: 'desktop-child-process', pid: 4242 },
    progress: null,
    queuedAt: createdAt,
    resources: [],
    result: null,
    schemaVersion: 1,
    stage: 'discovery',
    startedAt: createdAt,
    status: 'running',
    updatedAt: createdAt
  }
}

describe('Security page task selection', () => {
  it('uses the same newest authoritative security task record as Task Centre', () => {
    const older = task('security-old', '2026-08-03T08:00:00.000Z')
    const benchmark = task('benchmark-newer', '2026-08-03T10:00:00.000Z', 'benchmark')
    const newest = task('security-current', '2026-08-03T09:00:00.000Z')

    expect(latestSecurityTask([older, benchmark, newest])).toBe(newest)
    expect(latestSecurityTask([benchmark])).toBeUndefined()
  })
})
