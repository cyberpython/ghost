// Single source of color truth for methods and status classes across the app
// (badges, list rows, KPI panel). All values are theme-aware CSS variables —
// dark and light themes override them in index.css, so switching themes needs
// no JS.
export function methodColor(m: string): string {
  switch (m.toUpperCase()) {
    case 'GET': return 'var(--m-get)'
    case 'POST': return 'var(--m-post)'
    case 'PUT': return 'var(--m-put)'
    case 'DELETE': return 'var(--m-delete)'
    case 'HEAD': return 'var(--m-head)'
    default: return 'var(--m-other)'
  }
}
export function statusColor(code: number | null): string {
  if (code === null) return 'var(--s-null)'
  const h = Math.floor(code / 100)
  if (h === 2) return 'var(--s-2xx)'
  if (h === 3) return 'var(--s-3xx)'
  if (h === 4) return 'var(--s-4xx)'
  return 'var(--s-5xx)'
}
// KPI panels aggregate by class, not by concrete code; same palette.
export function statusClassColor(sc: string): string {
  switch (sc) {
    case '2xx': return 'var(--s-2xx)'
    case '3xx': return 'var(--s-3xx)'
    case '4xx': return 'var(--s-4xx)'
    case '5xx': return 'var(--s-5xx)'
    case 'err': return 'var(--s-null)'
    default: return 'var(--muted)'
  }
}
