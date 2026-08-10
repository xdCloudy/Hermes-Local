export type HermesLocalDashboardPhase =
  | 'authentication'
  | 'loading'
  | 'offline'
  | 'ready'
  | 'restarting'

export interface HermesLocalDashboardState {
  canRetry: boolean
  message: string
  origin: string
  phase: HermesLocalDashboardPhase
  retryCount: number
  visible: boolean
}

export interface HermesLocalDashboardBounds {
  height: number
  width: number
  x: number
  y: number
}

export type LocalAction =
  | 'backup'
  | 'benchmark'
  | 'diagnostics'
  | 'model-download'
  | 'repair'
  | 'restart'
  | 'restore'
  | 'security'
  | 'start'
  | 'stop'
  | 'switch-model'
  | 'test'
  | 'update'

export type LocalLog = 'dashboard' | 'hermes' | 'launcher' | 'model' | 'security' | 'supervisor'

export type LocalUpdateMode = 'Apply' | 'Check' | 'Compatibility' | 'Rollback'

export interface LocalUpdateOperation {
  completedAt: null | string
  currentStage: null | string
  failure: null | {
    activePreserved?: boolean
    code: string
    message: string
    rollback?: { status?: string }
    stage?: string
  }
  mode: LocalUpdateMode
  operationId: string
  progress: {
    completed: number
    percent: number
    total: number
  }
  recovery: {
    previousOperationId: null | string
    recoveredLockPath: null | string
    staleLockRecovered: boolean
  }
  reportPath: null | string
  requestedAt: string
  result: null | Record<string, unknown>
  stageResults: Record<string, unknown>
  status: 'failed' | 'queued' | 'rolled-back' | 'running' | 'succeeded'
  taskId: null | string
  target: null | {
    candidate: null | string
    current: null | string
    updateAvailable: null | boolean
  }
  updatedAt: string
}

export interface LocalActionTask {
  action: LocalAction
  context?: Record<string, string>
  capabilities: {
    cancel: boolean
    pause: boolean
    resume: boolean
    retry: boolean
  }
  completedAt: null | string
  conflictPolicy: 'queue' | 'reject'
  createdAt: string
  exitCode: null | number
  failure: null | {
    code: string
    message: string
  }
  id: string
  output: string
  outputTruncated: boolean
  owner: {
    kind: 'desktop-child-process' | 'external-process'
    pid: null | number
  }
  progress?: null | {
    bytesCompleted?: null | number
    bytesTotal?: null | number
    cancellable?: boolean
    completedUnits: null | number
    counters: Record<string, number>
    etaSeconds?: null | number
    message: null | string
    mode: 'determinate' | 'indeterminate'
    pauseSupported?: boolean
    percent: null | number
    rateBytesPerSecond?: null | number
    resumeSupported?: boolean
    totalUnits: null | number
  }
  queuedAt: string
  resources: {
    mode: 'exclusive' | 'shared'
    resource: 'installation' | 'model-runtime' | 'model-storage' | 'user-data' | 'workstation'
  }[]
  result: null | {
    kind: 'archive' | 'report' | 'runtime-state'
    path: string
  }
  schemaVersion: 1
  stage?: null | string
  startedAt: null | string
  status: 'cancelled' | 'cancelling' | 'failed' | 'interrupted' | 'paused' | 'queued' | 'running' | 'succeeded'
  updatedAt: string
}

export interface LocalLoginItemStatus {
  available: boolean
  enabled: boolean
  executable: string
}

export interface LocalInferenceProfile {
  batch: {
    logical: number
    physical: number
  }
  contextTokens: number
  description: string
  experimental?: boolean
  flashAttention: boolean
  gpu: {
    layers: 'auto' | number
    vramReserveMiB: number
  }
  kvCache: {
    keyType: 'f16' | 'q4_0' | 'q8_0'
    valueType: 'f16' | 'q4_0' | 'q8_0'
  }
  name: string
  promptCache: boolean
  seed?: number
  speculativeDecoding: boolean
  threads: {
    batch: number
    generation: number
  }
}

export interface LocalModel {
  actualSizeBytes: null | number
  alias: string
  displayName: string
  filename: string
  id: string
  installed: boolean
  license: null | string
  localPath: string
  metadata: {
    architecture?: string
    modelMaximumContextTokens?: number
    nativeToolCalling?: boolean
    quantization?: string
    reasoning?: string
  }
  repository: null | string
  resolvedPath: string
  revision: null | string
  server: {
    chatTemplate: null | string
    extraArguments: string[]
    jinja: boolean
  }
  sha256: null | string
  sizeBytes: null | number
  source: null | string
  userManaged: boolean
}

export interface LocalBackup {
  id: string
  modifiedAt: string
  name: string
  path: string
  sha256: null | string
  sizeBytes: number
  verified: boolean
}

export interface LocalWorkstationSettings {
  autoTuning: {
    batchThreads: number
    generationThreads: number
    logicalProcessors: number
    vramMiB: number
    vramReserveMiB: number
  }
  network: {
    hermesPort: number
    host: string
    modelPort: number
  }
  runtime: {
    acceleration: 'auto' | 'cpu' | 'cuda'
    buildParallelism: 'auto' | number
    cudaArchitecture: 'auto' | string
    pythonVersion: string
    verifyModelOnStart: boolean
  }
  selectedModelId: string
  selectedProfile: string
}

export interface LocalWorkstationSnapshot {
  actions: Record<LocalAction, boolean>
  backups: LocalBackup[]
  lifecycle: {
    identityMatches: boolean
    switchingModel: null | {
      previousModelId: string
      stage: null | string
      targetAlias: string
      targetModelId: string
      taskId: string
    }
  }
  generatedAt: string
  gpu: null | {
    memoryFreeMiB: number
    memoryTotalMiB: number
    memoryUsedMiB: number
    name: string
    powerWatts: number
    temperatureCelsius: number
    utilizationPercent: number
  }
  hardware: {
    cpu: string
    logicalProcessors: number
    memoryFreeBytes: number
    memoryTotalBytes: number
  }
  health: {
    dashboard: boolean
    gateway?: {
      checked: boolean
      reachable: boolean
      running: boolean
      state: string
      updatedAt: null | string
    }
    hermes: boolean
    model: boolean
  }
  model: LocalModel
  models: LocalModel[]
  profiles: null | {
    profiles: LocalInferenceProfile[]
    schemaVersion: number
    selected: string
  }
  reports: {
    benchmark: boolean
    security: boolean
  }
  root: string
  storage: {
    memoryFiles: number
    stateDatabaseBytes: number
  }
  startup: LocalLoginItemStatus
  settings: LocalWorkstationSettings
  taskLedger: string
  tasks: LocalActionTask[]
  runtime: {
    controllerAlive: boolean
    controllerPid?: null | number
    hermes?: {
      healthy: boolean
      pid: null | number
      url: string
    }
    hermesAlive: boolean
    message?: string
    model?: {
      healthy: boolean
      pid: null | number
      url: string
    }
    modelAlive: boolean
    identityMismatch?: null | string
    phase?: string
    profile?: string
    restartCount?: number
    selectedModelId?: string
    startedAt?: string
    updatedAt?: string
  }
  updates: {
    installed: {
      baseCommit: null | string
      harnessCommit: null | string
      harnessTree: null | string
      patchCount: number
    }
    latest: LocalUpdateOperation | null
  }
  version: null | {
    product: {
      name: string
      status: string
      version: string
    }
    sources: {
      hermesAgent: {
        branch?: string
        commit: string
        harnessCommit?: string
        harnessTree?: string
        patchSeries?: string
      }
      llamaCpp: { commit: string }
    }
  }
}

export interface LocalLogResult {
  content: string
  name: LocalLog
  path: string
}
