import { describe, it, expect, vi } from 'vitest'
import { render, fireEvent, waitFor } from '@testing-library/react'
import { MemoryRouter, Routes, Route, useLocation } from 'react-router'
import Detail from './Detail'
import { getCapture, getAdjacent } from '../api'
import type { CaptureDetail, AdjacentIds } from '../api'

vi.mock('../api', () => ({ getCapture: vi.fn(), getAdjacent: vi.fn() }))

const bd = { text: null, text_truncated: false, binary: false, hex_preview: null, len: 0, sha256: null, truncated: false, decoded_encoding: null }
const d = (): CaptureDetail => ({
  id: 2, created_at: 't', total_ms: 5, client_addr: 'c',
  request_version: 'HTTP/1.1', request_method: 'POST', request_target: '/a',
  request_headers: [], request_body: bd,
  upstream_scheme: 'http', upstream_authority: 'up:9000', upstream_path: '/a', upstream_query: null,
  response_version: null, status_code: null, response_headers: [],
  response_body: bd, error: null,
})

const mounted = (adj: AdjacentIds) => {
  vi.mocked(getCapture).mockResolvedValue(d())
  vi.mocked(getAdjacent).mockResolvedValue(adj)
  const Probe = () => <div data-testid="loc">{useLocation().pathname}</div>
  return render(
    <MemoryRouter initialEntries={['/captures/2']}>
      <Routes>
        <Route path="/captures/:id" element={<Detail auto onAuto={() => {}} />} />
      </Routes>
      <Probe />
    </MemoryRouter>,
  )
}

describe('Detail', () => {
  it('renders Prev/Next in the header with the pager styling, no First/Last', async () => {
    const { getByRole, queryByRole } = mounted({ prev: 1, next: 3 })
    await waitFor(() =>
      expect((getByRole('button', { name: 'Prev' }) as HTMLButtonElement).disabled).toBe(false))
    expect((getByRole('button', { name: 'Next' }) as HTMLButtonElement).disabled).toBe(false)
    expect(queryByRole('button', { name: 'First' })).toBeNull()
    expect(queryByRole('button', { name: 'Last' })).toBeNull()
  })
  it('navigates to the resolved neighbors (SPA-internal)', async () => {
    const { getByRole, getByTestId } = mounted({ prev: 1, next: 3 })
    await waitFor(() =>
      expect((getByRole('button', { name: 'Prev' }) as HTMLButtonElement).disabled).toBe(false))
    fireEvent.click(getByRole('button', { name: 'Next' }))
    await waitFor(() => expect(getByTestId('loc').textContent).toBe('/captures/3'))
    fireEvent.click(getByRole('button', { name: 'Prev' }))
    await waitFor(() => expect(getByTestId('loc').textContent).toBe('/captures/1'))
  })
  it('disables both buttons at the ends of the sequence', async () => {
    const { getByRole } = mounted({ prev: null, next: null })
    await waitFor(() =>
      expect((getByRole('button', { name: 'Prev' }) as HTMLButtonElement).disabled).toBe(true))
    expect((getByRole('button', { name: 'Next' }) as HTMLButtonElement).disabled).toBe(true)
  })
})
