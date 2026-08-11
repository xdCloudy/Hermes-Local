import { EventEmitter } from 'node:events'
import fs from 'node:fs'
import os from 'node:os'
import path from 'node:path'

import { app } from 'electron'
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

const originalRoot = process.env.HERMES_LOCAL_ROOT
let testRoot = ''

function validProfile() {
  return {
    batch: { logical: 1024, physical: 256 },
    contextTokens: 65_536,
    description: 'Interactive local profile',
    flashAttention: true,
    gpu: { layers: 'auto', vramReserveMiB: 1536 },
    kvCache: { keyType: 'q8_0', valueType: 'q8_0' },
    name: 'Daily',
    promptCache: true,
    speculativeDecoding: false,
    threads: { batch: 14, generation: 8 }
  }
}

describe('Hermes Local Electron boundary', () => {
  beforeEach(() => {
    testRoot = fs.mkdtempSync(path.join(os.tmpdir(), 'hermes-local-control-test-'))
    process.env.HERMES_LOCAL_ROOT = testRoot
  })

  afterEach(() => {
    fs.rmSync(testRoot, { force: true, recursive: true })

    if (originalRoot === undefined) {
      delete process.env.HERMES_LOCAL_ROOT
    } else {
      process.env.HERMES_LOCAL_ROOT = originalRoot
    }
  })

  it('converts zoomed renderer bounds into native Desktop coordinates', () => {
    const rendererBounds = { height: 780, width: 1430, x: 470, y: 314 }

    expect(hermesLocalControlTest.dashboardBoundsAtZoom(rendererBounds, 0.9)).toEqual({
      height: 702,
      width: 1287,
      x: 423,
      y: 283
    })
    expect(hermesLocalControlTest.dashboardBoundsAtZoom(rendererBounds, 1)).toEqual(rendererBounds)
    expect(hermesLocalControlTest.dashboardBoundsAtZoom(rendererBounds, 1.25)).toEqual({
      height: 975,
      width: 1787,
      x: 588,
      y: 393
    })
  })

  it('falls back to unscaled dashboard bounds for an invalid zoom factor', () => {
    const rendererBounds = { height: 780, width: 1430, x: 470, y: 314 }

    expect(hermesLocalControlTest.dashboardBoundsAtZoom(rendererBounds, Number.NaN)).toEqual(rendererBounds)
    expect(hermesLocalControlTest.dashboardBoundsAtZoom(rendererBounds, 0)).toEqual(rendererBounds)
    expect(hermesLocalControlTest.dashboardBoundsAtZoom(rendererBounds, -1)).toEqual(rendererBounds)
  })

  it('rejects relative roots and paths that escape the fixed workstation root', () => {
    process.env.HERMES_LOCAL_ROOT = 'relative-root'
    expect(() => hermesLocalControlTest.localRoot()).toThrow(/must be absolute/i)

    process.env.HERMES_LOCAL_ROOT = testRoot
    expect(() => hermesLocalControlTest.resolveUnderRoot('..\\outside.txt')).toThrow(/escapes/i)
    expect(hermesLocalControlTest.resolveUnderRoot('logs\\launcher\\launcher.log')).toBe(
      path.join(testRoot, 'logs', 'launcher', 'launcher.log')
    )
  })

  it('redacts bearer tokens, named secrets, and long credential-like values', () => {
    const secret = 'A'.repeat(64)

    const output = hermesLocalControlTest.redacted(
      `Authorization: Bearer abc.def.ghi token=${secret} password=hunter2 visible=healthy`
    )

    expect(output).not.toContain('abc.def.ghi')
    expect(output).not.toContain(secret)
    expect(output).not.toContain('hunter2')
    expect(output).toContain('visible=healthy')
  })

  it('returns only the profile fields that the supervisor understands', () => {
    const sanitized = hermesLocalControlTest.sanitizeEditableProfile({
      ...validProfile(),
      command: 'Remove-Item',
      env: { PATH: 'unsafe' },
      seed: 3407
    }) as Record<string, unknown>

    expect(sanitized).toMatchObject(validProfile())
    expect(sanitized.seed).toBe(3407)
    expect(sanitized).not.toHaveProperty('command')
    expect(sanitized).not.toHaveProperty('env')
  })

  it('rejects unsafe names and out-of-range resource settings', () => {
    expect(() => hermesLocalControlTest.sanitizeEditableProfile({ ...validProfile(), name: '..\\escape' })).toThrow(
      /invalid profile name/i
    )
    expect(() => hermesLocalControlTest.sanitizeEditableProfile({ ...validProfile(), contextTokens: 1024 })).toThrow(
      /context must be an integer/i
    )
  })

  it('uses a current-user Windows login item and rejects renderer type confusion', () => {
    hermesLocalControlTest.setLoginItem(true)

    expect(app.setLoginItemSettings).toHaveBeenCalledWith({
      args: ['--hermes-local-autostart'],
      openAtLogin: true,
      path: process.execPath
    })
    expect(() => hermesLocalControlTest.setLoginItem('true')).toThrow(/must be a boolean/i)
  })

  it('waits for the workstation start action to finish before connecting', async () => {
    const events = new EventEmitter()

    const task = {
      action: 'start',
      child: null,
      completedAt: null,
      conflictPolicy: 'queue',
      createdAt: new Date().toISOString(),
      exitCode: null,
      failure: null,
      id: 'startup-task',
      input: {},
      output: '',
      outputTruncated: false,
      owner: { kind: 'desktop-child-process', pid: null },
      queuedAt: new Date().toISOString(),
      resources: [{ mode: 'shared', resource: 'workstation' }],
      result: null,
      schemaVersion: 1,
      startedAt: new Date().toISOString(),
      status: 'running',
      updatedAt: new Date().toISOString(),
      events
    }

    let settled = false

    const waiting = hermesLocalControlTest.waitForActionTask(task as never).then(() => {
      settled = true
    })

    await Promise.resolve()
    expect(settled).toBe(false)

    task.status = 'succeeded'
    events.emit('terminal')
    await waiting

    expect(settled).toBe(true)
  })

  it('retains active tasks while bounding completed task history', () => {
    const taskMap = new Map<string, { status: string }>()

    for (let index = 0; index < 55; index += 1) {
      taskMap.set(`completed-${index}`, { status: 'succeeded' })
    }

    taskMap.set('running', { status: 'running' })
    taskMap.set('queued', { status: 'queued' })
    hermesLocalControlTest.pruneCompletedTasks(taskMap as never, 50)

    expect(taskMap.size).toBe(52)
    expect(taskMap.has('running')).toBe(true)
    expect(taskMap.has('queued')).toBe(true)
    expect(taskMap.has('completed-0')).toBe(false)
    expect(taskMap.has('completed-54')).toBe(true)
  })

  it('recovers authoritative report and runtime completion evidence', () => {
    const diagnostics = path.join(testRoot, 'logs', 'diagnostics')
    const runtime = path.join(testRoot, 'data', 'runtime')
    fs.mkdirSync(diagnostics, { recursive: true })
    fs.mkdirSync(runtime, { recursive: true })
    fs.writeFileSync(
      path.join(diagnostics, 'latest-test.json'),
      JSON.stringify({ generatedAt: new Date().toISOString(), passed: true, schemaVersion: 1 })
    )
    fs.writeFileSync(
      path.join(runtime, 'status.json'),
      JSON.stringify({ controllerPid: process.pid, phase: 'running', updatedAt: new Date().toISOString() })
    )

    const task = {
      action: 'test',
      createdAt: new Date(Date.now() - 2000).toISOString(),
      startedAt: new Date(Date.now() - 1000).toISOString()
    }

    const start = { ...task, action: 'start' }

    expect(hermesLocalControlTest.taskCompletionEvidence(task as never)).toMatchObject({
      result: { kind: 'report', path: 'logs/diagnostics/latest-test.json' },
      status: 'succeeded'
    })
    expect(hermesLocalControlTest.taskCompletionEvidence(start as never)).toMatchObject({
      result: { kind: 'runtime-state', path: 'data/runtime/status.json' },
      status: 'succeeded'
    })
  })

  it('checks the model, API, and dashboard using distinct health URLs', async () => {
    const urls: string[] = []

    const probe = vi.fn(async (url: string) => {
      urls.push(url)

      return !url.endsWith('/')
    })

    const result = await hermesLocalControlTest.serviceHealth('http://127.0.0.1:8011', 'http://127.0.0.1:9119', probe)

    expect(urls).toEqual(['http://127.0.0.1:8011/health', 'http://127.0.0.1:9119/api/health', 'http://127.0.0.1:9119/'])
    expect(result).toEqual({ dashboard: false, hermes: true, model: true })
  })
})
