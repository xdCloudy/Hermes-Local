import fs from 'node:fs'
import os from 'node:os'
import path from 'node:path'

import { _electron, expect, type Page, test } from '@playwright/test'

const ROOT = process.env.HERMES_LOCAL_ROOT || path.resolve(import.meta.dirname, '../../..')
const LAUNCHER = process.env.HERMES_LOCAL_LAUNCHER_PATH || path.join(ROOT, 'dist', 'Hermes Launcher.exe')
const ENABLED = process.env.HERMES_LOCAL_FUNCTIONAL_ACCEPTANCE === '1'

interface WorkstationState {
  health: {
    dashboard: boolean
    hermes: boolean
    model: boolean
  }
  runtime: {
    controllerAlive: boolean
  }
}

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

interface LocalActionTask {
  id: string
  status: 'failed' | 'running' | 'succeeded'
}

interface LocalActionWindow {
  hermesDesktop: {
    localWorkstation: {
      actionStatus: (taskId: string) => Promise<LocalActionTask>
      startAction: (action: 'restart') => Promise<LocalActionTask>
    }
  }
}

async function workstationState(page: Page): Promise<WorkstationState> {
  return page.evaluate(() =>
    (
      window as unknown as {
        hermesDesktop: {
          localWorkstation: {
            snapshot: () => Promise<WorkstationState>
          }
        }
      }
    ).hermesDesktop.localWorkstation.snapshot()
  )
}

async function expectProjectlessComposer(page: Page): Promise<void> {
  const codingRow = page.locator('[data-slot="composer-surface"] .coding-status-bar')

  await expect(codingRow.getByText('No project', { exact: true })).toBeVisible({ timeout: 60_000 })
  await expect(codingRow.getByText('main', { exact: true })).toHaveCount(0)
  await expect(codingRow.locator('button')).toHaveCount(0)
}

test.describe('Hermes Local packaged functional acceptance', () => {
  test.skip(!ENABLED, 'Set HERMES_LOCAL_FUNCTIONAL_ACCEPTANCE=1 to test the packaged local workstation.')

  test('exercises the packaged workstation routes and state-preserving controls', async () => {
    test.setTimeout(300_000)

    const sandbox = fs.mkdtempSync(path.join(os.tmpdir(), 'hermes-local-functional-'))
    const evidence = path.join(ROOT, 'reports', 'qa', 'screenshots')

    const expectedIntegrationCommit = JSON.parse(fs.readFileSync(path.join(ROOT, 'VERSION.json'), 'utf8')).sources
      .hermesAgent.harnessCommit as string

    const rendererErrors: string[] = []

    const env = Object.fromEntries(
      Object.entries(process.env).filter((entry): entry is [string, string] => typeof entry[1] === 'string')
    )

    fs.mkdirSync(evidence, { recursive: true })

    for (const name of ['ALLUSERSPROFILE', 'ProgramData']) {
      if (env[name]?.includes('%SystemDrive%')) {
        env[name] = env[name].replaceAll('%SystemDrive%', env.SystemDrive || 'C:')
      }
    }

    Object.assign(env, {
      HERMES_DESKTOP_APP_NAME: `Hermes Functional QA ${path.basename(sandbox)}`,
      HERMES_DESKTOP_TEST_HIDDEN: '1',
      HERMES_DESKTOP_USER_DATA_DIR: path.join(sandbox, 'user-data'),
      HERMES_LOCAL_ROOT: ROOT
    })
    delete env.HERMES_DESKTOP_BOOT_FAKE
    delete env.HERMES_DESKTOP_REMOTE_TOKEN
    delete env.HERMES_DESKTOP_REMOTE_URL

    const app = await _electron.launch({
      args: ['--disable-gpu'],
      executablePath: LAUNCHER,
      env
    })

    try {
      const packageIdentity = await app.evaluate(({ app }) => ({
        appVersion: app.getVersion(),
        executable: app.getPath('exe'),
        isPackaged: app.isPackaged,
        resourcesPath: process.resourcesPath
      }))

      const stamp = JSON.parse(
        fs.readFileSync(path.join(packageIdentity.resourcesPath, 'install-stamp.json'), 'utf8')
      ) as {
        commit: string
        dirty: boolean
      }

      expect(packageIdentity.isPackaged).toBe(true)
      expect(packageIdentity.appVersion).toBe('0.18.1')
      expect(path.resolve(packageIdentity.executable)).toBe(path.resolve(LAUNCHER))
      expect(stamp).toMatchObject({
        commit: expectedIntegrationCommit,
        dirty: false
      })

      const page = await app.firstWindow({ timeout: 120_000 })

      page.on('console', message => {
        if (message.type() === 'error') {
          rendererErrors.push(`console: ${message.text()}`)
        }
      })
      page.on('pageerror', error => rendererErrors.push(`pageerror: ${error.message}`))
      await page.setViewportSize({ height: 960, width: 1440 })

      const nav = page.locator('[data-sidebar="menu"]').first()

      await nav.getByRole('button', { exact: true, name: 'Home' }).click({ timeout: 120_000 })
      await expect(page.getByRole('heading', { name: 'Local AI workstation' })).toBeVisible({ timeout: 60_000 })
      await expect(page.getByText('Ready for local inference')).toBeVisible()

      const initialState = await workstationState(page)

      expect(initialState).toMatchObject({
        health: { dashboard: true, hermes: true, model: true },
        runtime: { controllerAlive: true }
      })

      // The workstation's execution directory lives below ROOT, which itself
      // is a Git checkout. It must remain an internal process detail: a fresh
      // chat starts projectless and stays that way through both gateway
      // reconnect and renderer reload.
      await page.getByRole('button', { name: 'Open Chat', exact: true }).click()
      await expectProjectlessComposer(page)

      const restartTask = await page.evaluate(() =>
        (window as unknown as LocalActionWindow).hermesDesktop.localWorkstation.startAction('restart')
      )

      await expect
        .poll(
          () =>
            page.evaluate(
              taskId =>
                (window as unknown as LocalActionWindow).hermesDesktop.localWorkstation.actionStatus(taskId),
              restartTask.id
            ),
          { timeout: 120_000 }
        )
        .toMatchObject({ status: 'succeeded' })
      await expectProjectlessComposer(page)

      await page.reload()
      await expectProjectlessComposer(page)

      await nav.getByRole('button', { exact: true, name: 'Home' }).click({ timeout: 60_000 })
      await page.getByRole('button', { name: 'Refresh workstation' }).click()
      await expect(page.getByRole('button', { name: 'Refresh workstation' })).toBeEnabled({ timeout: 30_000 })
      await page.screenshot({ fullPage: true, path: path.join(evidence, 'packaged-home.png') })

      const originalLoginItem = await page.evaluate(() =>
        (window as unknown as LoginItemWindow).hermesDesktop.localWorkstation.loginItem.get()
      )

      const toggledLoginItem = await page.evaluate(
        enabled => (window as unknown as LoginItemWindow).hermesDesktop.localWorkstation.loginItem.set(enabled),
        !originalLoginItem.enabled
      )

      try {
        expect(toggledLoginItem).toMatchObject({
          available: true,
          enabled: !originalLoginItem.enabled
        })
      } finally {
        const restoredLoginItem = await page.evaluate(
          enabled => (window as unknown as LoginItemWindow).hermesDesktop.localWorkstation.loginItem.set(enabled),
          originalLoginItem.enabled
        )

        expect(restoredLoginItem.enabled).toBe(originalLoginItem.enabled)
      }

      const sections = [
        ['Services', 'Services', 'services'],
        ['Web Dashboard', 'Dashboard', 'dashboard'],
        ['Tasks', 'Tasks', 'tasks'],
        ['Models', 'Models', 'models'],
        ['Profiles', 'Profiles', 'profiles'],
        ['Tools', 'Tools', 'tools'],
        ['Memory', 'Memory', 'memory'],
        ['Sessions', 'Sessions', 'sessions'],
        ['Projects', 'Projects', 'projects'],
        ['Benchmarks', 'Benchmarks', 'benchmarks'],
        ['Logs', 'Logs', 'logs'],
        ['About', 'About', 'about']
      ] as const

      for (const [buttonName, headingName, screenshotName] of sections) {
        await nav.getByRole('button', { exact: true, name: buttonName }).click()
        await expect(page.getByRole('heading', { exact: true, name: headingName })).toBeVisible()
        await page.screenshot({
          fullPage: true,
          path: path.join(evidence, `packaged-${screenshotName}.png`)
        })
      }

      await nav.getByRole('button', { exact: true, name: 'Services' }).click()

      for (const action of ['Start', 'Stop', 'Restart']) {
        await expect(page.getByRole('button', { exact: true, name: action })).toBeVisible()
      }

      await nav.getByRole('button', { exact: true, name: 'Web Dashboard' }).click()
      await expect(page.getByRole('button', { exact: true, name: 'Open externally' })).toBeEnabled()

      await nav.getByRole('button', { exact: true, name: 'Models' }).click()
      await expect(page.getByRole('button', { exact: true, name: 'Register GGUF' })).toBeEnabled()

      await nav.getByRole('button', { exact: true, name: 'Profiles' }).click()
      const nameInput = page.getByLabel('Profile name')
      const originalName = await nameInput.inputValue()

      await nameInput.fill(`${originalName} QA draft`)
      await page.getByRole('button', { exact: true, name: 'Flash Attention' }).click()
      await expect(page.getByRole('button', { exact: true, name: 'Save profile' })).toBeEnabled()
      await nav.getByRole('button', { exact: true, name: 'Home' }).click()

      const unchangedProfile = (await page.getByLabel('Inference profile').textContent())?.trim()

      expect(unchangedProfile).toBe(originalName)

      await nav.getByRole('button', { exact: true, name: 'Logs' }).click()
      const logSelector = page.getByLabel('Log source')

      await logSelector.click()
      await page.getByRole('option', { name: 'Model' }).click()
      await expect(logSelector).toHaveText('Model')
      await page.getByRole('button', { name: 'Refresh logs' }).click()
      await expect(page.locator('pre')).not.toBeEmpty()

      await nav.getByRole('button', { exact: true, name: 'TUI' }).click()
      await expect(page.getByText('Hermes TUI', { exact: true })).toBeVisible()
      await expect(page.getByText(/Connected · PID \d+/)).toBeVisible({ timeout: 60_000 })
      await page.waitForFunction(
        () => (document.querySelector('.xterm-rows')?.textContent?.trim().length || 0) > 10,
        undefined,
        { timeout: 60_000 }
      )
      await page.screenshot({ fullPage: true, path: path.join(evidence, 'packaged-tui.png') })

      await page.setViewportSize({ height: 700, width: 900 })
      await expect(page.getByText('Hermes TUI', { exact: true })).toBeVisible()

      const horizontalOverflow = await page.evaluate(
        () => document.documentElement.scrollWidth > document.documentElement.clientWidth
      )

      expect(horizontalOverflow).toBe(false)
      await page.screenshot({ fullPage: true, path: path.join(evidence, 'packaged-tui-narrow.png') })

      await page.keyboard.press('Tab')
      const focusedTag = await page.evaluate(() => document.activeElement?.tagName || '')

      expect(focusedTag).not.toBe('BODY')
      expect(rendererErrors, rendererErrors.join('\n')).toEqual([])
    } finally {
      await app.close().catch(() => undefined)
      fs.rmSync(sandbox, { force: true, recursive: true })
    }
  })
})
