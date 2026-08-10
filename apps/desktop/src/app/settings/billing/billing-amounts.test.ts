import { describe, expect, it } from 'vitest'

import { formatMoney, validateAutoReloadInputs } from './billing-amounts'

describe('billing amount formatting', () => {
  it('uses the product USD format independently of the workstation locale', () => {
    expect(formatMoney(10)).toBe('$10')
    expect(formatMoney('25.50')).toBe('$25.50')
  })

  it('uses the same USD format in validation feedback', () => {
    expect(validateAutoReloadInputs('7.50', '20', { min_usd: '10', max_usd: '1000' })).toEqual({
      error: 'Threshold: minimum is $10.'
    })
  })
})
