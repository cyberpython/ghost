import { describe, it, expect } from 'vitest'
import { render, fireEvent } from '@testing-library/react'
import JsonTree from './JsonTree'
import type { Json } from './JsonTree'

describe('JsonTree', () => {
  it('renders a flat object as key/value rows', () => {
    const { container } = render(<JsonTree value={{ a: 1, b: 'x' }} />)
    expect(container.textContent).toContain('"a": 1')
    expect(container.textContent).toContain('"b": "x"')
  })

  it('collapses containers at depth 2 by default and shows a summary', () => {
    const { container } = render(<JsonTree value={{ a: { b: { c: 1 } } }} />)
    // root and `a` (depth < 2) open; `b` (depth 2) collapsed — `c` is hidden.
    expect(container.textContent).not.toContain('"c"')
    expect(container.textContent).toContain('1 key')
  })

  it('expands a collapsed node on click and collapses it again', () => {
    const { getByRole, container } = render(<JsonTree value={{ a: { b: { c: 1 } } }} />)
    fireEvent.click(getByRole('button', { name: /"b"/ }))
    expect(container.textContent).toContain('"c": 1')
    fireEvent.click(getByRole('button', { name: /"b"/ }))
    expect(container.textContent).not.toContain('"c"')
  })

  it('long arrays start collapsed with an item count, root included', () => {
    const arr: Json[] = Array.from({ length: 25 }, (_, i) => `s${i}`)
    const { getByRole, container } = render(<JsonTree value={arr} />)
    expect(container.textContent).toContain('25 items')
    expect(container.textContent).not.toContain('s24')
    fireEvent.click(getByRole('button', { name: /25 items/ }))
    expect(container.textContent).toContain('s24')
  })

  it('renders primitive roots without a toggle', () => {
    const cases: [Json, string][] = [[42, '42'], ['hi', '"hi"'], [null, 'null'], [true, 'true']]
    for (const [v, want] of cases) {
      const { container } = render(<JsonTree value={v} />)
      expect(container.querySelectorAll('.json-tree button')).toHaveLength(0)
      expect(container.textContent).toContain(want)
    }
  })

  it('renders empty containers inline with no extra toggle', () => {
    const { container } = render(<JsonTree value={{ e: {}, f: [] }} />)
    expect(container.querySelectorAll('.json-tree button')).toHaveLength(1)
    expect(container.textContent).toContain('"e": {}')
    expect(container.textContent).toContain('"f": []')
  })

  it('caps very long string values for display and hints at it', () => {
    const { container } = render(<JsonTree value={{ token: 'x'.repeat(400) }} />)
    expect(container.textContent).toContain('…')
    expect(container.textContent).not.toContain('x'.repeat(400))
    expect(container.querySelector('[title*="truncated"]')).not.toBeNull()
  })

  it('quotes and escapes keys like the formatted view', () => {
    const { container } = render(<JsonTree value={{ 'a"b': 1 }} />)
    expect(container.textContent).toContain('"a\\"b": 1')
  })

  it('expand all / collapse all control every node and reset manual toggles', () => {
    const { getByRole, container } = render(<JsonTree value={{ a: { b: { c: { d: 1 } } } }} />)
    // Default: root and `a` open, `b` collapsed — `d` hidden.
    expect(container.textContent).not.toContain('"d"')
    fireEvent.click(getByRole('button', { name: 'Expand all' }))
    expect(container.textContent).toContain('"d": 1')
    fireEvent.click(getByRole('button', { name: 'Collapse all' }))
    expect(container.textContent).not.toContain('"a"')
    expect(container.textContent).toContain('1 key')
    fireEvent.click(getByRole('button', { name: 'Expand all' }))
    expect(container.textContent).toContain('"a"')
  })
})
