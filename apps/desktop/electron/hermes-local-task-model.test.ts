import { describe, expect, it } from 'vitest'

import {
  admitTask,
  boundedTaskOutput,
  createTaskRecord,
  reconcileRecoveredTask,
  reconcileTaskOwner,
  requestTaskCancellation,
  type TaskAction,
  taskCapabilities,
  type TaskRecord,
  transitionTask
} from './hermes-local-task-model'

const createdAt = '2026-08-01T10:00:00.000Z'

function running(action: TaskAction, id = `${action}-task`): TaskRecord {
  return transitionTask(
    createTaskRecord(action, id, { kind: 'desktop-child-process', pid: 4242 }, createdAt),
    'running',
    '2026-08-01T10:00:01.000Z'
  )
}

describe('Hermes Local task model', () => {
  it('permits only declared state transitions and records terminal timestamps', () => {
    const task = running('benchmark')
    const completed = transitionTask(task, 'succeeded', '2026-08-01T10:05:00.000Z', { exitCode: 0 })

    expect(completed).toMatchObject({
      completedAt: '2026-08-01T10:05:00.000Z',
      exitCode: 0,
      startedAt: '2026-08-01T10:00:01.000Z',
      status: 'succeeded'
    })
    expect(() => transitionTask(completed, 'running', '2026-08-01T10:05:01.000Z')).toThrow(
      /invalid task transition/i
    )
  })

  it('joins duplicate starts instead of creating a second owner', () => {
    expect(admitTask('benchmark', [running('benchmark', 'existing')])).toEqual({
      kind: 'join',
      message: "Joined existing 'benchmark' task existing",
      taskId: 'existing'
    })
  })

  it('serializes model switches through the exclusive workstation resource', () => {
    const switching = running('switch-model')

    expect(admitTask('switch-model', [switching])).toMatchObject({ kind: 'join', taskId: switching.id })
    expect(admitTask('restart', [switching])).toMatchObject({ kind: 'reject' })
    expect(admitTask('benchmark', [switching])).toMatchObject({ kind: 'reject' })
  })

  it('serializes restore against installation and user-data mutations', () => {
    const restore = running('restore')

    expect(admitTask('restore', [restore])).toMatchObject({ kind: 'join', taskId: restore.id })
    expect(admitTask('backup', [restore])).toMatchObject({
      conflicts: [{ action: 'restore', resources: ['user-data', 'workstation'], taskId: restore.id }],
      kind: 'reject'
    })
    expect(admitTask('repair', [restore])).toMatchObject({ kind: 'reject' })
    expect(admitTask('update', [restore])).toMatchObject({
      conflicts: [{ action: 'restore', resources: ['installation', 'model-storage'], taskId: restore.id }],
      kind: 'reject'
    })
    expect(admitTask('diagnostics', [restore])).toMatchObject({ kind: 'start' })
    expect(admitTask('security', [restore])).toMatchObject({ kind: 'start' })
  })

  it('keeps gateway readiness available while a benchmark owns only the model', () => {
    const benchmark = running('benchmark')

    expect(admitTask('start', [benchmark])).toMatchObject({ kind: 'start' })
    expect(admitTask('restart', [benchmark])).toMatchObject({
      conflicts: [
        {
          action: 'benchmark',
          resources: ['workstation'],
          taskId: 'benchmark-task'
        }
      ],
      kind: 'reject'
    })
  })

  it('never blocks observational diagnostics behind maintenance', () => {
    const update = running('update')

    expect(admitTask('diagnostics', [update])).toMatchObject({ kind: 'start' })
    expect(admitTask('security', [update])).toMatchObject({ kind: 'start' })
  })

  it('allows an automatic update check while workstation startup is in progress', () => {
    const start = running('start')

    expect(admitTask('update', [start])).toMatchObject({ kind: 'start' })
  })

  it('queues automatic readiness but rejects disruptive conflicts', () => {
    const repair = running('repair')

    expect(admitTask('start', [repair])).toMatchObject({ kind: 'queue' })
    expect(admitTask('benchmark', [repair])).toMatchObject({ kind: 'reject' })
  })

  it('cancels queued work immediately and running work cooperatively', () => {
    const queued = createTaskRecord(
      'benchmark',
      'queued',
      { kind: 'desktop-child-process', pid: null },
      createdAt
    )

    const cancelled = requestTaskCancellation(queued, '2026-08-01T10:00:02.000Z')
    const cancelling = requestTaskCancellation(running('test'), '2026-08-01T10:00:03.000Z')

    expect(cancelled.status).toBe('cancelled')
    expect(cancelled.completedAt).toBe('2026-08-01T10:00:02.000Z')
    expect(cancelling.status).toBe('cancelling')
    expect(() => requestTaskCancellation(running('update'), '2026-08-01T10:00:04.000Z')).toThrow(
      /cannot be cancelled/i
    )
  })

  it('interrupts stale owners so their resource locks can be released', () => {
    const task = running('benchmark')
    const interrupted = reconcileTaskOwner(task, false, '2026-08-01T10:01:00.000Z')

    expect(interrupted).toMatchObject({
      completedAt: '2026-08-01T10:01:00.000Z',
      failure: { code: 'owner-exited' },
      status: 'interrupted'
    })
    expect(admitTask('restart', [interrupted])).toMatchObject({ kind: 'start' })
  })

  it('recovers a live owner as external work without releasing its locks', () => {
    const task = running('benchmark')
    const recovered = reconcileRecoveredTask(task, true, null, '2026-08-01T10:01:00.000Z')

    expect(recovered).toMatchObject({
      owner: { kind: 'external-process', pid: 4242 },
      status: 'running'
    })
    expect(admitTask('restart', [recovered])).toMatchObject({ kind: 'reject' })
  })

  it('uses fresh authoritative evidence after a recovered owner exits', () => {
    const task = running('security')

    const recovered = reconcileRecoveredTask(
      task,
      false,
      {
        exitCode: 0,
        failure: null,
        observedAt: '2026-08-01T10:01:00.000Z',
        result: { kind: 'report', path: 'security/reports/latest-scan.json' },
        status: 'succeeded'
      },
      '2026-08-01T10:01:01.000Z'
    )

    expect(recovered).toMatchObject({
      exitCode: 0,
      result: { kind: 'report', path: 'security/reports/latest-scan.json' },
      status: 'succeeded'
    })
  })

  it('prefers authoritative restore completion after a late cancellation request', () => {
    const cancelling = requestTaskCancellation(running('restore'), '2026-08-01T10:00:03.000Z')
    const recovered = reconcileRecoveredTask(
      cancelling,
      false,
      {
        exitCode: 0,
        failure: null,
        observedAt: '2026-08-01T10:01:00.000Z',
        result: { kind: 'report', path: 'logs/restore/restore-task.json' },
        status: 'succeeded'
      },
      '2026-08-01T10:01:01.000Z'
    )

    expect(recovered).toMatchObject({
      exitCode: 0,
      result: { kind: 'report', path: 'logs/restore/restore-task.json' },
      status: 'succeeded'
    })
  })

  it('interrupts recovered work when result evidence is stale', () => {
    const task = running('security')

    const recovered = reconcileRecoveredTask(
      task,
      false,
      {
        exitCode: 0,
        failure: null,
        observedAt: '2026-08-01T09:59:00.000Z',
        result: null,
        status: 'succeeded'
      },
      '2026-08-01T10:01:00.000Z'
    )

    expect(recovered).toMatchObject({
      failure: { code: 'owner-exited-without-result' },
      status: 'interrupted'
    })
  })

  it('marks queued work interrupted after a Desktop restart', () => {
    const queued = createTaskRecord('start', 'queued', { kind: 'desktop-child-process', pid: null }, createdAt)
    const recovered = reconcileRecoveredTask(queued, false, null, '2026-08-01T10:01:00.000Z')

    expect(recovered).toMatchObject({
      failure: { code: 'desktop-restarted-before-start' },
      status: 'interrupted'
    })
  })

  it('bounds output from the tail and reports truncation', () => {
    expect(boundedTaskOutput('1234', '5678', 6)).toEqual({ output: '345678', truncated: true })
    expect(boundedTaskOutput('12', '34', 6)).toEqual({ output: '1234', truncated: false })
  })

  it('offers only controls that are safe for the current owner and state', () => {
    const queued = createTaskRecord('benchmark', 'queued', { kind: 'desktop-child-process', pid: null }, createdAt)
    const local = running('benchmark', 'local')
    const external = { ...running('benchmark', 'external'), owner: { kind: 'external-process', pid: 4242 } } as const
    const failed = transitionTask(external, 'failed', '2026-08-01T10:02:00.000Z')

    expect(taskCapabilities(queued)).toEqual({ cancel: true, pause: false, resume: false, retry: false })
    expect(taskCapabilities(local)).toEqual({ cancel: true, pause: false, resume: false, retry: false })
    expect(taskCapabilities(external).cancel).toBe(false)
    expect(taskCapabilities(failed)).toEqual({ cancel: false, pause: false, resume: false, retry: true })
    const restore = running('restore', 'restore')
    restore.progress = {
      cancellable: false,
      completedUnits: null,
      counters: {},
      message: 'Promoting restored state',
      mode: 'indeterminate',
      percent: null,
      totalUnits: null
    }

    expect(taskCapabilities(restore).cancel).toBe(false)
  })
})
