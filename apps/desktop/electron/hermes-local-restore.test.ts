import { describe, expect, it } from 'vitest'

import { hermesLocalControlTest } from './hermes-local-control'

describe('Hermes Local durable restore bridge', () => {
  it('maps determinate progress and safe cancellation capability', () => {
    expect(
      hermesLocalControlTest.taskProgressFromRestoreDocument({
        cancellable: false,
        completedUnits: 4,
        counters: { restoredItems: 4, totalItems: 8 },
        message: 'Restoring user data',
        mode: 'determinate',
        percent: 50,
        totalUnits: 8
      })
    ).toEqual({
      cancellable: false,
      completedUnits: 4,
      counters: { restoredItems: 4, totalItems: 8 },
      message: 'Restoring user data',
      mode: 'determinate',
      percent: 50,
      totalUnits: 8
    })
  })

  it('renders bounded restore progress summaries without private detail', () => {
    expect(
      hermesLocalControlTest.restoreProgressSummary({
        completedUnits: 2,
        message: 'Validated archive',
        stage: 'archive-inspection',
        totalUnits: 2
      })
    ).toContain('Restore progress: archive-inspection 2/2')
  })
})
