import { describe, it, expect, vi } from 'vitest'
import { render, fireEvent, waitFor } from '@testing-library/react'
import BodyViewer from './BodyViewer'
import { copyToClipboard } from '../lib/clipboard'

vi.mock('../lib/clipboard', () => ({ copyToClipboard: vi.fn() }))
const jb = { text: '{"a":1}', binary: false, hex_preview: null, len: 5, sha256: null, truncated: false, text_truncated: false }
const xb = { text: '<?xml version="1.0"?><root>hi</root>', binary: false, hex_preview: null, len: 0, sha256: null, truncated: false, text_truncated: false }
const bin = { text: null, binary: true, hex_preview: 'ff fe 00 41 42', len: 2048, sha256: null, truncated: true, text_truncated: false }
const binSmall = { text: null, binary: true, hex_preview: 'ff fe 00 41 42', len: 5, sha256: null, truncated: false, text_truncated: false }
const badJson = { text: '{"a":1', binary: false, hex_preview: null, len: 6, sha256: null, truncated: true, text_truncated: true }
describe('BodyViewer', () => {
  it('highlights JSON', () => { const { container } = render(<BodyViewer view={jb} id={1} part="response" />); expect(container.querySelectorAll('.token').length).toBeGreaterThan(0) })
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
  it('shows the full sha256, not an elided prefix', () => {
    const h = 'ab'.repeat(32)
    const { container } = render(<BodyViewer view={{ ...jb, sha256: h }} id={1} part="request" />)
    expect(container.textContent).toContain(`sha256 ${h}`)
    expect(container.textContent).not.toContain('…')
  })

  it('copies the text body to the clipboard', async () => {
    vi.mocked(copyToClipboard).mockResolvedValue(undefined)
    const { getByRole, container } = render(<BodyViewer view={jb} id={1} part="response" />)
    await fireEvent.click(getByRole('button', { name: 'Copy Body' }))
    await waitFor(() => expect(copyToClipboard).toHaveBeenCalledWith('{"a":1}'))
    await waitFor(() => expect(container.textContent).toContain('Copied!'))
  })

  it('copies the hexdump representation for binary bodies', async () => {
    vi.mocked(copyToClipboard).mockResolvedValue(undefined)
    const { getByRole } = render(<BodyViewer view={binSmall} id={7} part="request" />)
    await fireEvent.click(getByRole('button', { name: 'Copy Body' }))
    await waitFor(() =>
      expect(copyToClipboard).toHaveBeenCalledWith(expect.stringContaining('00000000')))
  })

  it('shows Copy failed when the clipboard write throws', async () => {
    vi.mocked(copyToClipboard).mockRejectedValue(new Error('nope'))
    const { getByRole, container } = render(<BodyViewer view={jb} id={1} part="request" />)
    await fireEvent.click(getByRole('button', { name: 'Copy Body' }))
    await waitFor(() => expect(container.textContent).toContain('Copy failed'))
  })
})
