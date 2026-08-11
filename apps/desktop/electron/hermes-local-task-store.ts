import fs from 'node:fs'
import path from 'node:path'

import { isTaskTerminal, restoreTaskRecord, type TaskRecord } from './hermes-local-task-model'

export const TASK_STORE_SCHEMA_VERSION = 1 as const

export interface TaskStoreLoadResult {
  records: TaskRecord[]
  warnings: string[]
}

function boundedRecords(records: Iterable<TaskRecord>, maximumTerminal: number): TaskRecord[] {
  if (!Number.isInteger(maximumTerminal) || maximumTerminal < 0) {
    throw new Error('Task history bound must be a non-negative integer')
  }

  const all = [...records]
  const terminal = all.filter(record => isTaskTerminal(record.status))
  const retainedTerminalIds = new Set(
    (maximumTerminal === 0 ? [] : terminal.slice(-maximumTerminal)).map(record => record.id)
  )

  return all.filter(record => !isTaskTerminal(record.status) || retainedTerminalIds.has(record.id))
}

export function serializeTaskStore(
  records: Iterable<TaskRecord>,
  updatedAt: string,
  maximumTerminal: number
): string {
  return `${JSON.stringify(
    {
      schemaVersion: TASK_STORE_SCHEMA_VERSION,
      tasks: boundedRecords(records, maximumTerminal),
      updatedAt
    },
    null,
    2
  )}\n`
}

export function parseTaskStore(text: string, maximumOutput: number, maximumTerminal: number): TaskStoreLoadResult {
  const warnings: string[] = []
  let document: unknown

  try {
    document = JSON.parse(text)
  } catch (error) {
    return {
      records: [],
      warnings: [`Task store JSON is invalid: ${error instanceof Error ? error.message : String(error)}`]
    }
  }

  if (!document || typeof document !== 'object') {
    return { records: [], warnings: ['Task store must be a JSON object'] }
  }

  const candidate = document as Record<string, unknown>

  if (candidate.schemaVersion !== TASK_STORE_SCHEMA_VERSION || !Array.isArray(candidate.tasks)) {
    return { records: [], warnings: ['Task store schema is unsupported or missing its task list'] }
  }

  const records: TaskRecord[] = []
  const ids = new Set<string>()

  for (const value of candidate.tasks) {
    const record = restoreTaskRecord(value, maximumOutput)

    if (!record) {
      warnings.push('Ignored an invalid persisted task record')

      continue
    }

    if (ids.has(record.id)) {
      warnings.push(`Ignored duplicate persisted task '${record.id}'`)

      continue
    }

    ids.add(record.id)
    records.push(record)
  }

  return { records: boundedRecords(records, maximumTerminal), warnings }
}

export function loadTaskStore(
  filePath: string,
  maximumOutput: number,
  maximumTerminal: number
): TaskStoreLoadResult {
  try {
    return parseTaskStore(fs.readFileSync(filePath, 'utf8'), maximumOutput, maximumTerminal)
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === 'ENOENT') {
      return { records: [], warnings: [] }
    }

    return {
      records: [],
      warnings: [`Task store could not be read: ${error instanceof Error ? error.message : String(error)}`]
    }
  }
}

export function saveTaskStore(
  filePath: string,
  records: Iterable<TaskRecord>,
  updatedAt: string,
  maximumTerminal: number
): void {
  const directory = path.dirname(filePath)
  const temporary = `${filePath}.${process.pid}.tmp`

  fs.mkdirSync(directory, { recursive: true })

  try {
    fs.writeFileSync(temporary, serializeTaskStore(records, updatedAt, maximumTerminal), {
      encoding: 'utf8',
      mode: 0o600
    })
    fs.renameSync(temporary, filePath)
  } finally {
    try {
      fs.rmSync(temporary, { force: true })
    } catch {
      // Preserve the successful write or original error if temporary cleanup fails.
    }
  }
}
