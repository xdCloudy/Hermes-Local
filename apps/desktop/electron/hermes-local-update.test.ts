import fs from 'node:fs'
import os from 'node:os'
import path from 'node:path'

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

vi.mock('electron', () => ({
  app: {
    getLoginItemSettings: vi.fn(() => ({ openAtLogin: false })),
    isReady: vi.fn(() => true),
    setLoginItemSettings: vi.fn()
  },
  ipcMain: { handle: vi.fn() },
  session: { fromPartition: vi.fn() },
  WebContentsView: vi.fn(),
  shell: {
    openExternal: vi.fn(),
    openPath: vi.fn()
  }
}))

import { hermesLocalControlTest } from './hermes-local-control'
import { createTaskRecord, reconcileRecoveredTask, transitionTask } from './hermes-local-task-model'

const originalRoot = process.env.HERMES_LOCAL_ROOT
let testRoot = ''

function writeJson(relativePath: string, value: unknown) {
  const filePath = path.join(testRoot, relativePath)

  fs.mkdirSync(path.dirname(filePath), { recursive: true })
  fs.writeFileSync(filePath, `${JSON.stringify(value, null, 2)}\n`, 'utf8')
}

describe('Hermes Local transactional Desktop updater', () => {
  beforeEach(() => {
    testRoot = fs.mkdtempSync(path.join(os.tmpdir(), 'hermes-local-update-test-'))
    process.env.HERMES_LOCAL_ROOT = testRoot
    fs.writeFileSync(path.join(testRoot, 'Update-Hermes-Local.ps1'), '', 'utf8')
    fs.writeFileSync(path.join(testRoot, 'Invoke-Hermes-DesktopUpdate.ps1'), '', 'utf8')
  })

  afterEach(() => {
    fs.rmSync(testRoot, { force: true, recursive: true })

    if (originalRoot === undefined) {
      delete process.env.HERMES_LOCAL_ROOT
    } else {
      process.env.HERMES_LOCAL_ROOT = originalRoot
    }
  })

  it('routes Desktop apply through the authoritative typed updater entrypoint', () => {
    const targetCommit = 'a'.repeat(40)
    const args = hermesLocalControlTest.actionArguments('update', {
      mode: 'Apply',
      targetCommit
    })

    expect(args).toContain(path.join(testRoot, 'Invoke-Hermes-DesktopUpdate.ps1'))
    expect(args).toEqual(
      expect.arrayContaining([
        '-Mode',
        'Apply',
        '-Channel',
        'development',
        '-TargetCommit',
        targetCommit,
        '-NonInteractive'
      ])
    )
    expect(() =>
      hermesLocalControlTest.actionArguments('update', {
        mode: 'Apply',
        targetCommit: 'not-a-sha'
      })
    ).toThrow(/40-character SHA/i)
  })

  it('projects specific compatibility and stale-lock evidence for the update UI', () => {
    const operation = hermesLocalControlTest.publicUpdateOperation({
      completedAt: '2026-08-03T08:00:09.000Z',
      createdAt: '2026-08-03T08:00:00.000Z',
      currentStage: null,
      failure: null,
      identity: {
        component: 'HermesAgent',
        mode: 'Compatibility',
        requestedAt: '2026-08-03T08:00:00.000Z'
      },
      operationId: 'operation-1',
      progress: { completed: 2, percent: 100, total: 2 },
      recovery: {
        previousOperationId: 'stale-operation',
        recoveredLockPath: path.join(testRoot, 'data', 'runtime', 'locks', 'recovered.json'),
        staleLockRecovered: true
      },
      reportPath: path.join(testRoot, 'build', 'updates', 'operations', 'operation-1.json'),
      stageResults: {
        compatibility: {
          candidate: 'b'.repeat(40),
          compatible: true,
          current: 'a'.repeat(40)
        }
      },
      status: 'succeeded',
      taskId: 'task-1',
      updatedAt: '2026-08-03T08:00:09.000Z'
    })

    expect(operation).toMatchObject({
      mode: 'Compatibility',
      recovery: { previousOperationId: 'stale-operation', staleLockRecovered: true },
      reportPath: 'build/updates/operations/operation-1.json',
      target: { updateAvailable: true }
    })
  })

  it('recovers the original opaque failure as a typed rolled-back Task Centre result', () => {
    const created = createTaskRecord(
      'update',
      'update-task',
      { kind: 'desktop-child-process', pid: 4242 },
      '2026-08-03T08:00:00.000Z',
      { mode: 'Apply' }
    )
    const running = transitionTask(created, 'running', '2026-08-03T08:00:01.000Z', {
      owner: { kind: 'desktop-child-process', pid: 4242 }
    })
    const reportPath = path.join(testRoot, 'build', 'updates', 'operations', 'operation-2.json')

    writeJson('data/runtime/update-operations/LATEST.json', {
      completedAt: '2026-08-03T08:00:10.000Z',
      failure: {
        activePreserved: true,
        code: 'build-failed',
        message: 'Desktop build failed.',
        rollback: { status: 'succeeded' },
        stage: 'build'
      },
      identity: {
        component: 'HermesAgent',
        mode: 'Apply',
        requestedAt: '2026-08-03T08:00:00.000Z'
      },
      operationId: 'operation-2',
      reportPath,
      status: 'rolled-back',
      taskId: 'update-task',
      updatedAt: '2026-08-03T08:00:10.000Z'
    })
    writeJson('build/updates/operations/operation-2.json', { status: 'rolled-back' })

    const evidence = hermesLocalControlTest.taskCompletionEvidence(running)
    const recovered = reconcileRecoveredTask(
      running,
      false,
      evidence,
      '2026-08-03T08:00:11.000Z'
    )

    expect(evidence).toMatchObject({
      failure: {
        code: 'update-rolled-back',
        message: expect.stringMatching(/previous backend was restored/i)
      },
      result: { kind: 'report', path: 'build/updates/operations/operation-2.json' },
      status: 'failed'
    })
    expect(recovered).toMatchObject({
      failure: { code: 'update-rolled-back' },
      result: { path: 'build/updates/operations/operation-2.json' },
      status: 'failed'
    })
  })
})
