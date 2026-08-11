import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { atom } from 'nanostores'
import { afterEach, describe, expect, it, vi } from 'vitest'

import { $notifications, clearNotifications } from '@/store/notifications'

vi.mock('@/store/coding-status', () => ({
  registerRepoStatusCwd: () => undefined,
  repoStatusForCwd: (cwd?: string) =>
    atom(
      cwd === '/repo' || cwd === '/Users/someone/www/repo'
        ? {
            added: 12,
            ahead: 0,
            behind: 0,
            branch: 'bb/hitbox',
            defaultBranch: 'main',
            detached: false,
            removed: 3,
            untracked: 0
          }
        : null
    ),
  repoWorktreesForCwd: () => atom([])
}))

vi.mock('@/i18n', () => ({
  useI18n: () => ({
    t: {
      common: { copy: 'Copy', copied: 'Copied', copyFailed: 'Copy failed' },
      rightSidebar: {
        changeCwdTitle: 'Change working directory',
        noProjectTitle: 'No project',
        removeProject: 'Remove project'
      },
      fileMenu: { copied: 'Copied', copyPath: 'Copy Path' },
      sidebar: { projects: { menu: 'Project actions', sectionLabel: 'Projects' } },
      statusStack: { coding: { branchOffFrom: (base: string) => `New branch from ${base}` } }
    }
  })
}))

const { CodingStatusRow } = await import('./coding-row')

describe('CodingStatusRow', () => {
  afterEach(() => {
    cleanup()
  })

  it('opens the review pane from the branch and the diff counts, never the bar itself', () => {
    const onOpen = vi.fn()

    const { container } = render(<CodingStatusRow onOpen={onOpen} repoPath="/repo" />)

    const bar = container.querySelector<HTMLElement>('.coding-status-bar')

    expect(bar).not.toBeNull()

    fireEvent.click(bar!)
    expect(onOpen).not.toHaveBeenCalled()

    fireEvent.click(screen.getByText('bb/hitbox'))
    expect(onOpen).toHaveBeenCalledTimes(1)

    fireEvent.click(screen.getByText('12'))
    expect(onOpen).toHaveBeenCalledTimes(2)
  })

  it('wraps the click targets without adding a layout box', () => {
    const { container } = render(<CodingStatusRow onOpen={() => undefined} repoPath="/repo" />)

    // `display: contents` is what keeps the branch label and the counts direct
    // flex children of the row — the hit areas cost nothing visually.
    expect(screen.getByText('bb/hitbox').parentElement?.classList.contains('contents')).toBe(true)
    expect(screen.getByText('12').closest('button')?.classList.contains('contents')).toBe(true)
    // The glyph button fills the row's existing 3.5 leading slot exactly.
    expect(container.querySelector('button[class~="size-3.5"]')).not.toBeNull()
  })

  it('parks the copy glyph against the end of the path, not the end of the row', () => {
    render(<CodingStatusRow onOpen={() => undefined} repoPath="/Users/someone/www/repo" />)

    const path = screen.getByText('~/www/repo')

    // The path sizes to its content and the glyph is its immediate sibling, so
    // the pair reads as one unit. `flex-1` belongs to the wrapper (which holds
    // the row's slack open) — on the label it stretched the text and pushed the
    // glyph out to the kebab.
    expect(path.classList.contains('flex-1')).toBe(false)
    expect(path.parentElement?.classList.contains('flex-1')).toBe(true)
    expect(path.nextElementSibling?.tagName).toBe('BUTTON')
  })

  it('copies the absolute cwd inline — checkmark feedback, no toast', async () => {
    const writeText = vi.fn().mockResolvedValue(undefined)
    Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText } })
    clearNotifications()

    render(<CodingStatusRow onOpen={() => undefined} repoPath="/Users/someone/www/repo" />)

    // Painted tildified, copied raw.
    expect(screen.getByText('~/www/repo')).toBeTruthy()

    const copy = screen.getByRole('button', { name: 'Copy Path' })

    fireEvent.click(copy)

    await waitFor(() => expect(writeText).toHaveBeenCalledWith('/Users/someone/www/repo'))
    // Confirmation is the button turning into a checkmark, not a notification.
    await waitFor(() => expect(screen.getByRole('button', { name: 'Copied' })).toBeTruthy())
    expect($notifications.get()).toHaveLength(0)
  })

  it('shows No project when the chat has no attached workspace', () => {
    render(<CodingStatusRow repoPath="" />)

    expect(screen.getByText('No project')).not.toBeNull()
    expect(screen.queryByText('main')).toBeNull()
  })

  it('does not mislabel an explicitly attached non-Git folder as projectless', () => {
    render(<CodingStatusRow repoPath="/selected/folder" />)

    expect(screen.queryByText('No project')).toBeNull()
    expect(screen.getByText('folder')).not.toBeNull()
  })

  it('offers change and remove actions for an attached folder', async () => {
    const onChangeProject = vi.fn()
    const onRemoveProject = vi.fn()

    render(
      <CodingStatusRow
        onChangeProject={onChangeProject}
        onRemoveProject={onRemoveProject}
        repoPath="/selected/folder"
      />
    )

    fireEvent.contextMenu(screen.getByText('folder'))
    fireEvent.click(await screen.findByText('Change working directory'))
    expect(onChangeProject).toHaveBeenCalledOnce()

    fireEvent.contextMenu(screen.getByText('folder'))
    fireEvent.click(await screen.findByText('Remove project'))
    expect(onRemoveProject).toHaveBeenCalledOnce()
  })
})
