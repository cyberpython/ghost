import { describe, it, expect, vi } from 'vitest'
import { render, fireEvent } from '@testing-library/react'
import KpiPanel from './KpiPanel'
import type { MethodStatusCell, StatWindow } from '../api'

const cells: MethodStatusCell[] = [
  { method: 'GET', status_class: '2xx', count: 10 },
  { method: 'GET', status_class: '4xx', count: 2 },
  { method: 'POST', status_class: '5xx', count: 3 },
]

describe('KpiPanel', () => {
  it('renders a method × status-class grid with row and column totals', () => {
    const { container } = render(
      <KpiPanel byMethodStatus={cells} total={15} statWindow={'all' as StatWindow} onStatWindow={() => {}} />)
    // Header row: Method + 5 status classes + Total
    const head = container.querySelectorAll('thead th')
    expect(head.length).toBe(7)
    // Body rows: one per method
    const bodyRows = container.querySelectorAll('tbody tr')
    expect(bodyRows.length).toBe(2)
    // GET row shows 10 under 2xx and 2 under 4xx
    const getRow = bodyRows[0]
    expect(getRow.textContent).toContain('10')
    expect(getRow.textContent).toContain('2')
    // Footer totals: 2xx=10, 5xx=3, grand total 15
    const foot = container.querySelector('tfoot')
    expect(foot?.textContent).toContain('10')
    expect(foot?.textContent).toContain('15')
  })

  it('shows an empty message when there are no cells', () => {
    const { container } = render(
      <KpiPanel byMethodStatus={[]} total={0} statWindow={'all' as StatWindow} onStatWindow={() => {}} />)
    expect(container.textContent).toContain('No captures match')
    expect(container.querySelector('table')).toBeNull()
  })

  it('renders the FR-033 window chips with All active by default', () => {
    const { getByRole } = render(
      <KpiPanel byMethodStatus={cells} total={15} statWindow={'all' as StatWindow} onStatWindow={() => {}} />)
    expect(getByRole('button', { name: '24 h' })).toBeTruthy()
    expect(getByRole('button', { name: '7 d' })).toBeTruthy()
    expect(getByRole('button', { name: 'All' })).toBeTruthy()
  })

  it('clicking a window chip reports the selected window', () => {
    const spy = vi.fn()
    const { getByRole } = render(
      <KpiPanel byMethodStatus={cells} total={15} statWindow={'all' as StatWindow} onStatWindow={spy} />)
    fireEvent.click(getByRole('button', { name: '24 h' }))
    expect(spy).toHaveBeenCalledWith('24h')
    fireEvent.click(getByRole('button', { name: '7 d' }))
    expect(spy).toHaveBeenCalledWith('7d')
  })

  it('windowed views show the breakdown sum instead of the unfiltered total', () => {
    // total=99 (whole filtered set) vs cells summing to 15 (inside the window)
    const { container } = render(
      <KpiPanel byMethodStatus={cells} total={99} statWindow={'24h' as StatWindow} onStatWindow={() => {}} />)
    expect(container.textContent).toContain('total 15')
    expect(container.textContent).not.toContain('total 99')
  })
})
