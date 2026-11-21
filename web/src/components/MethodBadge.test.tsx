import { describe, it, expect } from 'vitest'
import { render, screen } from '@testing-library/react'
import MethodBadge from './MethodBadge'
import { methodColor } from '../lib/colors'
describe('MethodBadge', () => {
  it('color map', () => {
    expect(methodColor('GET')).toBe('var(--m-get)')
    expect(methodColor('POST')).toBe('var(--m-post)')
    expect(methodColor('PATCH')).toBe('var(--m-other)')
  })
  it('renders label', () => { render(<MethodBadge m="GET" />); expect(screen.getByText('GET')).toBeTruthy() })
})
