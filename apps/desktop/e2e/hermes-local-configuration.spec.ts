import fs from 'node:fs'
import os from 'node:os'
import path from 'node:path'

import { _electron, expect, type Locator, test } from '@playwright/test'

const ROOT = process.env.HERMES_LOCAL_ROOT || path.resolve(import.meta.dirname, '../../..')
const LAUNCHER = process.env.HERMES_LOCAL_LAUNCHER_PATH || path.join(ROOT, 'dist', 'Hermes Launcher.exe')
const ENABLED = process.env.HERMES_LOCAL_ACCEPTANCE === '1'

test.describe('Hermes Local portable configuration UI', () => {
  test.skip(!ENABLED, 'Set HERMES_LOCAL_ACCEPTANCE=1 to exercise the packaged configuration UI.')

  test('persists model, runtime, and profile choices and restores the installed settings', async () => {
    test.setTimeout(180_000)

    const sandbox = fs.mkdtempSync(path.join(os.tmpdir(), 'hermes-local-configuration-'))
    const userData = path.join(sandbox, 'user-data')
    const evidence = path.join(ROOT, 'reports', 'acceptance')
    const settingsPath = path.join(ROOT, 'config', 'launcher', 'user-settings.json')
    const originalSettings = fs.existsSync(settingsPath) ? fs.readFileSync(settingsPath) : null
    const thirdPath = path.join(sandbox, 'third-anywhere.gguf')
    const rendererErrors: string[] = []

    const env = Object.fromEntries(
      Object.entries(process.env).filter((entry): entry is [string, string] => typeof entry[1] === 'string')
    )

    fs.mkdirSync(userData, { recursive: true })
    fs.mkdirSync(evidence, { recursive: true })
    fs.writeFileSync(thirdPath, 'portable arbitrary GGUF fixture')
    delete env.HERMES_DESKTOP_BOOT_FAKE
    delete env.HERMES_DESKTOP_REMOTE_TOKEN
    delete env.HERMES_DESKTOP_REMOTE_URL

    for (const name of ['ALLUSERSPROFILE', 'ProgramData']) {
      if (env[name]?.includes('%SystemDrive%')) {
        env[name] = env[name].replaceAll('%SystemDrive%', env.SystemDrive || 'C:')
      }
    }

    Object.assign(env, {
      HERMES_DESKTOP_APP_NAME: `Hermes Launcher Configuration ${path.basename(sandbox)}`,
      HERMES_DESKTOP_TEST_HIDDEN: '1',
      HERMES_DESKTOP_USER_DATA_DIR: userData,
      HERMES_LOCAL_ROOT: ROOT
    })

    const app = await _electron.launch({
      args: ['--disable-gpu'],
      executablePath: LAUNCHER,
      env
    })

    try {
      const page = await app.firstWindow({ timeout: 120_000 })

      await app.evaluate(({ BrowserWindow }) => BrowserWindow.getAllWindows()[0]?.show())
      page.on('console', message => {
        if (message.type() === 'error') {
          rendererErrors.push(`console: ${message.text()}`)
        }
      })
      page.on('pageerror', error => rendererErrors.push(`pageerror: ${error.message}`))

      const workstationNav = page.locator('[data-sidebar="menu"]').first()
      const activate = (locator: Locator) => locator.evaluate(element => (element as HTMLElement).click())

      const openSection = async (name: string) => {
        // Electron's persisted zoom can offset Playwright's pointer coordinates
        // from the CSS pixels it reports. Navigation is setup for this test,
        // not the behavior under test, so dispatch through the resolved button.
        await workstationNav
          .getByRole('button', { exact: true, name })
          .evaluate(button => (button as HTMLElement).click())
      }

      await openSection('Home')
      await expect(page.getByRole('heading', { name: 'Local AI workstation' })).toBeVisible({ timeout: 60_000 })
      await openSection('Models')
      await expect(page.getByRole('heading', { exact: true, name: 'Models' })).toBeVisible()

      await page.evaluate(async localPath => {
        const desktop = (
          window as unknown as {
            hermesDesktop: {
              localWorkstation: {
                registerModel: (model: { localPath: string }) => Promise<{ id: string }>
                selectModel: (id: string) => Promise<unknown>
              }
            }
          }
        ).hermesDesktop

        const registered = await desktop.localWorkstation.registerModel({ localPath })

        await desktop.localWorkstation.selectModel(registered.id)
      }, thirdPath)
      await activate(page.getByRole('button', { exact: true, name: 'Refresh workstation' }))
      await expect(page.getByRole('heading', { exact: true, name: 'third-anywhere' })).toBeVisible()
      await expect(page.getByText('Selected', { exact: true })).toBeVisible()
      await page.screenshot({ fullPage: true, path: path.join(evidence, 'launcher-configuration-models.png') })
      await activate(page.getByRole('button', { exact: true, name: 'Remove third-anywhere' }))
      await expect(page.getByRole('heading', { exact: true, name: 'third-anywhere' })).toHaveCount(0)

      const acceleration = page.getByLabel('Acceleration')

      await acceleration.press('Enter')
      await expect(page.getByRole('option', { name: 'CPU only' })).toBeVisible()
      await page.screenshot({ fullPage: true, path: path.join(evidence, 'launcher-dropdown-light.png') })
      await page.keyboard.press('Escape')
      await page.evaluate(() => window.localStorage.setItem('hermes-desktop-mode-v1', 'dark'))
      await page.reload()
      await expect(page.getByRole('heading', { exact: true, name: 'Models' })).toBeVisible({ timeout: 60_000 })
      await acceleration.press('Enter')
      await expect(page.getByRole('option', { name: 'CPU only' })).toBeVisible()
      await page.screenshot({ fullPage: true, path: path.join(evidence, 'launcher-dropdown-dark.png') })
      await page.getByRole('option', { name: 'CPU only' }).press('Enter')
      await expect(acceleration).toHaveText('CPU only')
      await page.getByLabel('Model API port').fill('18211')
      await page.getByLabel('Hermes/dashboard port').fill('19211')
      await page.getByLabel('Build workers').fill('6')
      await page.getByLabel('CUDA architecture').fill('89')
      await page.getByLabel('Python version').fill('3.12')
      await activate(page.getByRole('button', { exact: true, name: 'Verify model on start' }))
      await activate(page.getByRole('button', { exact: true, name: 'Save settings' }))
      await expect
        .poll(() => {
          const settings = JSON.parse(fs.readFileSync(settingsPath, 'utf8'))

          return settings.runtime
        })
        .toEqual({
          acceleration: 'cpu',
          buildParallelism: 6,
          cudaArchitecture: '89',
          pythonVersion: '3.12',
          verifyModelOnStart: false
        })
      await page.screenshot({ fullPage: true, path: path.join(evidence, 'launcher-configuration-runtime.png') })

      await openSection('Profiles')
      await expect(page.getByRole('heading', { exact: true, name: 'Profiles' })).toBeVisible()

      const modelSpeculativeDecoding = page.getByRole('button', {
        exact: true,
        name: 'Speculative decoding · model'
      })

      await expect(modelSpeculativeDecoding).toHaveAttribute('aria-pressed', 'true')
      await expect(modelSpeculativeDecoding).toBeDisabled()
      const settingsBeforeProfiles = fs.existsSync(settingsPath)
        ? JSON.parse(fs.readFileSync(settingsPath, 'utf8'))
        : {}

      const profileSource = Array.isArray(settingsBeforeProfiles.profiles)
        ? settingsBeforeProfiles.profiles
        : JSON.parse(fs.readFileSync(path.join(ROOT, 'config', 'profiles', 'profiles.json'), 'utf8')).profiles

      const profilesBefore = profileSource.map((profile: { name: string }) => profile.name)

      await activate(page.getByRole('main').getByText('New profile', { exact: true }))
      await expect
        .poll(() => {
          const settings = JSON.parse(fs.readFileSync(settingsPath, 'utf8'))

          return settings.profiles.some((profile: { name: string }) => !profilesBefore.includes(profile.name))
        })
        .toBe(true)
      const settingsAfterCreate = JSON.parse(fs.readFileSync(settingsPath, 'utf8'))

      const customProfileName = settingsAfterCreate.profiles.find(
        (profile: { name: string }) => !profilesBefore.includes(profile.name)
      ).name

      await activate(page.getByRole('button', { exact: true, name: customProfileName }))
      await expect(page.getByLabel('Profile name')).toHaveValue(customProfileName)
      await page.getByLabel('Context tokens').fill('65536')
      await expect(page.getByLabel('Context tokens')).toHaveValue('65536')
      await page.getByLabel('GPU layers').fill('32')
      await expect(page.getByLabel('GPU layers')).toHaveValue('32')
      const kvCacheValues = page.getByLabel('KV cache values')

      await kvCacheValues.press('Enter')
      await page.getByRole('option', { name: 'q4_0' }).press('Enter')
      await expect(kvCacheValues).toHaveText('q4_0')
      const saveProfileButton = page.getByRole('button', { exact: true, name: 'Save profile' })

      await activate(saveProfileButton)
      await expect(saveProfileButton).toBeEnabled()
      await expect
        .poll(() => {
          const settings = JSON.parse(fs.readFileSync(settingsPath, 'utf8'))
          const profile = settings.profiles.find((entry: { name: string }) => entry.name === customProfileName)

          return {
            contextTokens: profile?.contextTokens,
            gpuLayers: profile?.gpu?.layers,
            kvValueType: profile?.kvCache?.valueType,
            selectedProfile: settings.selectedProfile
          }
        })
        .toEqual({
          contextTokens: 65_536,
          gpuLayers: 32,
          kvValueType: 'q4_0',
          selectedProfile: customProfileName
        })
      await page.screenshot({ fullPage: true, path: path.join(evidence, 'launcher-configuration-profiles.png') })
      await activate(page.getByRole('button', { exact: true, name: 'Delete' }))

      expect(rendererErrors, rendererErrors.join('\n')).toEqual([])
    } finally {
      await app.close().catch(() => undefined)

      if (originalSettings) {
        fs.mkdirSync(path.dirname(settingsPath), { recursive: true })
        fs.writeFileSync(settingsPath, originalSettings)
      } else {
        fs.rmSync(settingsPath, { force: true })
      }

      fs.rmSync(sandbox, { force: true, maxRetries: 10, recursive: true, retryDelay: 250 })
    }
  })
})
