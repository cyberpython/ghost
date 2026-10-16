import { describe, it, expect } from 'vitest'
import { render, screen } from '@testing-library/react'
import MethodBadge from './MethodBadge'
import { methodColor } from '../lib/colors'
describe('MethodBadge', () => {
  it('color map', () => {
    expect(methodColor('GET')).toBe('#22c55e')
    expect(methodColor('POST')).toBe('#38bdf8')
    expect(methodColor('PATCH')).toBe('#8b93a7')
  })
  it('renders label', () => { render(<MethodBadge m="GET" />); expect(screen.getByText('GET')).toBeTruthy() })
})
