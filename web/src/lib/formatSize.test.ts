import { describe, it, expect } from 'vitest'
import { formatSize } from './formatSize'

describe('formatSize', () => {
  it('keeps byte values as-is', () => {
    expect(formatSize(0)).toBe('0 B')
    expect(formatSize(1023)).toBe('1023 B')
  })
  it('shows KB with one decimal under 10, none above', () => {
    expect(formatSize(1024)).toBe('1.0 KB')
    expect(formatSize(1536)).toBe('1.5 KB')
    expect(formatSize(10240)).toBe('10 KB')
    expect(formatSize(1024 * 1023)).toBe('1023 KB')
  })
  it('shows MB with one decimal under 10, none above', () => {
    expect(formatSize(1024 * 1024)).toBe('1.0 MB')
    expect(formatSize(2.5 * 1024 * 1024)).toBe('2.5 MB')
    expect(formatSize(12 * 1024 * 1024)).toBe('12 MB')
  })
  it('falls back to GB for very large bodies', () => {
    expect(formatSize(1024 * 1024 * 1024)).toBe('1.0 GB')
  })
})
