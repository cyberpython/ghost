import { describe, it, expect } from 'vitest'
import { render, screen } from '@testing-library/react'
import StatusBadge from './StatusBadge'
import { statusColor } from '../lib/colors'
describe('StatusBadge', () => {
  it('color map', () => {
    expect(statusColor(200)).toBe('var(--s-2xx)')
    expect(statusColor(404)).toBe('var(--s-4xx)')
    expect(statusColor(null)).toBe('var(--s-null)')
  })
  it('renders 200', () => { render(<StatusBadge code={200} />); expect(screen.getByText('200')).toBeTruthy() })
  it('renders ERR for null', () => { render(<StatusBadge code={null} />); expect(screen.getByText('ERR')).toBeTruthy() })
})
