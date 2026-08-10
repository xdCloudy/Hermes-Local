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
const originalProfile = process.env.USERPROFILE
let testRoot = ''

function writeProgress(value: Record<string, unknown>) {
  const filePath = path.join(testRoot, 'data', 'runtime', 'security-scan-progress.json')

  fs.mkdirSync(path.dirname(filePath), { recursive: true })
  fs.writeFileSync(filePath, `${JSON.stringify(value, null, 2)}\n`, 'utf8')
}

function runningTask(id = 'security-task') {
  const created = createTaskRecord(
    'security',
    id,
    { kind: 'desktop-child-process', pid: 4242 },
    '2026-08-03T08:00:00.000Z',
    { defender: 'skipped', mode: 'quick' }
  )

  return transitionTask(created, 'running', '2026-08-03T08:00:01.000Z', {
    owner: { kind: 'desktop-child-process', pid: 4242 }
  })
}

function progress(status: string, overrides: Record<string, unknown> = {}) {
  return {
    completedAt: ['cancelled', 'failed', 'stale', 'succeeded'].includes(status)
      ? '2026-08-03T08:00:10.000Z'
      : null,
    completedChecks: 4,
    counters: { checks: 4, findings: 2, targets: 1 },
    failure: null,
    message: 'Dependency checks completed.',
    mode: 'determinate',
    ownerPid: 4242,
    percent: 50,
    result: {
      directory: 'security/scans/20260803T080000Z',
      findings: 'security/scans/20260803T080000Z/findings.json',
      log: 'security/scans/20260803T080000Z/task.log',
      report: 'security/scans/20260803T080000Z/summary.json'
    },
    stage: 'discovery',
    startedAt: '2026-08-03T08:00:00.000Z',
    status,
    taskId: 'security-task',
    totalChecks: 8,
    updatedAt: '2026-08-03T08:00:10.000Z',
    ...overrides
  }
}

describe('Hermes Local durable security scan tasks', () => {
  beforeEach(() => {
    testRoot = fs.mkdtempSync(path.join(os.tmpdir(), 'hermes-local-security-test-'))
    process.env.HERMES_LOCAL_ROOT = testRoot
    process.env.USERPROFILE = path.join(testRoot, 'private-user')
    fs.writeFileSync(path.join(testRoot, 'Security-Scan-Hermes-Local.ps1'), '', 'utf8')
  })

  afterEach(() => {
    fs.rmSync(testRoot, { force: true, recursive: true })

    if (originalRoot === undefined) {
      delete process.env.HERMES_LOCAL_ROOT
    } else {
      process.env.HERMES_LOCAL_ROOT = originalRoot
    }

    if (originalProfile === undefined) {
      delete process.env.USERPROFILE
    } else {
      process.env.USERPROFILE = originalProfile
    }
  })

  it('starts quick and full scans through validated owned-process arguments', () => {
    const quick = hermesLocalControlTest.actionArguments('security', { quick: true, skipDefender: true })
    const full = hermesLocalControlTest.actionArguments('security', { quick: false, skipDefender: false })

    expect(quick).toEqual(expect.arrayContaining(['-Quick', '-SkipDefender', '-NonInteractive']))
    expect(quick).toContain(path.join(testRoot, 'Security-Scan-Hermes-Local.ps1'))
    expect(full).not.toContain('-Quick')
    expect(full).not.toContain('-SkipDefender')
  })

  it('projects real phases and counters into the shared task progress shape', () => {
    expect(hermesLocalControlTest.taskProgressFromSecurityDocument(progress('running'))).toEqual({
      completedUnits: 4,
      counters: { checks: 4, findings: 2, targets: 1 },
      message: 'Dependency checks completed.',
      mode: 'determinate',
      percent: 50,
      totalUnits: 8
    })
    expect(hermesLocalControlTest.securityProgressSummary(progress('running'))).toMatch(
      /discovery · 4 checks · 2 findings · 1 targets/
    )
  })

  it.each([
    ['succeeded', 'succeeded', 0, null],
    ['cancelled', 'cancelled', 130, null],
    [
      'failed',
      'failed',
      1,
      { code: 'security-active-checks-failed', message: 'Semgrep failed safely.' }
    ],
    [
      'stale',
      'interrupted',
      null,
      { code: 'security-scan-stale', message: 'Recovered security scan marker is stale' }
    ]
  ] as const)('maps %s progress into terminal task evidence', (sourceStatus, expectedStatus, exitCode, failure) => {
    writeProgress(
      progress(sourceStatus, {
        failure:
          sourceStatus === 'failed'
            ? { code: 'security-active-checks-failed', message: 'Semgrep failed safely.' }
            : sourceStatus === 'stale'
              ? { code: 'security-scan-stale', message: 'Recovered security scan marker is stale' }
              : null
      })
    )

    const evidence = hermesLocalControlTest.securityCompletionEvidence(runningTask())

    expect(evidence).toMatchObject({
      exitCode,
      failure,
      result: { kind: 'report', path: 'security/scans/20260803T080000Z' },
      status: expectedStatus
    })
  })

  it('recovers a cancelled scan after Desktop restart without leaving it running', () => {
    writeProgress(progress('cancelled'))
    const running = runningTask()
    const recovered = reconcileRecoveredTask(
      running,
      false,
      hermesLocalControlTest.securityCompletionEvidence(running),
      '2026-08-03T08:00:11.000Z'
    )

    expect(recovered.status).toBe('cancelled')
    expect(recovered.exitCode).toBe(130)
    expect(recovered.result?.path).toBe('security/scans/20260803T080000Z')
  })

  it('redacts credentials, private targets and personal paths from bounded task output', () => {
    const secret = 'A'.repeat(64)
    const privatePath = path.join(process.env.USERPROFILE || '', 'targets', 'internal.txt')
    const output = hermesLocalControlTest.redacted(
      `Authorization: Bearer ${secret}\napi_key=${secret}\nhttps://alice:password@10.0.0.4/private\n${privatePath}`
    )

    expect(output).not.toContain(secret)
    expect(output).not.toContain('10.0.0.4')
    expect(output).not.toContain(privatePath)
    expect(output).toContain('[PRIVATE-TARGET]')
    expect(output).toContain('[PRIVATE-PATH]')
  })
})
