// Format a space-separated hex string (as returned by `hex_preview`) into
// classic hexdump lines: 8-hex-digit offset, 32 hex bytes (extra gap after
// 16), and an ASCII gutter (printables as-is, everything else '.').
export const BYTES_PER_LINE = 32

export function hexDump(hex: string): string {
  const bytes = hex.match(/[0-9a-fA-F]{2}/g) ?? []
  const lines: string[] = []
  for (let i = 0; i < bytes.length; i += BYTES_PER_LINE) {
    const chunk = bytes.slice(i, i + BYTES_PER_LINE)
    const head = chunk.slice(0, 16).join(' ')
    const tail = chunk.slice(16).join(' ')
    const hexPart = tail ? head + '  ' + tail : head
    const ascii = chunk
      .map(b => {
        const c = parseInt(b, 16)
        return c >= 0x20 && c <= 0x7e ? String.fromCharCode(c) : '.'
      })
      .join('')
    lines.push(`${i.toString(16).padStart(8, '0')}  ${hexPart.padEnd(96)}  |${ascii}|`)
  }
  return lines.join('\n')
}
