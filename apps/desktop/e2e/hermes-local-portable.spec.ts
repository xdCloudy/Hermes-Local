import { execFileSync, spawn, type ChildProcess } from 'node:child_process'
import fs from 'node:fs'
import net from 'node:net'
import os from 'node:os'
import path from 'node:path'

import { chromium, expect, test } from '@playwright/test'

const ROOT = process.env.HERMES_LOCAL_ROOT || path.resolve(import.meta.dirname, '../../..')
const DESKTOP_VERSION = JSON.parse(
  fs.readFileSync(path.join(ROOT, 'source', 'hermes-agent', 'apps', 'desktop', 'package.json'), 'utf8')
).version
const PORTABLE =
  process.env.HERMES_LOCAL_PORTABLE_PATH ||
  path.join(ROOT, 'dist', `Hermes-Launcher-${DESKTOP_VERSION}-windows-x64-portable.exe`)
const ENABLED = process.env.HERMES_LOCAL_ACCEPTANCE === '1'

async function reserveLoopbackPort(): Promise<number> {
  return new Promise((resolve, reject) => {
    const server = net.createServer()

    server.once('error', reject)
    server.listen(0, '127.0.0.1', () => {
      const address = server.address()

      if (!address || typeof address === 'string') {
        server.close()
        reject(new Error('Could not reserve a loopback debug port'))

        return
      }

      server.close(error => (error ? reject(error) : resolve(address.port)))
    })
  })
}

async function waitForDebugger(port: number, child: ChildProcess): Promise<void> {
  const deadline = Date.now() + 120_000

  while (Date.now() < deadline) {
    if (child.exitCode !== null) {
      throw new Error(`Portable launcher exited before its Electron child was ready (exit ${child.exitCode})`)
    }

    try {
      const response = await fetch(`http://127.0.0.1:${port}/json/version`, {
        signal: AbortSignal.timeout(1000)
      })

      if (response.ok) {
        return
      }
    } catch {
      // The self-extractor is still starting.
    }

    await new Promise(resolve => setTimeout(resolve, 500))
  }

  throw new Error('Portable launcher did not expose its loopback-only test debugger within 120 seconds')
}

function stopPortableTree(child: ChildProcess): void {
  if (!child.pid || child.exitCode !== null) {
    return
  }

  try {
    execFileSync('taskkill.exe', ['/PID', String(child.pid), '/T', '/F'], {
      stdio: 'ignore',
      windowsHide: true
    })
  } catch {
    child.kill()
  }
}

test.describe('Hermes Local portable workstation', () => {
  test.skip(!ENABLED, 'Set HERMES_LOCAL_ACCEPTANCE=1 to exercise the portable local stack.')

  test('extracts, launches and exposes the real packaged workstation', async () => {
    test.setTimeout(300_000)

    const sandbox = fs.mkdtempSync(path.join(os.tmpdir(), 'hermes-local-portable-'))
    const userData = path.join(sandbox, 'user-data')
    const evidence = path.join(ROOT, 'reports', 'acceptance')
    const port = await reserveLoopbackPort()
    const rendererErrors: string[] = []

    fs.mkdirSync(userData, { recursive: true })
    fs.mkdirSync(evidence, { recursive: true })

    const env = Object.fromEntries(
      Object.entries(process.env).filter((entry): entry is [string, string] => typeof entry[1] === 'string')
    )

    delete env.HERMES_DESKTOP_BOOT_FAKE
    delete env.HERMES_DESKTOP_REMOTE_TOKEN
    delete env.HERMES_DESKTOP_REMOTE_URL

    Object.assign(env, {
      HERMES_DESKTOP_APP_NAME: `Hermes Launcher Portable Acceptance ${path.basename(sandbox)}`,
      HERMES_DESKTOP_TEST_HIDDEN: '1',
      HERMES_DESKTOP_USER_DATA_DIR: userData,
      HERMES_LOCAL_ROOT: ROOT
    })

    const child = spawn(
      PORTABLE,
      ['--disable-gpu', '--remote-debugging-address=127.0.0.1', `--remote-debugging-port=${port}`],
      {
        env,
        shell: false,
        stdio: 'ignore',
        windowsHide: true
      }
    )

    let browser: Awaited<ReturnType<typeof chromium.connectOverCDP>> | null = null

    try {
      await waitForDebugger(port, child)
      browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`)

      const context = browser.contexts()[0]
      const page = context.pages()[0] || (await context.waitForEvent('page'))

      page.on('console', message => {
        if (message.type() === 'error') {
          rendererErrors.push(`console: ${message.text()}`)
        }
      })
      page.on('pageerror', error => rendererErrors.push(`pageerror: ${error.message}`))

      const workstationNav = page.locator('[data-sidebar="menu"]').first()

      await expect(workstationNav.getByRole('button', { exact: true, name: 'Tools' })).toHaveCount(0)

      await workstationNav.getByRole('button', { name: 'Home', exact: true }).click()
      await expect(page.getByRole('heading', { name: 'Local AI workstation' })).toBeVisible({ timeout: 60_000 })
      await expect(page.getByText('Ready for local inference')).toBeVisible()
      await expect(page.getByRole('main').getByText('Web Dashboard')).toBeVisible()
      await page.screenshot({ fullPage: true, path: path.join(evidence, 'launcher-portable-home.png') })

      await workstationNav.getByRole('button', { name: 'Sessions', exact: true }).click()
      await expect(page.getByRole('heading', { name: 'Sessions', exact: true })).toBeVisible()

      await workstationNav.getByRole('button', { name: 'Projects', exact: true }).click()
      await expect(page.getByRole('heading', { name: 'Projects', exact: true })).toBeVisible()

      await workstationNav.getByRole('button', { name: 'TUI', exact: true }).click()
      await expect(page.getByText(/Connected · PID \d+/)).toBeVisible({ timeout: 60_000 })
      await page.waitForFunction(
        () => (document.querySelector('.xterm-rows')?.textContent?.trim().length || 0) > 10,
        undefined,
        { timeout: 60_000 }
      )
      await page.setViewportSize({ height: 700, width: 900 })
      await page.screenshot({ fullPage: true, path: path.join(evidence, 'launcher-portable-tui-narrow.png') })

      expect(rendererErrors, rendererErrors.join('\n')).toEqual([])
    } finally {
      await browser?.close().catch(() => undefined)
      stopPortableTree(child)
      await new Promise(resolve => setTimeout(resolve, 500))
      fs.rmSync(sandbox, { force: true, maxRetries: 10, recursive: true, retryDelay: 250 })
    }
  })
})
