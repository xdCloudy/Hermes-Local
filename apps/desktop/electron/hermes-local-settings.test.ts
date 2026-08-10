import fs from 'node:fs'
import os from 'node:os'
import path from 'node:path'

import { afterEach, beforeEach, describe, expect, it } from 'vitest'

import {
  deleteProfile,
  readLocalConfiguration,
  registerModel,
  removeModel,
  saveProfile,
  saveWorkstationSettings,
  selectModel,
  selectProfile
} from './hermes-local-settings'

let root = ''

function writeJson(relativePath: string, value: unknown) {
  const filePath = path.join(root, relativePath)

  fs.mkdirSync(path.dirname(filePath), { recursive: true })
  fs.writeFileSync(filePath, `${JSON.stringify(value, null, 2)}\n`)
}

function profile(name = 'Balanced') {
  return {
    batch: { logical: 512, physical: 128 },
    contextTokens: 32_768,
    description: 'Portable starter',
    flashAttention: true,
    gpu: { layers: 'auto', vramReserveMiB: 'auto' },
    kvCache: { keyType: 'q8_0', valueType: 'q8_0' },
    name,
    promptCache: true,
    speculativeDecoding: false,
    threads: { batch: 'auto', generation: 'auto' }
  }
}

beforeEach(() => {
  root = fs.mkdtempSync(path.join(os.tmpdir(), 'hermes-local-settings-'))
  writeJson('config/defaults/workstation.json', {
    schemaVersion: 1,
    network: { host: '127.0.0.1', modelPort: 8011, hermesPort: 9119 },
    runtime: {
      acceleration: 'auto',
      buildParallelism: 'auto',
      cudaArchitecture: 'auto',
      pythonVersion: '3.13',
      verifyModelOnStart: true
    },
    selectedModelId: 'starter',
    selectedProfile: 'Balanced',
    models: []
  })
  writeJson('config/profiles/profiles.json', { schemaVersion: 1, selected: 'Balanced', profiles: [profile()] })
  writeJson('models/manifests/starter.json', {
    alias: 'starter',
    displayName: 'Starter model',
    filename: 'starter.gguf',
    id: 'starter',
    localPath: 'models/starter.gguf',
    metadata: {},
    server: { jinja: true }
  })
  fs.mkdirSync(path.join(root, 'models'), { recursive: true })
  fs.writeFileSync(path.join(root, 'models', 'starter.gguf'), 'fixture')
})

afterEach(() => {
  fs.rmSync(root, { force: true, recursive: true })
})

describe('Hermes Local portable settings', () => {
  it('resolves auto-tuned profile values from the current machine inputs', () => {
    const small = readLocalConfiguration(root, 4, 4096)
    const large = readLocalConfiguration(root, 32, 24_576)

    expect(small.profiles[0].threads).toEqual({ batch: 3, generation: 2 })
    expect(large.profiles[0].threads).toEqual({ batch: 24, generation: 8 })
    expect(small.profiles[0].gpu.vramReserveMiB).toBe(640)
    expect(large.profiles[0].gpu.vramReserveMiB).toBe(3712)
  })

  it('registers and selects an arbitrary GGUF without changing the tracked catalog', () => {
    const customPath = path.join(root, 'outside-model.gguf')

    fs.writeFileSync(customPath, 'custom fixture')

    const registered = registerModel(root, {
      displayName: 'My Custom Model',
      localPath: customPath,
      metadata: { modelMaximumContextTokens: 65_536 }
    })

    selectModel(root, registered.id, 8)

    const configuration = readLocalConfiguration(root, 8)
    const catalog = JSON.parse(fs.readFileSync(path.join(root, 'models', 'manifests', 'starter.json'), 'utf8'))

    expect(configuration.selectedModelId).toBe('my-custom-model')
    expect(configuration.selectedModel.resolvedPath).toBe(customPath)
    expect(catalog.id).toBe('starter')

    removeModel(root, registered.id, 8)
    expect(readLocalConfiguration(root, 8).selectedModelId).toBe('starter')
  })

  it('creates, selects, edits, and deletes user profiles without mutating defaults', () => {
    const custom = {
      ...readLocalConfiguration(root, 8).profiles[0],
      contextTokens: 65_536,
      name: 'Long context'
    }

    saveProfile(root, custom, 8)
    selectProfile(root, 'Long context', 8)
    expect(readLocalConfiguration(root, 8).selectedProfile).toBe('Long context')

    deleteProfile(root, 'Long context', 8)
    expect(readLocalConfiguration(root, 8).selectedProfile).toBe('Balanced')
    expect(
      JSON.parse(fs.readFileSync(path.join(root, 'config', 'profiles', 'profiles.json'), 'utf8')).profiles
    ).toHaveLength(1)
  })

  it('renames the selected profile without duplicating it', () => {
    const renamed = {
      ...readLocalConfiguration(root, 8).profiles[0],
      name: 'Māori kōrero'
    }

    saveProfile(root, renamed, 8, 0, 'Balanced')

    const configuration = readLocalConfiguration(root, 8)

    expect(configuration.profiles.map(entry => entry.name)).toEqual(['Māori kōrero'])
    expect(configuration.selectedProfile).toBe('Māori kōrero')
  })

  it('rejects rename collisions and missing rename sources without changing profiles', () => {
    const balanced = readLocalConfiguration(root, 8).profiles[0]

    saveProfile(root, { ...balanced, name: 'Existing' }, 8)

    expect(() => saveProfile(root, { ...balanced, name: 'Existing' }, 8, 0, 'Balanced')).toThrow(/already exists/i)
    expect(() => saveProfile(root, { ...balanced, name: 'Replacement' }, 8, 0, 'Missing')).toThrow(/does not exist/i)
    expect(readLocalConfiguration(root, 8).profiles.map(entry => entry.name)).toEqual(['Balanced', 'Existing'])
  })

  it('persists configurable loopback ports and rejects unsafe network settings', () => {
    saveWorkstationSettings(
      root,
      {
        network: { host: '::1', modelPort: 18_011, hermesPort: 19_119 },
        runtime: {
          acceleration: 'cpu',
          buildParallelism: 12,
          cudaArchitecture: '89',
          pythonVersion: '3.12',
          verifyModelOnStart: false
        }
      },
      8
    )

    const saved = readLocalConfiguration(root, 8)

    expect(saved.network).toEqual({
      hermesPort: 19_119,
      host: '::1',
      modelPort: 18_011
    })
    expect(saved.runtime).toEqual({
      acceleration: 'cpu',
      buildParallelism: 12,
      cudaArchitecture: '89',
      pythonVersion: '3.12',
      verifyModelOnStart: false
    })
    expect(() =>
      saveWorkstationSettings(
        root,
        {
          network: { host: '0.0.0.0', modelPort: 8011, hermesPort: 8011 }
        },
        8
      )
    ).toThrow(/loopback|different ports/i)
  })
})
