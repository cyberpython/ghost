// Query string (without '?') → ordered [name, value] pairs; duplicates preserved.
export function parseQuery(qs: string): [string, string][] {
  if (!qs) return []
  return [...new URLSearchParams(qs).entries()]
}

// Pull the query string out of a request target (`path?query`).
export function targetQuery(target: string): string {
  const i = target.indexOf('?')
  return i >= 0 ? target.slice(i + 1) : ''
}
