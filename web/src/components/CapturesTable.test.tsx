import { describe, it, expect } from 'vitest'
import { render } from '@testing-library/react'
import { MemoryRouter } from 'react-router'
import CapturesTable from './CapturesTable'
import type { CaptureListItem } from '../api'
const rows: CaptureListItem[] = [
  { id: 1, created_at: 't', total_ms: 5, client_addr: 'c', request_method: 'GET', request_target: '/a', upstream_path: '/a',
    upstream_query: null, status_code: 200, error: null, request_body_len: 150, request_body_truncated: false, response_body_len: 0, response_body_truncated: false },
  { id: 2, created_at: 't', total_ms: 6, client_addr: 'c2', request_method: 'POST', request_target: '/b', upstream_path: '/b',
    upstream_query: null, status_code: 500, error: 'boom', request_body_len: 2048, request_body_truncated: true, response_body_len: 0, response_body_truncated: false },
]
const renderTable = () => render(
  <MemoryRouter><CapturesTable captures={rows}/></MemoryRouter>
)
describe('CapturesTable', () => {
  it('renders rows with badges', () => {
    const { container } = renderTable()
    expect(container.querySelectorAll('tbody tr').length).toBe(2)
    expect(container.querySelector('.badge.method')?.textContent).toBe('GET')
    expect(container.querySelector('tbody tr')?.className).not.toContain('bg-[color-mix(in_srgb,var(--s-5xx)_18%,transparent)]')
    expect(container.querySelectorAll('tbody tr')[1]?.className).toContain('bg-[color-mix(in_srgb,var(--s-5xx)_18%,transparent)]')
  })
  it('links rows to the detail page', () => {
    const { container } = renderTable()
    const a = container.querySelector('tbody a')
    expect(a?.getAttribute('href')).toBe('/captures/1')
  })
  it('renders the request body size column', () => {
    const { container } = renderTable()
    expect(container.querySelector('thead')?.textContent).toContain('Body')
    expect(container.textContent).toContain('150 B')
    expect(container.textContent).toContain('2.0 KB *')
  })
  it('title-hints truncated bodies in the size column', () => {
    const { container } = renderTable()
    expect(container.querySelectorAll('td[title*="truncated"]')).toHaveLength(1)
  })
})
