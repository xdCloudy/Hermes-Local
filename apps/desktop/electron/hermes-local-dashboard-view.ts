import {
  app,
  type BrowserWindow,
  type Rectangle,
  session,
  type Session,
  shell,
  WebContentsView
} from 'electron'

const DASHBOARD_PARTITION = 'hermes:local-dashboard'
const RETRY_DELAYS_MS = [750, 1500, 3000, 6000, 10_000]
const TOKEN_RE = /^[A-Za-z0-9_-]{40,128}$/

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

export interface HermesLocalDashboardConfig {
  token: string
  url: string
}

export interface HermesLocalDashboardViewController {
  dispose(): void
  getState(): HermesLocalDashboardState
  handleWindowClosed(window: BrowserWindow): void
  hide(): HermesLocalDashboardState
  isTrustedSender(sender: Electron.WebContents): boolean
  reload(config: HermesLocalDashboardConfig): Promise<HermesLocalDashboardState>
  resize(bounds: HermesLocalDashboardBounds): HermesLocalDashboardState
  show(config: HermesLocalDashboardConfig, bounds: HermesLocalDashboardBounds): Promise<HermesLocalDashboardState>
}

interface DashboardViewLike {
  setBackgroundColor(color: string): void
  setBounds(bounds: Rectangle): void
  setVisible(visible: boolean): void
  webContents: Electron.WebContents
}

interface ControllerDependencies {
  createView?: () => DashboardViewLike
  emitState?: (state: HermesLocalDashboardState) => void
  getSession?: () => Session
  getWindow: () => BrowserWindow | null
  openExternal?: (url: string) => Promise<void>
  setTimeout?: typeof globalThis.setTimeout
  clearTimeout?: typeof globalThis.clearTimeout
}

function loopbackHostname(hostname: string): boolean {
  const normalized = hostname.toLowerCase()

  if (normalized === '[::1]' || normalized === '::1') {
    return true
  }

  const octets = normalized.split('.').map(value => Number(value))

  return (
    octets.length === 4 &&
    octets.every(value => Number.isInteger(value) && value >= 0 && value <= 255) &&
    octets[0] === 127
  )
}

export function normalizeHermesLocalDashboardUrl(rawUrl: string): URL {
  const url = new URL(String(rawUrl || '').trim())

  if (url.protocol !== 'http:' || !loopbackHostname(url.hostname) || !url.port || url.username || url.password) {
    throw new Error('The embedded dashboard requires an explicit HTTP loopback origin')
  }

  url.hash = ''

  return url
}

function comparableOrigin(url: URL): string {
  if (url.protocol === 'ws:') {
    return `http://${url.host}`
  }

  if (url.protocol === 'wss:') {
    return `https://${url.host}`
  }

  return url.origin
}

export function dashboardRequestAllowed(rawUrl: string, allowedOrigin: string): boolean {
  try {
    if (rawUrl === 'about:blank') {
      return true
    }

    if (rawUrl.startsWith('blob:')) {
      return comparableOrigin(new URL(rawUrl.slice('blob:'.length))) === allowedOrigin
    }

    if (rawUrl.startsWith('data:')) {
      return true
    }

    return comparableOrigin(new URL(rawUrl)) === allowedOrigin
  } catch {
    return false
  }
}

export function dashboardAuthenticatedRequestAllowed(rawUrl: string, allowedOrigin: string): boolean {
  try {
    const url = new URL(rawUrl)

    return ['http:', 'https:', 'ws:', 'wss:'].includes(url.protocol) && comparableOrigin(url) === allowedOrigin
  } catch {
    return false
  }
}

export function dashboardNavigationAllowed(rawUrl: string, allowedOrigin: string): boolean {
  try {
    const url = new URL(rawUrl)

    return (url.protocol === 'http:' || url.protocol === 'https:') && url.origin === allowedOrigin
  } catch {
    return false
  }
}

export function safeDashboardBounds(
  input: HermesLocalDashboardBounds,
  contentBounds: Pick<Rectangle, 'height' | 'width'>
): Rectangle {
  const numbers = [input?.x, input?.y, input?.width, input?.height]

  if (!numbers.every(value => Number.isFinite(value))) {
    throw new Error('Dashboard bounds must contain finite numbers')
  }

  const contentWidth = Math.max(1, Math.floor(contentBounds.width))
  const contentHeight = Math.max(1, Math.floor(contentBounds.height))
  const x = Math.min(contentWidth - 1, Math.max(0, Math.round(input.x)))
  const y = Math.min(contentHeight - 1, Math.max(0, Math.round(input.y)))
  const width = Math.min(contentWidth - x, Math.max(1, Math.round(input.width)))
  const height = Math.min(contentHeight - y, Math.max(1, Math.round(input.height)))

  return { height, width, x, y }
}

function defaultView(): DashboardViewLike {
  return new WebContentsView({
    webPreferences: {
      allowRunningInsecureContent: false,
      contextIsolation: true,
      devTools: false,
      navigateOnDragDrop: false,
      nodeIntegration: false,
      partition: DASHBOARD_PARTITION,
      safeDialogs: true,
      sandbox: true,
      spellcheck: false,
      webSecurity: true,
      webviewTag: false
    }
  })
}

function defaultState(): HermesLocalDashboardState {
  return {
    canRetry: true,
    message: 'Preparing the local Hermes dashboard.',
    origin: '',
    phase: 'loading',
    retryCount: 0,
    visible: false
  }
}

export function createHermesLocalDashboardViewController(
  dependencies: ControllerDependencies
): HermesLocalDashboardViewController {
  const createView = dependencies.createView || defaultView
  const emitState = dependencies.emitState || (() => undefined)
  const getSession = dependencies.getSession || (() => session.fromPartition(DASHBOARD_PARTITION, { cache: true }))
  const openExternal = dependencies.openExternal || (url => shell.openExternal(url))
  const schedule = dependencies.setTimeout || globalThis.setTimeout
  const cancel = dependencies.clearTimeout || globalThis.clearTimeout

  let currentConfig: null | { origin: string; token: string; url: string } = null
  let currentWindow: BrowserWindow | null = null
  let view: DashboardViewLike | null = null
  let state = defaultState()
  let bounds: Rectangle = { height: 1, width: 1, x: 0, y: 0 }
  let retryTimer: ReturnType<typeof globalThis.setTimeout> | null = null
  let retryCount = 0
  let sessionInstalled = false
  let disposed = false
  let desiredVisible = false
  let lastMainFrameStatus: null | number = null
  let mainFrameDomReady = false
  let loadPromise: Promise<void> | null = null

  const publish = (next: Partial<HermesLocalDashboardState>): HermesLocalDashboardState => {
    state = { ...state, ...next }
    emitState({ ...state })

    return { ...state }
  }

  const clearRetry = () => {
    if (retryTimer) {
      cancel(retryTimer)
      retryTimer = null
    }
  }

  const setViewVisibility = (visible: boolean) => {
    if (!view) {
      return
    }

    view.setVisible(visible)
    state.visible = visible
  }

  const attachToCurrentWindow = () => {
    const window = dependencies.getWindow()

    if (!window || window.isDestroyed() || !view) {
      return false
    }

    if (currentWindow !== window) {
      currentWindow?.contentView.removeChildView(view as never)
      currentWindow = window
      currentWindow.contentView.addChildView(view as never)
    }

    view.setBounds(safeDashboardBounds(bounds, window.getContentBounds()))
    setViewVisibility(desiredVisible && state.phase === 'ready')

    return true
  }

  const openExternalIfSafe = (rawUrl: string) => {
    try {
      const url = new URL(rawUrl)

      if (url.protocol === 'http:' || url.protocol === 'https:') {
        void openExternal(url.toString()).catch(() => undefined)
      }
    } catch {
      // Invalid and non-web destinations stay blocked.
    }
  }

  const destroyView = () => {
    clearRetry()
    loadPromise = null

    if (!view) {
      return
    }

    currentWindow?.contentView.removeChildView(view as never)
    const contents = view.webContents
    view = null
    currentWindow = null

    if (!contents.isDestroyed()) {
      contents.close({ waitForBeforeUnload: false })
    }
  }

  const scheduleRetry = () => {
    clearRetry()

    if (!currentConfig || disposed || !desiredVisible || state.phase === 'authentication') {
      return
    }

    const delay = RETRY_DELAYS_MS[Math.min(retryCount, RETRY_DELAYS_MS.length - 1)]
    retryCount += 1
    publish({ retryCount })
    retryTimer = schedule(() => {
      retryTimer = null
      void ensureViewAndLoad(true)
    }, delay)
  }

  const markReadyIfUsable = (allowUnknownStatus = false, loadedUrlOverride?: string): boolean => {
    if (!currentConfig || !view || !desiredVisible) {
      return false
    }

    if (state.phase !== 'loading' && state.phase !== 'restarting') {
      return false
    }

    if (!mainFrameDomReady && !allowUnknownStatus) {
      return false
    }

    const loadedUrl = loadedUrlOverride || view.webContents.getURL()

    if (!dashboardNavigationAllowed(loadedUrl, currentConfig.origin)) {
      return false
    }

    if ((!allowUnknownStatus && lastMainFrameStatus === null) || (lastMainFrameStatus ?? 0) >= 400) {
      return false
    }

    clearRetry()
    retryCount = 0
    publish({
      canRetry: false,
      message: 'Connected to the protected loopback dashboard.',
      phase: 'ready',
      retryCount: 0
    })
    attachToCurrentWindow()

    return true
  }

  const installSessionGuards = () => {
    if (sessionInstalled) {
      return
    }

    const dashboardSession = getSession()
    sessionInstalled = true

    dashboardSession.setPermissionRequestHandler((_contents, _permission, callback) => callback(false))
    dashboardSession.setPermissionCheckHandler(() => false)
    dashboardSession.on('will-download', event => event.preventDefault())

    dashboardSession.webRequest.onBeforeRequest({ urls: ['<all_urls>'] }, (details, callback) => {
      callback({ cancel: !currentConfig || !dashboardRequestAllowed(details.url, currentConfig.origin) })
    })

    dashboardSession.webRequest.onBeforeSendHeaders({ urls: ['<all_urls>'] }, (details, callback) => {
      if (!currentConfig || !dashboardAuthenticatedRequestAllowed(details.url, currentConfig.origin)) {
        callback({ requestHeaders: details.requestHeaders })

        return
      }

      callback({
        requestHeaders: {
          ...details.requestHeaders,
          Authorization: `Bearer ${currentConfig.token}`
        }
      })
    })

    dashboardSession.webRequest.onCompleted({ urls: ['<all_urls>'] }, details => {
      if (!currentConfig || !dashboardRequestAllowed(details.url, currentConfig.origin)) {
        return
      }

      if (details.resourceType === 'mainFrame') {
        lastMainFrameStatus = details.statusCode

        if (details.statusCode >= 200 && details.statusCode < 400) {
          mainFrameDomReady = true
          markReadyIfUsable(true, details.url)
        }
      }

      if (details.statusCode === 401 || details.statusCode === 403) {
        clearRetry()
        setViewVisibility(false)
        publish({
          canRetry: true,
          message: 'The local dashboard rejected its protected session. Retry after Hermes finishes starting.',
          phase: 'authentication'
        })
      } else if (details.resourceType === 'mainFrame' && details.statusCode >= 500) {
        setViewVisibility(false)
        publish({
          canRetry: true,
          message: 'The dashboard is restarting or temporarily unavailable.',
          phase: 'offline'
        })
        scheduleRetry()
      }
    })
  }

  const wireView = (nextView: DashboardViewLike) => {
    const contents = nextView.webContents

    nextView.setBackgroundColor('#111315')
    nextView.setVisible(false)
    contents.setAudioMuted(true)

    contents.setWindowOpenHandler(details => {
      if (currentConfig && dashboardNavigationAllowed(details.url, currentConfig.origin)) {
        void contents.loadURL(details.url).catch(() => undefined)
      } else {
        openExternalIfSafe(details.url)
      }

      return { action: 'deny' }
    })

    const guardNavigation = (event: Electron.Event, url: string) => {
      if (currentConfig && dashboardNavigationAllowed(url, currentConfig.origin)) {
        return
      }

      event.preventDefault()
      openExternalIfSafe(url)
    }

    contents.on('will-navigate', guardNavigation)
    contents.on('will-redirect', guardNavigation)
    contents.on('will-attach-webview', event => event.preventDefault())
    contents.on('did-navigate', (_event, url, httpResponseCode) => {
      if (currentConfig && dashboardNavigationAllowed(url, currentConfig.origin) && httpResponseCode > 0) {
        lastMainFrameStatus = httpResponseCode
      }
    })
    contents.on('did-start-navigation', details => {
      if (!details.isMainFrame || details.isSameDocument) {
        return
      }

      lastMainFrameStatus = null
      mainFrameDomReady = false
      setViewVisibility(false)
      publish({
        canRetry: true,
        message: 'Loading the protected loopback dashboard.',
        phase: state.phase === 'restarting' ? 'restarting' : 'loading'
      })
    })
    contents.on('dom-ready', () => {
      mainFrameDomReady = true
      markReadyIfUsable()
    })
    contents.on('did-stop-loading', () => {
      const mainFrameSettled = !contents.isLoadingMainFrame()

      if (mainFrameSettled) {
        mainFrameDomReady = true
      }

      markReadyIfUsable(mainFrameSettled)
    })
    contents.on('did-finish-load', () => {
      mainFrameDomReady = true
      markReadyIfUsable(true)
    })
    contents.on('did-fail-load', (_event, errorCode, _description, _validatedUrl, isMainFrame) => {
      if (!isMainFrame || errorCode === -3) {
        return
      }

      mainFrameDomReady = false
      setViewVisibility(false)
      publish({
        canRetry: true,
        message: 'The local dashboard is offline. Hermes Local will reconnect automatically.',
        phase: 'offline'
      })
      scheduleRetry()
    })
    contents.on('render-process-gone', () => {
      if (nextView !== view) {
        return
      }

      destroyView()
      publish({
        canRetry: true,
        message: 'The dashboard renderer restarted. Reconnecting without closing Desktop.',
        phase: 'restarting',
        visible: false
      })
      scheduleRetry()
    })
    contents.on('unresponsive', () => {
      setViewVisibility(false)
      publish({
        canRetry: true,
        message: 'The dashboard stopped responding. Reconnecting without closing Desktop.',
        phase: 'restarting'
      })
      scheduleRetry()
    })
  }

  const ensureViewAndLoad = async (forceReload = false): Promise<HermesLocalDashboardState> => {
    if (disposed || !currentConfig) {
      return { ...state }
    }

    if (!app.isReady()) {
      throw new Error('The dashboard cannot be embedded before Electron is ready')
    }

    installSessionGuards()

    if (!view || view.webContents.isDestroyed()) {
      view = createView()
      wireView(view)
    }

    attachToCurrentWindow()

    const loadedUrl = view.webContents.getURL()

    if (
      forceReload ||
      !loadedUrl ||
      !dashboardNavigationAllowed(loadedUrl, currentConfig.origin) ||
      state.phase === 'offline' ||
      state.phase === 'restarting'
    ) {
      if (loadPromise) {
        await loadPromise
        attachToCurrentWindow()

        return { ...state }
      }

      setViewVisibility(false)
      publish({
        canRetry: true,
        message: forceReload ? 'Reconnecting to the local dashboard.' : 'Loading the protected loopback dashboard.',
        phase: forceReload ? 'restarting' : 'loading'
      })

      const loadingView = view
      loadPromise = loadingView.webContents
        .loadURL(currentConfig.url)
        .then(() => undefined)
        .catch(() => undefined)
        .finally(() => {
          if (view === loadingView) {
            loadPromise = null
          }
        })
      await loadPromise
    } else {
      if (!view.webContents.isLoadingMainFrame()) {
        mainFrameDomReady = true
        markReadyIfUsable(true, loadedUrl)
      }

      attachToCurrentWindow()
    }

    return { ...state }
  }

  const applyConfig = (config: HermesLocalDashboardConfig): boolean => {
    const url = normalizeHermesLocalDashboardUrl(config.url)
    const token = String(config.token || '').trim()

    if (!TOKEN_RE.test(token)) {
      throw new Error('The protected local dashboard session is unavailable')
    }

    const changed = !currentConfig || currentConfig.origin !== url.origin || currentConfig.token !== token

    if (changed) {
      destroyView()
      retryCount = 0
      currentConfig = { origin: url.origin, token, url: url.toString() }
      publish({
        canRetry: true,
        message: 'Connecting to the configured loopback dashboard.',
        origin: url.origin,
        phase: 'loading',
        retryCount: 0,
        visible: false
      })
    }

    return changed
  }

  return {
    dispose() {
      disposed = true
      desiredVisible = false
      currentConfig = null
      destroyView()
    },
    getState() {
      return { ...state }
    },
    handleWindowClosed(window) {
      if (currentWindow === window) {
        destroyView()
      }
    },
    hide() {
      desiredVisible = false
      clearRetry()
      setViewVisibility(false)
      publish({ visible: false })

      return { ...state }
    },
    isTrustedSender(sender) {
      const window = dependencies.getWindow()

      return Boolean(window && !window.isDestroyed() && sender === window.webContents)
    },
    async reload(config) {
      applyConfig(config)
      desiredVisible = true

      return ensureViewAndLoad(true)
    },
    resize(nextBounds) {
      const window = dependencies.getWindow()

      if (!window || window.isDestroyed()) {
        return { ...state }
      }

      bounds = safeDashboardBounds(nextBounds, window.getContentBounds())
      view?.setBounds(bounds)

      return { ...state }
    },
    async show(config, nextBounds) {
      applyConfig(config)
      desiredVisible = true
      const window = dependencies.getWindow()

      if (!window || window.isDestroyed()) {
        throw new Error('The Desktop window is unavailable')
      }

      bounds = safeDashboardBounds(nextBounds, window.getContentBounds())
      const nextState = await ensureViewAndLoad(false)
      attachToCurrentWindow()

      return nextState
    }
  }
}
