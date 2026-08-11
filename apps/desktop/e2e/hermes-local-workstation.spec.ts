import fs from 'node:fs'
import os from 'node:os'
import path from 'node:path'

import { _electron, expect, type Page, test } from '@playwright/test'

const ROOT = process.env.HERMES_LOCAL_ROOT || path.resolve(import.meta.dirname, '../../..')
const LAUNCHER = process.env.HERMES_LOCAL_LAUNCHER_PATH || path.join(ROOT, 'dist', 'Hermes Launcher.exe')
const ENABLED = process.env.HERMES_LOCAL_ACCEPTANCE === '1'

interface LoginItemStatus {
  available: boolean
  enabled: boolean
}

interface LoginItemWindow {
  hermesDesktop: {
    localWorkstation: {
      loginItem: {
        get: () => Promise<LoginItemStatus>
        set: (enabled: boolean) => Promise<LoginItemStatus>
      }
    }
  }
}

async function assertRendererCsp(page: Page) {
  const policy = await page.locator('meta[http-equiv="Content-Security-Policy"]').getAttribute('content')

  expect(policy).toContain("script-src 'self'")
  expect(policy).toContain("object-src 'none'")
  expect(policy).toContain("base-uri 'self'")
  expect(policy?.match(/script-src[^;]*/)?.[0]).not.toContain("'unsafe-inline'")
  expect(policy?.match(/script-src[^;]*/)?.[0]).not.toContain("'unsafe-eval'")

  const inlineScriptBlocked = await page.evaluate(
    () =>
      new Promise<boolean>(resolve => {
        const marker = `__hermesCspProbe${Date.now()}`
        const script = document.createElement('script')
        const target = window as unknown as Record<string, unknown>

        script.textContent = `window.${marker} = true`
        document.head.append(script)
        setTimeout(() => {
          const executed = Boolean(target[marker])

          delete target[marker]
          script.remove()
          resolve(!executed)
        }, 0)
      })
  )

  expect(inlineScriptBlocked).toBe(true)
}

test.describe('Hermes Local packaged workstation', () => {
  test.skip(!ENABLED, 'Set HERMES_LOCAL_ACCEPTANCE=1 to exercise the installed local stack.')

  test('auto-connects and exposes the workstation and TUI surfaces', async () => {
    test.setTimeout(240_000)

    const sandbox = fs.mkdtempSync(path.join(os.tmpdir(), 'hermes-local-acceptance-'))
    const userData = path.join(sandbox, 'user-data')
    const evidence = path.join(ROOT, 'reports', 'acceptance')
    const defaults = JSON.parse(fs.readFileSync(path.join(ROOT, 'config', 'defaults', 'workstation.json'), 'utf8'))
    const settingsPath = path.join(ROOT, 'config', 'launcher', 'user-settings.json')
    const userSettings = fs.existsSync(settingsPath) ? JSON.parse(fs.readFileSync(settingsPath, 'utf8')) : {}
    const expectedNetwork = { ...defaults.network, ...(userSettings.network || {}) }
    const mainLogs: string[] = []
    const rendererErrors: string[] = []

    fs.mkdirSync(userData, { recursive: true })
    fs.mkdirSync(evidence, { recursive: true })

    const env = Object.fromEntries(
      Object.entries(process.env).filter((entry): entry is [string, string] => typeof entry[1] === 'string')
    )

    delete env.HERMES_DESKTOP_BOOT_FAKE
    delete env.HERMES_DESKTOP_REMOTE_TOKEN
    delete env.HERMES_DESKTOP_REMOTE_URL
    for (const name of ['ALLUSERSPROFILE', 'ProgramData']) {
      if (env[name]?.includes('%SystemDrive%')) {
        env[name] = env[name].replaceAll('%SystemDrive%', env.SystemDrive || 'C:')
      }
    }

    Object.assign(env, {
      HERMES_DESKTOP_APP_NAME: `Hermes Launcher Acceptance ${path.basename(sandbox)}`,
      HERMES_DESKTOP_TEST_HIDDEN: '1',
      HERMES_DESKTOP_USER_DATA_DIR: userData,
      HERMES_LOCAL_ROOT: ROOT
    })

    const app = await _electron.launch({
      args: ['--disable-gpu'],
      executablePath: LAUNCHER,
      env
    })

    app.on('console', message => mainLogs.push(`${message.type()}: ${message.text()}`))

    try {
      const launchState = await app.evaluate(({ app, BrowserWindow }) => ({
        environment: {
          appName: process.env.HERMES_DESKTOP_APP_NAME,
          hidden: process.env.HERMES_DESKTOP_TEST_HIDDEN,
          localRoot: process.env.HERMES_LOCAL_ROOT,
          userData: process.env.HERMES_DESKTOP_USER_DATA_DIR
        },
        isPackaged: app.isPackaged,
        isReady: app.isReady(),
        paths: {
          appData: app.getPath('appData'),
          userData: app.getPath('userData')
        },
        windows: BrowserWindow.getAllWindows().map(window => ({
          destroyed: window.isDestroyed(),
          title: window.getTitle(),
          visible: window.isVisible()
        }))
      }))

      const launchStateEvidence = JSON.stringify(
        launchState,
        (_key, value) =>
          typeof value === 'string'
            ? value
                .replaceAll(sandbox, '<sandbox>')
                .replaceAll(path.basename(sandbox), '<sandbox-id>')
                .replaceAll(ROOT, '<project-root>')
                .replaceAll(os.homedir(), '<user-home>')
            : value,
        2
      )

      fs.writeFileSync(path.join(evidence, 'launcher-launch-state.json'), launchStateEvidence, 'utf8')

      const page = await app.firstWindow({ timeout: 120_000 })
      const workstationState = await page.evaluate(() =>
        (
          window as unknown as {
            hermesDesktop: {
              localWorkstation: {
                snapshot: () => Promise<{ settings: { network: unknown } }>
              }
            }
          }
        ).hermesDesktop.localWorkstation.snapshot()
      )

      expect(workstationState.settings.network).toEqual({
        ...expectedNetwork,
        host: expectedNetwork.host === 'localhost' ? '127.0.0.1' : expectedNetwork.host
      })

      // Perform the intentional blocked-inline-script probe before collecting
      // unexpected console errors; Chromium logs the expected CSP violation.
      await assertRendererCsp(page)

      const originalLoginItem = await page.evaluate(() =>
        (window as unknown as LoginItemWindow).hermesDesktop.localWorkstation.loginItem.get()
      )

      const toggledLoginItem = await page.evaluate(
        enabled => (window as unknown as LoginItemWindow).hermesDesktop.localWorkstation.loginItem.set(enabled),
        !originalLoginItem.enabled
      )

      try {
        expect(toggledLoginItem.available).toBe(true)
        expect(toggledLoginItem.enabled).toBe(!originalLoginItem.enabled)
      } finally {
        const restoredLoginItem = await page.evaluate(
          enabled => (window as unknown as LoginItemWindow).hermesDesktop.localWorkstation.loginItem.set(enabled),
          originalLoginItem.enabled
        )

        expect(restoredLoginItem.enabled).toBe(originalLoginItem.enabled)
      }

      page.on('console', message => {
        if (message.type() === 'error') {
          rendererErrors.push(`console: ${message.text()}`)
        }
      })
      page.on('pageerror', error => rendererErrors.push(`pageerror: ${error.message}`))

      const workstationNav = page.locator('[data-sidebar="menu"]').first()

      // A cold Hermes Local launch starts the workstation stack before the
      // gateway can open. Keep the proof on the real user path and wait for
      // the initial boot/onboarding overlay to release interaction.
      await workstationNav.getByRole('button', { name: 'Home', exact: true }).click({ timeout: 120_000 })
      await page.getByRole('heading', { name: 'Local AI workstation' }).waitFor({ timeout: 60_000 })
      await expect(page.getByText('Ready for local inference')).toBeVisible()
      await expect(page.getByText(/and Hermes are online/)).toBeVisible()
      await expect(page.getByText(/server$/).first()).toBeVisible()
      await expect(page.getByText('Hermes serve')).toBeVisible()
      await expect(page.getByRole('main').getByText('Web Dashboard')).toBeVisible()
      await page.screenshot({ fullPage: true, path: path.join(evidence, 'launcher-home.png') })

      await workstationNav.getByRole('button', { name: 'Services', exact: true }).click()
      await expect(page.getByRole('heading', { name: 'Services' })).toBeVisible()
      await expect(page.getByText('PID', { exact: false }).first()).toBeVisible()

      await workstationNav.getByRole('button', { name: 'Web Dashboard', exact: true }).click()
      await expect(page.getByRole('heading', { name: 'Dashboard', exact: true })).toBeVisible()
      await expect(page.getByRole('button', { name: 'Open externally', exact: true })).toBeEnabled()

      await workstationNav.getByRole('button', { name: 'Sessions', exact: true }).click()
      await expect(page.getByRole('heading', { name: 'Sessions', exact: true })).toBeVisible()
      await expect(page.getByRole('button', { name: 'Open session workspace', exact: true })).toBeEnabled()

      await workstationNav.getByRole('button', { name: 'Projects', exact: true }).click()
      await expect(page.getByRole('heading', { name: 'Projects', exact: true })).toBeVisible()
      await expect(page.getByRole('button', { name: 'Open project workspace', exact: true })).toBeEnabled()

      await expect(workstationNav.getByRole('button', { name: 'Skills', exact: true })).toBeVisible()

      await workstationNav.getByRole('button', { name: 'TUI', exact: true }).click()
      await expect(page.getByText('Hermes TUI', { exact: true })).toBeVisible()
      await expect(page.getByText(/Connected · PID \d+/)).toBeVisible({ timeout: 60_000 })
      await page.waitForFunction(
        () => (document.querySelector('.xterm-rows')?.textContent?.trim().length || 0) > 10,
        undefined,
        { timeout: 60_000 }
      )
      await page.screenshot({ fullPage: true, path: path.join(evidence, 'launcher-tui.png') })

      await page.setViewportSize({ height: 700, width: 900 })
      await expect(page.getByText('Hermes TUI', { exact: true })).toBeVisible()
      await page.screenshot({ fullPage: true, path: path.join(evidence, 'launcher-tui-narrow.png') })

      expect(rendererErrors, rendererErrors.join('\n')).toEqual([])
    } finally {
      fs.writeFileSync(path.join(evidence, 'launcher-main.log'), `${mainLogs.join('\n')}\n`, 'utf8')
      await app.close().catch(() => undefined)
      fs.rmSync(sandbox, { force: true, recursive: true })
    }
  })
})
