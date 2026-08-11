import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

import { filterTasks, TaskCentre, taskElapsed } from './task-centre'
import type { LocalActionTask } from './types'

function task(overrides: Partial<LocalActionTask> = {}): LocalActionTask {
  return {
    action: 'benchmark',
    capabilities: { cancel: true, pause: false, resume: false, retry: false },
    completedAt: null,
    conflictPolicy: 'reject',
    createdAt: '2026-08-01T10:00:00.000Z',
    exitCode: null,
    failure: null,
    id: 'benchmark-running',
    output: 'Preparing benchmark',
    outputTruncated: false,
    owner: { kind: 'desktop-child-process', pid: 4242 },
    queuedAt: '2026-08-01T10:00:00.000Z',
    resources: [{ mode: 'exclusive', resource: 'model-runtime' }],
    result: null,
    schemaVersion: 1,
    startedAt: '2026-08-01T10:00:01.000Z',
    status: 'running',
    updatedAt: '2026-08-01T10:00:02.000Z',
    ...overrides
  }
}

describe('Task Centre', () => {
  it('filters authoritative task records without changing their state', () => {
    const tasks = [
      task(),
      task({ id: 'queued', startedAt: null, status: 'queued' }),
      task({
        capabilities: { cancel: false, pause: false, resume: false, retry: true },
        completedAt: '2026-08-01T10:02:00.000Z',
        failure: { code: 'process-exit', message: 'Process failed' },
        id: 'failed',
        status: 'failed'
      })
    ]

    expect(filterTasks(tasks, 'active').map(item => item.id)).toEqual(['benchmark-running', 'queued'])
    expect(filterTasks(tasks, 'failed').map(item => item.id)).toEqual(['failed'])
    expect(tasks.map(item => item.status)).toEqual(['running', 'queued', 'failed'])
  })

  it('formats elapsed time from authoritative lifecycle timestamps', () => {
    expect(taskElapsed(task(), Date.parse('2026-08-01T10:01:12.000Z'))).toBe('1m 11s')
    expect(
      taskElapsed(
        task({ completedAt: '2026-08-01T12:05:00.000Z', status: 'succeeded' }),
        Date.parse('2026-08-01T15:00:00.000Z')
      )
    ).toBe('2h 4m')
  })

  it('keeps an empty filtered list and detail pane consistent', () => {
    render(
      <TaskCentre
        modelName="Portable model"
        onCancel={vi.fn(async () => undefined)}
        onError={vi.fn()}
        onNavigate={vi.fn()}
        onOpenResult={vi.fn(async () => undefined)}
        onRetry={vi.fn(async () => undefined)}
        profileName="Balanced"
        tasks={[task()]}
      />
    )

    fireEvent.click(screen.getByRole('tab', { name: 'Failed (0)' }))

    expect(screen.getAllByText('No failed tasks')).toHaveLength(2)
    expect(screen.queryByRole('heading', { name: 'Benchmark' })).toBeNull()
  })

  it('supports arrow-key navigation across task filters', () => {
    render(
      <TaskCentre
        modelName="Portable model"
        onCancel={vi.fn(async () => undefined)}
        onError={vi.fn()}
        onNavigate={vi.fn()}
        onOpenResult={vi.fn(async () => undefined)}
        onRetry={vi.fn(async () => undefined)}
        profileName="Balanced"
        tasks={[task()]}
      />
    )

    fireEvent.keyDown(screen.getByRole('tab', { name: 'All (1)' }), { key: 'ArrowRight' })

    expect(screen.getByRole('tab', { name: 'Active (1)' }).getAttribute('aria-selected')).toBe('true')
    expect(screen.getByRole('tabpanel').getAttribute('aria-labelledby')).toBe('task-filter-active')
  })

  it('shows capability-gated controls and retries a selected failed task', async () => {
    const retry = vi.fn(async () => undefined)

    const failed = task({
      action: 'security',
      capabilities: { cancel: false, pause: false, resume: false, retry: true },
      completedAt: '2026-08-01T10:02:00.000Z',
      createdAt: '2026-08-01T10:01:00.000Z',
      failure: { code: 'scan-failed', message: 'Scan found a blocking issue' },
      id: 'security-failed',
      status: 'failed'
    })

    render(
      <TaskCentre
        modelName="Portable model"
        onCancel={vi.fn(async () => undefined)}
        onError={vi.fn()}
        onNavigate={vi.fn()}
        onOpenResult={vi.fn(async () => undefined)}
        onRetry={retry}
        profileName="Balanced"
        tasks={[task(), failed]}
      />
    )

    expect(screen.getByRole<HTMLButtonElement>('button', { name: 'Cancel' }).disabled).toBe(false)
    expect(screen.getByRole<HTMLButtonElement>('button', { name: 'Pause' }).disabled).toBe(true)
    expect(screen.getByRole<HTMLButtonElement>('button', { name: 'Resume' }).disabled).toBe(true)

    fireEvent.click(screen.getByRole('button', { name: /Security scan/ }))
    expect(screen.getByRole<HTMLButtonElement>('button', { name: 'Cancel' }).disabled).toBe(true)
    fireEvent.click(screen.getByRole('button', { name: 'Retry' }))

    await waitFor(() => expect(retry).toHaveBeenCalledWith('security-failed'))
  })

  it('shows authoritative security task identity, stage and real progress counters', () => {
    const security = task({
      action: 'security',
      id: 'security-task-25',
      output: 'Security scan progress: discovery',
      progress: {
        completedUnits: 4,
        counters: { checks: 4, findings: 2, targets: 1 },
        message: 'Dependency checks completed.',
        mode: 'determinate',
        percent: 50,
        totalUnits: 8
      },
      stage: 'discovery'
    })

    render(
      <TaskCentre
        modelName="Portable model"
        onCancel={vi.fn(async () => undefined)}
        onError={vi.fn()}
        onNavigate={vi.fn()}
        onOpenResult={vi.fn(async () => undefined)}
        onRetry={vi.fn(async () => undefined)}
        profileName="Balanced"
        tasks={[security]}
      />
    )

    expect(screen.getByText('security-task-25')).toBeTruthy()
    expect(screen.getAllByText('discovery').length).toBeGreaterThan(0)
    expect(screen.getByText('checks 4/8')).toBeTruthy()
    expect(screen.getByText('findings 2')).toBeTruthy()
    expect(screen.getByText('targets 1')).toBeTruthy()
    expect(screen.getByRole('progressbar').getAttribute('aria-valuenow')).toBe('50')
    expect(screen.getByText('Dependency checks completed.')).toBeTruthy()
  })

})
