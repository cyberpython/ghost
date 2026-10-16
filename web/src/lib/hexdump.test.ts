import { describe, it, expect } from 'vitest'
import { hexDump } from './hexdump'
describe('hexDump', () => {
  it('one line for a short body', () => {
    expect(hexDump('ff fe 00 41')).toBe(`00000000  ${'ff fe 00 41'.padEnd(96)}  |...A|`)
  })
  it('breaks at 32 bytes', () => {
    const bytes = Array.from({ length: 32 }, () => '01').join(' ') + ' 02'
    const lines = hexDump(bytes).split('\n')
    expect(lines.length).toBe(2)
    expect(lines[0]).toContain('00000000')
    expect(lines[1]).toContain('00000020')
    expect(lines[1]).toContain('02')
  })
  it('widens the gap after the 16th byte', () => {
    const w = (n: number) => hexDump(Array.from({ length: n }, () => '41').join(' '))
    expect(w(16)).not.toContain('41  41')
    expect(w(17)).toContain('41  41')
  })
  it('empty input', () => expect(hexDump('')).toBe(''))
})
