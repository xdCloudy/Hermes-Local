import { EventEmitter } from 'node:events'

import { app } from 'electron'
import { beforeEach, describe, expect, it, vi } from 'vitest'

vi.mock('electron', () => ({
  app: { isReady: vi.fn(() => true) },
  session: { fromPartition: vi.fn() },
  shell: { openExternal: vi.fn() },
  WebContentsView: vi.fn()
}))

import {
  createHermesLocalDashboardViewController,
  dashboardNavigationAllowed,
  dashboardRequestAllowed,
  normalizeHermesLocalDashboardUrl,
  safeDashboardBounds
} from './hermes-local-dashboard-view'

const TOKEN = 'A'.repeat(48)

class FakeWebContents extends EventEmitter {
  closed = false
  currentUrl = ''
  domReady = true
  finishLoad = true
  loadingMainFrame = false
  pauseLoad = false
  pendingLoads: Array<() => void> = []
  id = Math.floor(Math.random() * 100_000)
  loadURL = vi.fn(async (url: string) => {
    this.loadingMainFrame = true
    this.emit('did-start-navigation', {
      isMainFrame: true,
      isSameDocument: false,
      url
    })

    if (this.pauseLoad) {
      await new Promise<void>(resolve => this.pendingLoads.push(resolve))
    }

    this.currentUrl = url
    this.loadingMainFrame = false
    this.emit('did-navigate', {}, url, 200, 'OK')

    if (this.domReady) {
      this.emit('dom-ready')
    }

    if (this.finishLoad) {
      this.emit('did-finish-load')
    }
  })
  windowOpenHandler: ((details: { url: string }) => { action: string }) | null = null

  close() {
    this.closed = true
  }

  completeNextLoad() {
    const resolve = this.pendingLoads.shift()

    if (!resolve) {
      throw new Error('No dashboard load is pending')
    }

    resolve()
  }

  getURL() {
    return this.currentUrl
  }

  isDestroyed() {
    return this.closed
  }

  isLoadingMainFrame() {
    return this.loadingMainFrame
  }

  setAudioMuted() {}

  setWindowOpenHandler(handler: (details: { url: string }) => { action: string }) {
    this.windowOpenHandler = handler
  }
}

class FakeView {
  background = ''
  bounds = { height: 0, width: 0, x: 0, y: 0 }
  visible = false
  webContents = new FakeWebContents()

  setBackgroundColor(color: string) {
    this.background = color
  }

  setBounds(bounds: { height: number; width: number; x: number; y: number }) {
    this.bounds = bounds
  }

  setVisible(visible: boolean) {
    this.visible = visible
  }
}

function fakeSession() {
  const handlers: Record<string, any> = {}

  return {
    handlers,
    on: vi.fn(),
    setPermissionCheckHandler: vi.fn(),
    setPermissionRequestHandler: vi.fn(),
    webRequest: {
      onBeforeRequest: vi.fn((_filter, handler) => {
        handlers.beforeRequest = handler
      }),
      onBeforeSendHeaders: vi.fn((_filter, handler) => {
        handlers.beforeSendHeaders = handler
      }),
      onCompleted: vi.fn((_filter, handler) => {
        handlers.completed = handler
      })
    }
  }
}

function harness(options: { domReady?: boolean; finishLoad?: boolean; pauseLoad?: boolean } = {}) {
  const views: FakeView[] = []
  const childViews = new Set<FakeView>()
  const renderer = {}

  const window = {
    contentView: {
      addChildView: vi.fn((view: FakeView) => childViews.add(view)),
      removeChildView: vi.fn((view: FakeView) => childViews.delete(view))
    },
    getContentBounds: () => ({ height: 700, width: 1000, x: 0, y: 0 }),
    isDestroyed: () => false,
    webContents: renderer
  }

  const partition = fakeSession()
  const openExternal = vi.fn(async () => undefined)
  const timers: Array<() => void> = []

  const controller = createHermesLocalDashboardViewController({
    clearTimeout: vi.fn(),
    createView: () => {
      const view = new FakeView()
      view.webContents.domReady = options.domReady ?? true
      view.webContents.finishLoad = options.finishLoad ?? true
      view.webContents.pauseLoad = options.pauseLoad ?? false
      views.push(view)

      return view as never
    },
    getSession: () => partition as never,
    getWindow: () => window as never,
    openExternal,
    setTimeout: ((callback: () => void) => {
      timers.push(callback)

      return timers.length as never
    }) as never
  })

  return { childViews, controller, openExternal, partition, renderer, timers, views, window }
}

describe('Hermes Local embedded dashboard policy', () => {
  beforeEach(() => {
    vi.mocked(app.isReady).mockReturnValue(true)
  })

  it('accepts only explicit HTTP loopback origins', () => {
    expect(normalizeHermesLocalDashboardUrl('http://127.0.0.1:9119/chat').origin).toBe('http://127.0.0.1:9119')
    expect(normalizeHermesLocalDashboardUrl('http://127.25.4.9:9119').origin).toBe('http://127.25.4.9:9119')
    expect(normalizeHermesLocalDashboardUrl('http://[::1]:9119').origin).toBe('http://[::1]:9119')
    expect(() => normalizeHermesLocalDashboardUrl('http://192.168.1.20:9119')).toThrow(/loopback/i)
    expect(() => normalizeHermesLocalDashboardUrl('https://127.0.0.1:9119')).toThrow(/loopback/i)
    expect(() => normalizeHermesLocalDashboardUrl('http://localhost:9119')).toThrow(/loopback/i)
  })

  it('allows same-origin routes and resources while rejecting unexpected origins', () => {
    const origin = 'http://127.0.0.1:9119'

    expect(dashboardNavigationAllowed(`${origin}/sessions/1`, origin)).toBe(true)
    expect(dashboardNavigationAllowed('https://example.com/', origin)).toBe(false)
    expect(dashboardRequestAllowed('ws://127.0.0.1:9119/api/ws', origin)).toBe(true)
    expect(dashboardRequestAllowed(`blob:${origin}/123`, origin)).toBe(true)
    expect(dashboardRequestAllowed('https://example.com/script.js', origin)).toBe(false)
  })

  it('clamps renderer-provided bounds to the Desktop content area', () => {
    expect(
      safeDashboardBounds({ height: 900, width: 1400, x: 900, y: 650 }, { height: 700, width: 1000 })
    ).toEqual({ height: 50, width: 100, x: 900, y: 650 })
    expect(() =>
      safeDashboardBounds({ height: Number.NaN, width: 100, x: 0, y: 0 }, { height: 700, width: 1000 })
    ).toThrow(/finite/i)
  })

  it('injects the protected token only for the configured origin', async () => {
    const { controller, partition } = harness()

    await controller.show(
      { token: TOKEN, url: 'http://127.0.0.1:9119' },
      { height: 500, width: 800, x: 20, y: 100 }
    )

    const localResult = await new Promise<any>(resolve =>
      partition.handlers.beforeSendHeaders(
        { requestHeaders: { Accept: 'text/html' }, url: 'http://127.0.0.1:9119/api/status' },
        resolve
      )
    )

    const externalResult = await new Promise<any>(resolve =>
      partition.handlers.beforeSendHeaders(
        { requestHeaders: { Accept: 'text/html' }, url: 'https://example.com/' },
        resolve
      )
    )

    const blocked = await new Promise<any>(resolve =>
      partition.handlers.beforeRequest({ url: 'https://example.com/' }, resolve)
    )

    expect(localResult.requestHeaders.Authorization).toBe(`Bearer ${TOKEN}`)
    expect(externalResult.requestHeaders).not.toHaveProperty('Authorization')
    expect(blocked).toEqual({ cancel: true })
  })

  it('opens external links in the system browser without navigating the embedded view', async () => {
    const { controller, openExternal, views } = harness()

    await controller.show(
      { token: TOKEN, url: 'http://127.0.0.1:9119' },
      { height: 500, width: 800, x: 20, y: 100 }
    )

    const view = views[0]
    expect(view.webContents.windowOpenHandler?.({ url: 'https://example.com/docs' })).toEqual({ action: 'deny' })
    await Promise.resolve()

    expect(openExternal).toHaveBeenCalledWith('https://example.com/docs')
    expect(view.webContents.currentUrl).toBe('http://127.0.0.1:9119/')
  })

  it('preserves a ready view while hidden and reuses it after Desktop navigation', async () => {
    const { controller, views } = harness()
    const config = { token: TOKEN, url: 'http://127.0.0.1:9119' }
    const bounds = { height: 500, width: 800, x: 20, y: 100 }

    await controller.show(config, bounds)
    expect(views).toHaveLength(1)
    expect(views[0].visible).toBe(true)

    controller.hide()
    expect(views[0].visible).toBe(false)

    await controller.show(config, bounds)
    expect(views).toHaveLength(1)
    expect(views[0].visible).toBe(true)
    expect(views[0].webContents.loadURL).toHaveBeenCalledTimes(1)
  })

  it('shows a successful dashboard when DOM readiness arrives without did-finish-load', async () => {
    const { controller, views } = harness({ finishLoad: false })

    await controller.show(
      { token: TOKEN, url: 'http://127.0.0.1:9119' },
      { height: 500, width: 800, x: 20, y: 100 }
    )

    expect(controller.getState().phase).toBe('ready')
    expect(controller.getState().message).toMatch(/connected/i)
    expect(views[0].visible).toBe(true)
    expect(views[0].webContents.loadURL).toHaveBeenCalledTimes(1)
  })

  it('shows a successful dashboard when the completed main-frame request is the only readiness signal', async () => {
    const { controller, partition, views } = harness({ domReady: false, finishLoad: false })

    await controller.show(
      { token: TOKEN, url: 'http://127.0.0.1:9119' },
      { height: 500, width: 800, x: 20, y: 100 }
    )
    partition.handlers.completed({
      resourceType: 'mainFrame',
      statusCode: 200,
      url: 'http://127.0.0.1:9119/'
    })

    expect(controller.getState().phase).toBe('ready')
    expect(controller.getState().message).toMatch(/connected/i)
    expect(views[0].visible).toBe(true)
  })

  it('coalesces concurrent show calls onto one in-flight navigation', async () => {
    const { controller, views } = harness({ pauseLoad: true })
    const config = { token: TOKEN, url: 'http://127.0.0.1:9119' }
    const bounds = { height: 500, width: 800, x: 20, y: 100 }

    const firstShow = controller.show(config, bounds)
    await Promise.resolve()
    const secondShow = controller.show(config, bounds)
    await Promise.resolve()

    expect(views[0].webContents.loadURL).toHaveBeenCalledTimes(1)

    views[0].webContents.completeNextLoad()
    await Promise.all([firstShow, secondShow])

    expect(controller.getState().phase).toBe('ready')
    expect(views[0].visible).toBe(true)
  })

  it('keeps a ready dashboard visible during later non-navigation loading activity', async () => {
    const { controller, views } = harness()

    await controller.show(
      { token: TOKEN, url: 'http://127.0.0.1:9119' },
      { height: 500, width: 800, x: 20, y: 100 }
    )

    views[0].webContents.emit('did-start-loading')
    views[0].webContents.emit('did-stop-loading')

    expect(controller.getState().phase).toBe('ready')
    expect(controller.getState().visible).toBe(true)
    expect(views[0].visible).toBe(true)
  })

  it('self-heals a stale loading state when the existing main document is settled', async () => {
    const { controller, views } = harness()
    const config = { token: TOKEN, url: 'http://127.0.0.1:9119' }
    const bounds = { height: 500, width: 800, x: 20, y: 100 }

    await controller.show(config, bounds)
    views[0].webContents.emit('did-start-navigation', {
      isMainFrame: true,
      isSameDocument: false,
      url: 'http://127.0.0.1:9119/sessions'
    })
    views[0].webContents.loadingMainFrame = false

    expect(controller.getState().phase).toBe('loading')

    const recovered = await controller.show(config, bounds)

    expect(recovered.phase).toBe('ready')
    expect(recovered.visible).toBe(true)
    expect(views[0].visible).toBe(true)
  })

  it('ignores same-document main-frame navigation starts after readiness', async () => {
    const { controller, views } = harness()

    await controller.show(
      { token: TOKEN, url: 'http://127.0.0.1:9119' },
      { height: 500, width: 800, x: 20, y: 100 }
    )

    views[0].webContents.emit('did-start-navigation', {
      isMainFrame: true,
      isSameDocument: true,
      url: 'http://127.0.0.1:9119/sessions'
    })

    expect(controller.getState().phase).toBe('ready')
    expect(controller.getState().visible).toBe(true)
    expect(views[0].visible).toBe(true)
  })

  it('recreates the view after a renderer crash and after a configured connection change', async () => {
    const { controller, timers, views } = harness()
    const bounds = { height: 500, width: 800, x: 20, y: 100 }

    await controller.show({ token: TOKEN, url: 'http://127.0.0.1:9119' }, bounds)
    views[0].webContents.emit('render-process-gone')
    expect(controller.getState().phase).toBe('restarting')
    expect(timers).toHaveLength(1)

    timers.shift()?.()
    await Promise.resolve()
    await Promise.resolve()
    expect(views).toHaveLength(2)

    await controller.show({ token: 'B'.repeat(48), url: 'http://127.0.0.1:9120' }, bounds)
    expect(views).toHaveLength(3)
    expect(views[1].webContents.closed).toBe(true)
    expect(controller.getState().origin).toBe('http://127.0.0.1:9120')
  })

  it('rejects IPC calls from anything except the primary Desktop renderer', () => {
    const { controller, renderer } = harness()

    expect(controller.isTrustedSender(renderer as never)).toBe(true)
    expect(controller.isTrustedSender({} as never)).toBe(false)
  })
})
