import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

import { ModelDownloadCard } from './model-download-card'
import type { LocalActionTask } from './types'

function task(): LocalActionTask {
  return {
    action: 'model-download',
    capabilities: { cancel: true, pause: true, resume: false, retry: false },
    completedAt: null,
    conflictPolicy: 'queue',
    context: { displayName: 'Fixture model', targetIdentity: 'target' },
    createdAt: '2026-08-05T08:00:00.000Z',
    exitCode: null,
    failure: null,
    id: 'download-task',
    output: '',
    outputTruncated: false,
    owner: { kind: 'desktop-child-process', pid: 42 },
    progress: {
      bytesCompleted: 512,
      bytesTotal: 1024,
      cancellable: true,
      completedUnits: 512,
      counters: {},
      etaSeconds: 2,
      message: 'Transferring fixture.gguf.',
      mode: 'determinate',
      pauseSupported: true,
      percent: 50,
      rateBytesPerSecond: 256,
      resumeSupported: false,
      totalUnits: 1024
    },
    queuedAt: '2026-08-05T08:00:00.000Z',
    resources: [{ mode: 'exclusive', resource: 'model-storage' }],
    result: null,
    schemaVersion: 1,
    stage: 'download',
    startedAt: '2026-08-05T08:00:01.000Z',
    status: 'running',
    updatedAt: '2026-08-05T08:00:02.000Z'
  }
}

describe('Model download card', () => {
  it('shows the authoritative task id, stage and transfer metrics', () => {
    render(<ModelDownloadCard onNavigate={vi.fn()} onRefresh={vi.fn()} onTaskError={vi.fn()} tasks={[task()]} />)
    expect(screen.getByText('Task download-task')).toBeTruthy()
    expect(screen.getByText('download')).toBeTruthy()
    expect(screen.getByText('512 B / 1 KiB')).toBeTruthy()
    expect(screen.getByRole('progressbar').getAttribute('aria-valuenow')).toBe('50')
  })

  it('routes pause through the authoritative Desktop task API', async () => {
    const pauseAction = vi.fn(async () => task())
    Object.defineProperty(window, 'hermesDesktop', {
      configurable: true,
      value: { localWorkstation: { pauseAction } }
    })
    render(<ModelDownloadCard onNavigate={vi.fn()} onRefresh={vi.fn()} onTaskError={vi.fn()} tasks={[task()]} />)
    fireEvent.click(screen.getByRole('button', { name: 'Pause' }))
    await waitFor(() => expect(pauseAction).toHaveBeenCalledWith('download-task'))
  })
})
