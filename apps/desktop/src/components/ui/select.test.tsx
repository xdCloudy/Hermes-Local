import { cleanup, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it } from 'vitest'

import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from './select'

afterEach(() => {
  cleanup()
  window.document.documentElement.classList.remove('dark')
})

describe.each(['light', 'dark'] as const)('Select in %s mode', mode => {
  it('renders a portalled themed listbox with accessible option states', () => {
    window.document.documentElement.classList.toggle('dark', mode === 'dark')

    const { container } = render(
      <Select defaultOpen value="auto">
        <SelectTrigger aria-label="Acceleration">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          <SelectItem value="auto">Auto detect</SelectItem>
          <SelectItem value="cuda">NVIDIA CUDA</SelectItem>
          <SelectItem disabled value="cpu">
            CPU only
          </SelectItem>
        </SelectContent>
      </Select>
    )

    const content = window.document.querySelector<HTMLElement>('[data-slot="select-content"]')
    const trigger = container.querySelector<HTMLElement>('[data-slot="select-trigger"]')
    const disabledOption = screen.getByRole('option', { name: 'CPU only' })

    expect(container.querySelector('select')).toBeNull()
    expect(trigger?.getAttribute('role')).toBe('combobox')
    expect(trigger?.getAttribute('aria-label')).toBe('Acceleration')
    expect(trigger?.textContent).toContain('Auto detect')
    expect(screen.getByRole('listbox')).toBe(content)
    expect(window.document.body.contains(content)).toBe(true)
    expect(content?.className).toContain('bg-popover')
    expect(content?.className).toContain('text-popover-foreground')
    expect(content?.className).toContain('border-(--ui-stroke-secondary)')
    expect(disabledOption.getAttribute('aria-disabled')).toBe('true')
  })
})
