import fs from 'node:fs'
import path from 'node:path'

const MODEL_ID = /^[a-z0-9][a-z0-9._-]{0,63}$/
const MODEL_ALIAS = /^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$/
const PROFILE_NAME = /^\p{L}[\p{L}\p{M}\p{N} _-]{0,63}$/u

type JsonObject = Record<string, any>

function readJson<T>(filePath: string, optional = false): T | null {
  try {
    return JSON.parse(fs.readFileSync(filePath, 'utf8')) as T
  } catch (error) {
    if (optional && (error as NodeJS.ErrnoException).code === 'ENOENT') {
      return null
    }

    throw error
  }
}

function clone<T>(value: T): T {
  return structuredClone(value)
}

function resolveModelPath(root: string, modelPath: string): string {
  const expanded = modelPath.replace(/%([^%]+)%/g, (_match, name: string) => process.env[name] || `%${name}%`)

  return path.resolve(path.isAbsolute(expanded) ? expanded : path.join(root, expanded))
}

function atomicWrite(filePath: string, value: unknown): void {
  fs.mkdirSync(path.dirname(filePath), { recursive: true })
  const temporary = `${filePath}.${process.pid}.${Date.now()}.tmp`

  fs.writeFileSync(temporary, `${JSON.stringify(value, null, 2)}\n`, { encoding: 'utf8', flag: 'wx' })
  fs.renameSync(temporary, filePath)
}

function userSettingsPath(root: string): string {
  return path.join(root, 'config', 'launcher', 'user-settings.json')
}

function readUserSettings(root: string): JsonObject {
  const value = readJson<JsonObject>(userSettingsPath(root), true) || { schemaVersion: 1 }

  if (value.schemaVersion !== 1) {
    throw new Error(`Unsupported user settings schema version: ${String(value.schemaVersion)}`)
  }

  return value
}

function saveUserSettings(root: string, settings: JsonObject): void {
  atomicWrite(userSettingsPath(root), { ...settings, schemaVersion: 1 })
}

function autoTuning(logicalProcessors: number, vramMiB: number) {
  const processors = Math.max(1, Math.floor(logicalProcessors))
  const generationThreads = Math.max(1, Math.min(8, Math.floor(processors / 2)))
  const batchThreads = Math.max(generationThreads, Math.min(processors, Math.floor(processors * 0.75)))
  const vramReserveMiB = vramMiB > 0 ? Math.max(512, Math.min(4096, Math.round((vramMiB * 0.15) / 128) * 128)) : 1024

  return { batchThreads, generationThreads, logicalProcessors: processors, vramMiB, vramReserveMiB }
}

function resolvedProfile(profileValue: unknown, tuning: ReturnType<typeof autoTuning>) {
  const profile = clone(profileValue as JsonObject)

  if (profile.threads?.generation === 'auto') {
    profile.threads.generation = tuning.generationThreads
  }

  if (profile.threads?.batch === 'auto') {
    profile.threads.batch = tuning.batchThreads
  }

  if (profile.gpu?.vramReserveMiB === 'auto') {
    profile.gpu.vramReserveMiB = tuning.vramReserveMiB
  }

  return sanitizeEditableProfile(profile)
}

export function validProfileName(value: unknown): string {
  const profile = String(value || '').trim()

  if (!PROFILE_NAME.test(profile)) {
    throw new Error('Invalid profile name')
  }

  return profile
}

export function sanitizeEditableProfile(value: unknown) {
  if (!value || typeof value !== 'object') {
    throw new Error('Profile must be an object')
  }

  const candidate = clone(value as JsonObject)
  const name = validProfileName(candidate.name)

  const integer = (input: unknown, minimum: number, maximum: number, label: string) => {
    const parsed = Number(input)

    if (!Number.isSafeInteger(parsed) || parsed < minimum || parsed > maximum) {
      throw new Error(`${label} must be an integer from ${minimum} to ${maximum}`)
    }

    return parsed
  }

  const batch = candidate.batch && typeof candidate.batch === 'object' ? candidate.batch : {}
  const gpu = candidate.gpu && typeof candidate.gpu === 'object' ? candidate.gpu : {}
  const kvCache = candidate.kvCache && typeof candidate.kvCache === 'object' ? candidate.kvCache : {}
  const threads = candidate.threads && typeof candidate.threads === 'object' ? candidate.threads : {}

  const description = String(candidate.description || '')
    .trim()
    .slice(0, 240)

  const keyType =
    typeof kvCache.keyType === 'string' && ['f16', 'q8_0', 'q4_0'].includes(kvCache.keyType) ? kvCache.keyType : 'q8_0'

  const valueType =
    typeof kvCache.valueType === 'string' && ['f16', 'q8_0', 'q4_0'].includes(kvCache.valueType)
      ? kvCache.valueType
      : 'q8_0'

  const profile: JsonObject = {
    batch: {
      logical: integer(batch.logical, 32, 65_536, 'Batch size'),
      physical: integer(batch.physical, 16, 16_384, 'Micro-batch size')
    },
    contextTokens: integer(candidate.contextTokens, 2_048, 4_194_304, 'Context'),
    description,
    flashAttention: Boolean(candidate.flashAttention),
    gpu: {
      layers: gpu.layers === 'auto' ? 'auto' : integer(gpu.layers, 0, 9_999, 'GPU layers'),
      vramReserveMiB: integer(gpu.vramReserveMiB, 0, 131_072, 'VRAM reserve')
    },
    kvCache: { keyType, valueType },
    name,
    promptCache: Boolean(candidate.promptCache),
    speculativeDecoding: Boolean(candidate.speculativeDecoding),
    threads: {
      batch: integer(threads.batch, 1, 512, 'Batch threads'),
      generation: integer(threads.generation, 1, 512, 'Generation threads')
    }
  }

  if (candidate.experimental === true) {
    profile.experimental = true
  }

  if (candidate.seed !== undefined) {
    profile.seed = integer(candidate.seed, 0, 2_147_483_647, 'Seed')
  }

  return profile
}

function modelCatalog(root: string, user: JsonObject) {
  const byId = new Map<string, JsonObject>()
  const userModelIds = new Set<string>()
  const manifestDirectory = path.join(root, 'models', 'manifests')

  if (fs.existsSync(manifestDirectory)) {
    for (const entry of fs.readdirSync(manifestDirectory, { withFileTypes: true })) {
      if (!entry.isFile() || path.extname(entry.name).toLocaleLowerCase() !== '.json') {
        continue
      }

      const model = readJson<JsonObject>(path.join(manifestDirectory, entry.name))

      if (model) {
        byId.set(String(model.id), model)
      }
    }
  }

  for (const model of Array.isArray(user.models) ? user.models : []) {
    byId.set(String(model.id), model)
    userModelIds.add(String(model.id))
  }

  return [...byId.values()].map(value => {
    const model = sanitizeModel(value, root, false)
    const resolvedPath = resolveModelPath(root, model.localPath)
    const installed = fs.existsSync(resolvedPath) && fs.statSync(resolvedPath).isFile()

    return {
      ...model,
      actualSizeBytes: installed ? fs.statSync(resolvedPath).size : null,
      installed,
      resolvedPath,
      userManaged: userModelIds.has(model.id)
    }
  })
}

function sanitizeModel(value: unknown, root: string, requireInstalled: boolean) {
  if (!value || typeof value !== 'object') {
    throw new Error('Model registration must be an object')
  }

  const candidate = clone(value as JsonObject)
  const localPath = String(candidate.localPath || '').trim()
  const filename = String(candidate.filename || path.basename(localPath)).trim()

  const displayName = String(candidate.displayName || path.basename(filename, path.extname(filename)))
    .trim()
    .slice(0, 120)

  const generatedId = displayName
    .toLocaleLowerCase()
    .replace(/[^a-z0-9._-]+/g, '-')
    .replace(/^[^a-z0-9]+|[^a-z0-9]+$/g, '')
    .slice(0, 64)

  const id = String(candidate.id || generatedId).trim()
  const alias = String(candidate.alias || id).trim()

  if (!MODEL_ID.test(id)) {
    throw new Error('Model id must contain lowercase letters, numbers, dots, underscores, or hyphens')
  }

  if (!MODEL_ALIAS.test(alias)) {
    throw new Error('Model alias contains unsupported characters')
  }

  if (!localPath || path.extname(localPath).toLocaleLowerCase() !== '.gguf') {
    throw new Error('Select a GGUF model file')
  }

  const resolvedPath = resolveModelPath(root, localPath)

  if (requireInstalled && (!fs.existsSync(resolvedPath) || !fs.statSync(resolvedPath).isFile())) {
    throw new Error('The selected GGUF model file does not exist')
  }

  const sha256 = candidate.sha256 ? String(candidate.sha256).trim().toLocaleLowerCase() : null

  if (sha256 && !/^[a-f0-9]{64}$/.test(sha256)) {
    throw new Error('Model SHA-256 must contain exactly 64 hexadecimal characters')
  }

  const extraArguments = Array.isArray(candidate.server?.extraArguments)
    ? candidate.server.extraArguments.map((argument: unknown) => String(argument))
    : []

  const reserved = /^(?:-m|--model|--host|--port|--api-key|--api-key-file|--log-file)(?:=|$)/

  if (extraArguments.some((argument: string) => reserved.test(argument))) {
    throw new Error('Custom model arguments cannot override model, network, authentication, or log ownership')
  }

  return {
    alias,
    displayName,
    filename,
    id,
    license: candidate.license ? String(candidate.license) : null,
    localPath,
    metadata: candidate.metadata && typeof candidate.metadata === 'object' ? candidate.metadata : {},
    repository: candidate.repository ? String(candidate.repository) : null,
    revision: candidate.revision ? String(candidate.revision) : null,
    server: {
      chatTemplate: candidate.server?.chatTemplate ? String(candidate.server.chatTemplate) : null,
      extraArguments,
      jinja: candidate.server?.jinja !== false
    },
    sha256,
    sizeBytes: candidate.sizeBytes ? Number(candidate.sizeBytes) : null,
    source: candidate.source ? String(candidate.source) : null
  }
}

export function readLocalConfiguration(root: string, logicalProcessors: number, vramMiB = 0) {
  const defaults = readJson<JsonObject>(path.join(root, 'config', 'defaults', 'workstation.json'))
  const profileDocument = readJson<JsonObject>(path.join(root, 'config', 'profiles', 'profiles.json'))
  const user = readUserSettings(root)

  if (!defaults || !profileDocument) {
    throw new Error('Hermes Local portable defaults are missing')
  }

  const network = { ...defaults.network, ...(user.network || {}) }
  const runtime = { ...defaults.runtime, ...(user.runtime || {}) }
  const tuning = autoTuning(logicalProcessors, vramMiB)
  const profileSource = Array.isArray(user.profiles) && user.profiles.length ? user.profiles : profileDocument.profiles
  const profiles = profileSource.map((profile: unknown) => resolvedProfile(profile, tuning))
  const models = modelCatalog(root, user)
  const requestedModelId = String(user.selectedModelId || defaults.selectedModelId)
  const selectedProfile = String(user.selectedProfile || defaults.selectedProfile)
  const selectedModel =
    models.find(model => model.id === requestedModelId) ||
    models.find(model => model.installed) ||
    models[0]
  const selectedModelId = selectedModel?.id || requestedModelId

  if (!['127.0.0.1', '::1', 'localhost'].includes(String(network.host))) {
    throw new Error('Hermes Local services must bind to loopback')
  }

  for (const [name, value] of Object.entries({ hermesPort: network.hermesPort, modelPort: network.modelPort })) {
    if (!Number.isSafeInteger(Number(value)) || Number(value) < 1024 || Number(value) > 65_535) {
      throw new Error(`${name} must be an integer from 1024 to 65535`)
    }
  }

  if (Number(network.hermesPort) === Number(network.modelPort)) {
    throw new Error('The model and Hermes services must use different ports')
  }

  if (!['auto', 'cpu', 'cuda'].includes(String(runtime.acceleration))) {
    throw new Error('Unsupported acceleration setting')
  }

  if (!selectedModel) {
    throw new Error('No registered Hermes Local model is available')
  }

  if (!profiles.some((profile: JsonObject) => profile.name === selectedProfile)) {
    throw new Error(`Selected profile '${selectedProfile}' does not exist`)
  }

  return {
    autoTuning: tuning,
    models,
    network: {
      hermesPort: Number(network.hermesPort),
      host: network.host === 'localhost' ? '127.0.0.1' : String(network.host),
      modelPort: Number(network.modelPort)
    },
    profiles,
    runtime,
    selectedModel,
    selectedModelId,
    selectedProfile
  }
}

export function saveProfile(
  root: string,
  value: unknown,
  logicalProcessors: number,
  vramMiB = 0,
  originalNameValue?: unknown
) {
  const profile = sanitizeEditableProfile(value)
  const settings = readUserSettings(root)
  const configuration = readLocalConfiguration(root, logicalProcessors, vramMiB)
  const profiles = configuration.profiles.map((entry: JsonObject) => clone(entry))
  const originalName = originalNameValue === undefined ? undefined : validProfileName(originalNameValue)
  const index = profiles.findIndex((entry: JsonObject) => entry.name === (originalName ?? profile.name))

  if (index >= 0) {
    if (
      originalName &&
      originalName !== profile.name &&
      profiles.some((entry: JsonObject) => entry.name === profile.name)
    ) {
      throw new Error(`Profile '${profile.name}' already exists`)
    }

    profiles[index] = profile
  } else if (originalName) {
    throw new Error(`Profile '${originalName}' does not exist`)
  } else {
    profiles.push(profile)
  }

  settings.profiles = profiles

  if (originalName && configuration.selectedProfile === originalName) {
    settings.selectedProfile = profile.name
  } else {
    settings.selectedProfile ||= configuration.selectedProfile
  }

  saveUserSettings(root, settings)

  return profile
}

export function deleteProfile(root: string, nameValue: unknown, logicalProcessors: number, vramMiB = 0) {
  const name = validProfileName(nameValue)
  const settings = readUserSettings(root)
  const configuration = readLocalConfiguration(root, logicalProcessors, vramMiB)
  const profiles = configuration.profiles.filter((profile: JsonObject) => profile.name !== name)

  if (profiles.length === configuration.profiles.length) {
    throw new Error('Profile does not exist')
  }

  if (!profiles.length) {
    throw new Error('At least one profile is required')
  }

  settings.profiles = profiles

  if (configuration.selectedProfile === name) {
    settings.selectedProfile = profiles[0].name
  }

  saveUserSettings(root, settings)

  return { name, selected: settings.selectedProfile || configuration.selectedProfile }
}

export function selectProfile(root: string, nameValue: unknown, logicalProcessors: number, vramMiB = 0) {
  const name = validProfileName(nameValue)
  const configuration = readLocalConfiguration(root, logicalProcessors, vramMiB)

  if (!configuration.profiles.some((profile: JsonObject) => profile.name === name)) {
    throw new Error('Profile does not exist')
  }

  const settings = readUserSettings(root)

  settings.selectedProfile = name
  saveUserSettings(root, settings)

  return { name }
}

export function registerModel(root: string, value: unknown) {
  const model = sanitizeModel(value, root, true)
  const settings = readUserSettings(root)
  const models = Array.isArray(settings.models) ? settings.models : []
  const index = models.findIndex((entry: JsonObject) => entry.id === model.id)

  if (index >= 0) {
    models[index] = model
  } else {
    models.push(model)
  }

  settings.models = models
  settings.selectedModelId ||= model.id
  saveUserSettings(root, settings)

  return model
}

export function selectModel(root: string, idValue: unknown, logicalProcessors: number) {
  const id = String(idValue || '').trim()

  if (!MODEL_ID.test(id)) {
    throw new Error('Invalid model id')
  }

  const configuration = readLocalConfiguration(root, logicalProcessors)

  if (!configuration.models.some(model => model.id === id)) {
    throw new Error('Model is not registered')
  }

  const settings = readUserSettings(root)

  settings.selectedModelId = id
  saveUserSettings(root, settings)

  return { id }
}

export function removeModel(root: string, idValue: unknown, logicalProcessors: number) {
  const id = String(idValue || '').trim()
  const settings = readUserSettings(root)
  const models = Array.isArray(settings.models) ? settings.models : []
  const next = models.filter((model: JsonObject) => model.id !== id)

  if (next.length === models.length) {
    throw new Error(
      'Built-in catalog models cannot be removed; select another model or remove the manifest from your fork'
    )
  }

  settings.models = next

  if (settings.selectedModelId === id) {
    const configuration = readLocalConfiguration(root, logicalProcessors)
    const fallback = configuration.models.find(model => model.id !== id)

    if (!fallback) {
      throw new Error('At least one model registration is required')
    }

    settings.selectedModelId = fallback.id
  }

  saveUserSettings(root, settings)

  return { id, selected: settings.selectedModelId }
}

export function saveWorkstationSettings(root: string, value: unknown, logicalProcessors: number) {
  if (!value || typeof value !== 'object') {
    throw new Error('Workstation settings must be an object')
  }

  const candidate = value as JsonObject
  const current = readLocalConfiguration(root, logicalProcessors)

  const network = {
    hermesPort: Number(candidate.network?.hermesPort ?? current.network.hermesPort),
    host: String(candidate.network?.host ?? current.network.host),
    modelPort: Number(candidate.network?.modelPort ?? current.network.modelPort)
  }

  const acceleration = String(candidate.runtime?.acceleration ?? current.runtime.acceleration)
  const buildParallelismValue = candidate.runtime?.buildParallelism ?? current.runtime.buildParallelism
  const buildParallelism = buildParallelismValue === 'auto' ? 'auto' : Number(buildParallelismValue)
  const cudaArchitecture = String(candidate.runtime?.cudaArchitecture ?? current.runtime.cudaArchitecture).trim()
  const pythonVersion = String(candidate.runtime?.pythonVersion ?? current.runtime.pythonVersion).trim()
  const verifyModelOnStart = candidate.runtime?.verifyModelOnStart ?? current.runtime.verifyModelOnStart

  if (!['127.0.0.1', '::1', 'localhost'].includes(network.host)) {
    throw new Error('Services must bind to loopback')
  }

  if (
    !Number.isSafeInteger(network.modelPort) ||
    !Number.isSafeInteger(network.hermesPort) ||
    network.modelPort < 1024 ||
    network.modelPort > 65_535 ||
    network.hermesPort < 1024 ||
    network.hermesPort > 65_535 ||
    network.modelPort === network.hermesPort
  ) {
    throw new Error('Choose two different ports from 1024 to 65535')
  }

  if (!['auto', 'cpu', 'cuda'].includes(acceleration)) {
    throw new Error('Unsupported acceleration setting')
  }

  if (
    buildParallelism !== 'auto' &&
    (!Number.isSafeInteger(buildParallelism) || buildParallelism < 1 || buildParallelism > 512)
  ) {
    throw new Error('Build parallelism must be auto or an integer from 1 to 512')
  }

  if (cudaArchitecture !== 'auto' && !/^[0-9]{2,3}$/.test(cudaArchitecture)) {
    throw new Error('CUDA architecture must be auto or a two/three-digit CMake architecture')
  }

  if (!/^[0-9]+\.[0-9]+$/.test(pythonVersion)) {
    throw new Error('Python version must contain a major and minor version, such as 3.13')
  }

  if (typeof verifyModelOnStart !== 'boolean') {
    throw new Error('Model verification setting must be a boolean')
  }

  const settings = readUserSettings(root)

  settings.network = network
  settings.runtime = {
    acceleration,
    buildParallelism,
    cudaArchitecture,
    pythonVersion,
    verifyModelOnStart
  }
  saveUserSettings(root, settings)

  return { network, runtime: settings.runtime }
}

export const hermesLocalSettingsTest = {
  autoTuning,
  readUserSettings,
  resolveModelPath,
  sanitizeModel
}
