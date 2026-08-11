import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

import { StartupSettings } from './startup-settings'

describe('Startup settings', () => {
  it('reads and changes the existing current-user login item', async () => {
    const get = vi.fn(async () => ({
      available: true,
      enabled: false,
      executable: 'D:\\Hermes-Local\\Hermes Launcher.exe'
    }))
    const set = vi.fn(async (enabled: boolean) => ({
      available: true,
      enabled,
      executable: 'D:\\Hermes-Local\\Hermes Launcher.exe'
    }))

    Object.defineProperty(window, 'hermesDesktop', {
      configurable: true,
      value: {
        localWorkstation: { loginItem: { get, set } }
      }
    })

    render(<StartupSettings />)

    expect(await screen.findByText('D:\\Hermes-Local\\Hermes Launcher.exe')).toBeTruthy()
    expect(get).toHaveBeenCalledTimes(1)

    fireEvent.click(screen.getByRole('button', { name: 'Enable launch at sign-in' }))

    await waitFor(() => expect(set).toHaveBeenCalledWith(true))
    expect(screen.getByRole('button', { name: 'Disable launch at sign-in' })).toBeTruthy()
  })
})
