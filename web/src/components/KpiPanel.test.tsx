import { describe, it, expect } from 'vitest'
import { render } from '@testing-library/react'
import KpiPanel from './KpiPanel'
import type { MethodStatusCell } from '../api'

const cells: MethodStatusCell[] = [
  { method: 'GET', status_class: '2xx', count: 10 },
  { method: 'GET', status_class: '4xx', count: 2 },
  { method: 'POST', status_class: '5xx', count: 3 },
]

describe('KpiPanel', () => {
  it('renders a method × status-class grid with row and column totals', () => {
    const { container } = render(<KpiPanel byMethodStatus={cells} total={15} />)
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
    const { container } = render(<KpiPanel byMethodStatus={[]} total={0} />)
    expect(container.textContent).toContain('No captures match')
    expect(container.querySelector('table')).toBeNull()
  })
})
