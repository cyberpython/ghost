export type BodyType = 'json' | 'xml' | 'html' | 'text' | 'binary'
export function detectBodyType(ct: string | null | undefined, text: string, isBinary: boolean): BodyType {
  if (isBinary) return 'binary'
  const base = ((ct || '').toLowerCase().split(';')[0]).trim()
  if (base === 'application/json' || base.endsWith('+json')) return 'json'
  if (base === 'application/xml' || base === 'text/xml' || base.endsWith('+xml')) return 'xml'
  if (base === 'text/html') return 'html'
  const t = text.trim()
  if (t.startsWith('{') || t.startsWith('[')) { try { JSON.parse(t); return 'json' } catch {} }
  if (t.startsWith('<?xml')) return 'xml'
  if (t.startsWith('<!doctype') || t.startsWith('<html')) return 'html'
  return 'text'
}
