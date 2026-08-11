import fs from 'node:fs'
import os from 'node:os'
import path from 'node:path'

import { afterEach, describe, expect, it } from 'vitest'

import { createTaskRecord, transitionTask } from './hermes-local-task-model'
import { loadTaskStore, parseTaskStore, saveTaskStore, serializeTaskStore } from './hermes-local-task-store'

const roots: string[] = []
const createdAt = '2026-08-01T10:00:00.000Z'

afterEach(() => {
  for (const root of roots.splice(0)) {
    fs.rmSync(root, { force: true, recursive: true })
  }
})

function terminal(id: string) {
  const running = transitionTask(
    createTaskRecord('benchmark', id, { kind: 'desktop-child-process', pid: 4242 }, createdAt),
    'running',
    '2026-08-01T10:00:01.000Z'
  )

  return transitionTask(running, 'succeeded', '2026-08-01T10:01:00.000Z', { exitCode: 0 })
}

describe('Hermes Local durable task store', () => {
  it('round-trips records atomically without serializing process handles', () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'hermes-task-store-'))
    const filePath = path.join(root, 'data', 'runtime', 'desktop-tasks.json')
    roots.push(root)

    const task = transitionTask(
      createTaskRecord('start', 'startup', { kind: 'desktop-child-process', pid: 4242 }, createdAt),
      'running',
      '2026-08-01T10:00:01.000Z'
    )

    saveTaskStore(filePath, [task], '2026-08-01T10:00:02.000Z', 50)
    saveTaskStore(filePath, [task], '2026-08-01T10:00:03.000Z', 50)

    expect(loadTaskStore(filePath, 128 * 1024, 50)).toEqual({ records: [task], warnings: [] })
    expect(fs.readdirSync(path.dirname(filePath))).toEqual(['desktop-tasks.json'])
  })

  it('persists switch target context across Desktop reloads', () => {
    const task = createTaskRecord(
      'switch-model',
      'switch',
      { kind: 'desktop-child-process', pid: null },
      createdAt,
      { previousModelId: 'qwen', targetModelId: 'agents' }
    )
    task.stage = 'starting-target'

    const parsed = parseTaskStore(serializeTaskStore([task], createdAt, 50), 128 * 1024, 50)

    expect(parsed.records[0]).toMatchObject({
      context: { previousModelId: 'qwen', targetModelId: 'agents' },
      stage: 'starting-target'
    })
  })

  it('keeps all active work while bounding terminal history', () => {
    const active = transitionTask(
      createTaskRecord('start', 'active', { kind: 'desktop-child-process', pid: 4242 }, createdAt),
      'running',
      '2026-08-01T10:00:01.000Z'
    )

    const records = [terminal('old'), terminal('new'), active]

    const parsed = parseTaskStore(
      serializeTaskStore(records, '2026-08-01T10:02:00.000Z', 1),
      128 * 1024,
      1
    )

    expect(parsed.records.map(record => record.id)).toEqual(['new', 'active'])

    const activeOnly = parseTaskStore(
      serializeTaskStore(records, '2026-08-01T10:02:00.000Z', 0),
      128 * 1024,
      0
    )

    expect(activeOnly.records.map(record => record.id)).toEqual(['active'])
  })

  it('rejects malformed records and restores canonical resource policy', () => {
    const task = createTaskRecord('benchmark', 'benchmark', { kind: 'desktop-child-process', pid: null }, createdAt)

    const tampered = {
      ...task,
      conflictPolicy: 'queue',
      resources: []
    }

    const parsed = parseTaskStore(
      JSON.stringify({ schemaVersion: 1, tasks: [tampered, { id: 'invalid' }], updatedAt: createdAt }),
      128 * 1024,
      50
    )

    expect(parsed.records).toHaveLength(1)
    expect(parsed.records[0]).toMatchObject({
      conflictPolicy: 'reject',
      resources: [
        { mode: 'shared', resource: 'workstation' },
        { mode: 'exclusive', resource: 'model-runtime' }
      ]
    })
    expect(parsed.warnings).toEqual(['Ignored an invalid persisted task record'])
  })

  it('fails closed on invalid JSON', () => {
    const parsed = parseTaskStore('{broken', 128 * 1024, 50)

    expect(parsed.records).toEqual([])
    expect(parsed.warnings[0]).toMatch(/invalid/i)
  })
})
