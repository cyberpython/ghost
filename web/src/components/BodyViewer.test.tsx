import { describe, it, expect, vi } from 'vitest'
import { render, fireEvent, waitFor, within } from '@testing-library/react'
import BodyViewer from './BodyViewer'
import { copyToClipboard } from '../lib/clipboard'
import { getCapture } from '../api'
import type { BodyView, CaptureDetail } from '../api'

vi.mock('../lib/clipboard', () => ({ copyToClipboard: vi.fn() }))
vi.mock('../api', () => ({ getCapture: vi.fn() }))

const bd = (over: Partial<BodyView> = {}): BodyView => ({
  text: null, text_truncated: false, binary: false, hex_preview: null,
  len: 0, sha256: null, truncated: false, decoded_encoding: null, ...over,
})
const detail: CaptureDetail = {
  id: 5, created_at: 't', total_ms: 5, client_addr: 'c',
  request_version: 'HTTP/1.1', request_method: 'POST', request_target: '/api?a=1',
  request_headers: [
    { name: 'host', value: '127.0.0.1:8080' },
    { name: 'content-length', value: '9' },
    { name: 'content-type', value: 'application/json' },
  ],
  request_body: bd({ text: '{"pw":"***"}', len: 9 }),
  upstream_scheme: 'http', upstream_authority: 'up:9000', upstream_path: '/app/api',
  upstream_query: 'a=1', response_version: null, status_code: null,
  response_headers: [], response_body: bd(), error: null,
}
const jb = { text: '{"a":1}', binary: false, hex_preview: null, len: 5, sha256: null, truncated: false, text_truncated: false, decoded_encoding: null }
const xb = { text: '<?xml version="1.0"?><root>hi</root>', binary: false, hex_preview: null, len: 0, sha256: null, truncated: false, text_truncated: false, decoded_encoding: null }
const bin = { text: null, binary: true, hex_preview: 'ff fe 00 41 42', len: 2048, sha256: null, truncated: true, text_truncated: false, decoded_encoding: null }
const binSmall = { text: null, binary: true, hex_preview: 'ff fe 00 41 42', len: 5, sha256: null, truncated: false, text_truncated: false, decoded_encoding: null }
const badJson = { text: '{"a":1', binary: false, hex_preview: null, len: 6, sha256: null, truncated: true, text_truncated: true, decoded_encoding: null }
describe('BodyViewer', () => {
  it('renders the JSON tree by default (Tree is the first tab)', () => {
    const { container } = render(<BodyViewer view={jb} id={1} part="response" />)
    expect(container.querySelector('.json-tree')).not.toBeNull()
    expect(container.textContent).toContain('"a": 1')
    expect(container.querySelectorAll('.token')).toHaveLength(0)
  })
  it('shows the highlighted raw JSON on the Raw tab', () => {
    const { getByRole, container } = render(<BodyViewer view={jb} id={1} part="response" />)
    fireEvent.click(getByRole('button', { name: 'Raw' }))
    expect(container.querySelectorAll('.token').length).toBeGreaterThan(0)
    expect(container.querySelector('.json-tree')).toBeNull()
  })
  it('highlights XML', () => { const { container } = render(<BodyViewer view={xb} id={1} part="response" />); expect(container.querySelectorAll('.token').length).toBeGreaterThan(0) })
  it('renders hex lines + download link for big binary', () => {
    const { container } = render(<BodyViewer view={bin} id={7} part="request" />)
    expect(container.querySelector('pre.hexdump')?.textContent).toContain('00000000')
    expect(container.querySelector('a[href="/api/captures/7/body?part=request"]')).not.toBeNull()
  })
  it('no download link when the body fits the preview', () => {
    const { container } = render(<BodyViewer view={binSmall} id={7} part="request" />)
    expect(container.querySelector('a[href*="/body?part="]')).toBeNull()
  })
  it('survives invalid JSON under application/json', () => {
    const { container } = render(<BodyViewer view={badJson} id={1} part="request" contentType="application/json" />)
    expect(container.textContent).toContain('not valid JSON')
  })
  it('hides the Tree tab when the JSON does not parse', () => {
    const { queryByRole, container } = render(
      <BodyViewer view={badJson} id={1} part="request" contentType="application/json" />)
    expect(queryByRole('button', { name: 'Tree' })).toBeNull()
    expect(container.querySelector('.json-tree')).toBeNull()
  })
  it('shows the full sha256, not an elided prefix', () => {
    const h = 'ab'.repeat(32)
    const { container } = render(<BodyViewer view={{ ...jb, sha256: h }} id={1} part="request" />)
    expect(container.textContent).toContain(`sha256 ${h}`)
    expect(container.textContent).not.toContain('…')
  })

  it('copies the raw body text to the clipboard', async () => {
    vi.mocked(copyToClipboard).mockResolvedValue(undefined)
    const { getByRole, container } = render(<BodyViewer view={jb} id={1} part="response" />)
    await fireEvent.click(getByRole('button', { name: 'Copy Raw' }))
    await waitFor(() => expect(copyToClipboard).toHaveBeenCalledWith('{"a":1}'))
    await waitFor(() => expect(container.textContent).toContain('Copied!'))
  })

  it('copies the pretty JSON via Copy Formatted', async () => {
    vi.mocked(copyToClipboard).mockResolvedValue(undefined)
    const { getByRole } = render(<BodyViewer view={jb} id={1} part="response" />)
    await fireEvent.click(getByRole('button', { name: 'Copy Formatted' }))
    await waitFor(() => expect(copyToClipboard).toHaveBeenCalledWith('{\n  "a": 1\n}'))
  })

  it('shows Copy Formatted only for parseable JSON', () => {
    const j = render(<BodyViewer view={jb} id={1} part="response" />)
    expect(within(j.container).getByRole('button', { name: 'Copy Formatted' })).toBeTruthy()
    const bad = render(<BodyViewer view={badJson} id={1} part="request" contentType="application/json" />)
    expect(within(bad.container).queryByRole('button', { name: 'Copy Formatted' })).toBeNull()
    const x = render(<BodyViewer view={xb} id={1} part="response" />)
    expect(within(x.container).queryByRole('button', { name: 'Copy Formatted' })).toBeNull()
  })

  it('copies the hexdump representation for binary bodies', async () => {
    vi.mocked(copyToClipboard).mockResolvedValue(undefined)
    const { getByRole } = render(<BodyViewer view={binSmall} id={7} part="request" />)
    await fireEvent.click(getByRole('button', { name: 'Copy Raw' }))
    await waitFor(() =>
      expect(copyToClipboard).toHaveBeenCalledWith(expect.stringContaining('00000000')))
  })

  it('shows Copy failed when the clipboard write throws', async () => {
    vi.mocked(copyToClipboard).mockRejectedValue(new Error('nope'))
    const { getByRole, container } = render(<BodyViewer view={jb} id={1} part="request" />)
    await fireEvent.click(getByRole('button', { name: 'Copy Raw' }))
    await waitFor(() => expect(container.textContent).toContain('Copy failed'))
  })

  it('shows Copy cURL only on the request part', () => {
    const req = render(<BodyViewer view={jb} id={5} part="request" />)
    expect(within(req.container).getByRole('button', { name: 'Copy cURL' })).toBeTruthy()
    const resp = render(<BodyViewer view={jb} id={5} part="response" />)
    expect(within(resp.container).queryByRole('button', { name: 'Copy cURL' })).toBeNull()
  })

  it('copies the curl command built from the fetched capture', async () => {
    vi.mocked(copyToClipboard).mockResolvedValue(undefined)
    vi.mocked(getCapture).mockResolvedValue(detail)
    const { getByRole } = render(<BodyViewer view={jb} id={5} part="request" />)
    await fireEvent.click(getByRole('button', { name: 'Copy cURL' }))
    await waitFor(() => expect(copyToClipboard).toHaveBeenCalledWith(
      expect.stringContaining("curl -X POST 'http://127.0.0.1:8080/api?a=1'")))
    expect(copyToClipboard).toHaveBeenCalledWith(expect.stringContaining('--data-binary'))
    expect(copyToClipboard).not.toHaveBeenCalledWith(expect.stringContaining('content-length'))
  })

  it('shows Copy failed when fetching the capture for cURL fails', async () => {
    vi.mocked(getCapture).mockRejectedValue(new Error('boom'))
    const { getByRole, container } = render(<BodyViewer view={jb} id={5} part="request" />)
    await fireEvent.click(getByRole('button', { name: 'Copy cURL' }))
    await waitFor(() => expect(container.textContent).toContain('Copy failed'))
  })

  it('shows Copy Hurl only on the request part', () => {
    const req = render(<BodyViewer view={jb} id={5} part="request" />)
    expect(within(req.container).getByRole('button', { name: 'Copy Hurl' })).toBeTruthy()
    const resp = render(<BodyViewer view={jb} id={5} part="response" />)
    expect(within(resp.container).queryByRole('button', { name: 'Copy Hurl' })).toBeNull()
  })

  it('copies a hurl file with a raw-fenced body and proxy URL', async () => {
    vi.mocked(copyToClipboard).mockResolvedValue(undefined)
    vi.mocked(getCapture).mockResolvedValue(detail)
    const { getByRole } = render(<BodyViewer view={jb} id={5} part="request" />)
    await fireEvent.click(getByRole('button', { name: 'Copy Hurl' }))
    await waitFor(() => expect(copyToClipboard).toHaveBeenCalledWith(
      expect.stringContaining('```raw')))
    expect(copyToClipboard).toHaveBeenCalledWith(
      expect.stringContaining('POST http://127.0.0.1:8080/api?a=1'))
  })

  it('shows Copy failed when hurl generation/clipboard fails', async () => {
    vi.mocked(copyToClipboard).mockRejectedValue(new Error('nope'))
    vi.mocked(getCapture).mockResolvedValue(detail)
    const { getByRole, container } = render(<BodyViewer view={jb} id={5} part="request" />)
    await fireEvent.click(getByRole('button', { name: 'Copy Hurl' }))
    await waitFor(() => expect(container.textContent).toContain('Copy failed'))
  })

  it('shows the decoded-from badge on view-decoded bodies', () => {
    const gz = bd({ text: '{"ok":true}', len: 9, decoded_encoding: 'gzip' })
    const { container, queryByText } = render(
      <BodyViewer view={gz} id={1} part="response" contentType="application/json" />)
    expect(queryByText('decoded from gzip')).not.toBeNull()
    // The decoded body flows through the normal JSON branch (pretty-printed).
    expect(container.textContent).toContain('"ok": true')
  })
  const fb = { text: 'user=john+doe&pw=***&flag=1&flag=2', binary: false, hex_preview: null, len: 36, sha256: null, truncated: false, text_truncated: false, decoded_encoding: null }

  it('renders a decoded table for form-urlencoded bodies (default tab)', () => {
    const { container } = render(
      <BodyViewer view={fb} id={1} part="request" contentType="application/x-www-form-urlencoded; charset=UTF-8" />)
    const rows = container.querySelectorAll('tbody tr')
    expect(rows).toHaveLength(4)
    const cell = (r: Element, c: number) => r.children[c].textContent
    expect(cell(rows[0], 0)).toBe('user')
    expect(cell(rows[0], 1)).toBe('john doe') // '+' decodes to a space
    expect(cell(rows[1], 0)).toBe('pw')
    expect(cell(rows[1], 1)).toBe('***') // stored mask passes through
    expect(cell(rows[2], 1)).toBe('1')
    expect(cell(rows[3], 1)).toBe('2') // duplicate keys preserved
  })

  it('shows the undecoded form body under Raw', () => {
    const { container, getByRole } = render(
      <BodyViewer view={fb} id={1} part="request" contentType="application/x-www-form-urlencoded" />)
    fireEvent.click(getByRole('button', { name: 'Raw' }))
    expect(container.querySelector('pre')?.textContent)
      .toBe('user=john+doe&pw=***&flag=1&flag=2')
  })

  it('falls back to raw when a form body has no pairs', () => {
    const empty = { ...fb, text: '', len: 0 }
    const { container } = render(
      <BodyViewer view={empty} id={1} part="request" contentType="application/x-www-form-urlencoded" />)
    expect(container.querySelector('table')).toBeNull()
    expect(container.querySelector('pre')).not.toBeNull()
  })
})
