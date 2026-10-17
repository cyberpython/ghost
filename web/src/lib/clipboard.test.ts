import { describe, it, expect, vi, afterEach } from 'vitest'
import { copyToClipboard } from './clipboard'

describe('copyToClipboard', () => {
  afterEach(() => {
    vi.unstubAllGlobals()
    delete (document as any).execCommand
  })

  it('prefers the async Clipboard API', async () => {
    const writeText = vi.fn().mockResolvedValue(undefined)
    vi.stubGlobal('navigator', { clipboard: { writeText } })
    await copyToClipboard('hello')
    expect(writeText).toHaveBeenCalledWith('hello')
  })

  it('falls back to execCommand when the Clipboard API is missing', async () => {
    vi.stubGlobal('navigator', {})
    const exec = vi.fn().mockReturnValue(true)
    Object.defineProperty(document, 'execCommand', {
      value: exec, configurable: true, writable: true,
    })
    await copyToClipboard('hello')
    expect(exec).toHaveBeenCalledWith('copy')
  })

  it('throws when no copy mechanism is available', async () => {
    vi.stubGlobal('navigator', {})
    const exec = vi.fn().mockReturnValue(false)
    Object.defineProperty(document, 'execCommand', {
      value: exec, configurable: true, writable: true,
    })
    await expect(copyToClipboard('hello')).rejects.toThrow('copy failed')
  })
})
