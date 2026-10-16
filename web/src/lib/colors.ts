export function methodColor(m: string): string {
  switch (m.toUpperCase()) {
    case 'GET': return '#22c55e'
    case 'POST': return '#38bdf8'
    case 'PUT': return '#f59e0b'
    case 'DELETE': return '#ef4444'
    case 'HEAD': return '#a78bfa'
    default: return '#8b93a7'
  }
}
export function statusColor(code: number | null): string {
  if (code === null) return '#a78bfa'
  const h = Math.floor(code / 100)
  if (h === 2) return '#22c55e'
  if (h === 3) return '#38bdf8'
  if (h === 4) return '#f59e0b'
  return '#ef4444'
}
