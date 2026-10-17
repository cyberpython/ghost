import type { CaptureDetail } from '../api'

// UTF-8 bytes → base64 (chunked to dodge call-stack limits on large bodies).
export function bytesToBase64(b: Uint8Array): string {
  let s = ''
  const chunk = 0x8000
  for (let i = 0; i < b.length; i += chunk) {
    s += String.fromCharCode(...b.subarray(i, i + chunk))
  }
  return btoa(s)
}

// Hurl string escaping (header values, URLs): backslash, double quote, and
// `#` (which would otherwise start a comment) must be escaped. Fenced
// multiline content needs no escaping — escapes are not processed there.
const hesc = (s: string): string =>
  s.replaceAll('\\', '\\\\').replaceAll('"', '\\"').replaceAll('#', '\\#')

// Build a single-entry `.hurl` file that replays the stored request through
// the proxy (client Host header as the authority; masked request target as
// path/query) — same replay semantics as Copy cURL. See docs/hurl-format.md.
//
// Body strategy (format hazards documented in hurl-format.md §3-4):
// - text body → ```raw fenced multiline: no template evaluation of `{{…}}`,
//   no escape processing; a missing trailing newline gets one added
//   (multiline bodies are "lines + \n"), a present one is preserved by
//   trimming it before splitting;
// - text containing the fence itself → `base64,` of the UTF-8 bytes (exact);
// - binary stored body (fetch supplies `binaryB64`) → `base64,…;` byte-exact;
// - truncated body → comment only (the full on-wire body is not available).
export function copyHurlFile(d: CaptureDetail, binaryB64?: string | null): string {
  const body = d.request_body
  const lines: string[] = []
  const host =
    d.request_headers.find(h => h.name.toLowerCase() === 'host')?.value ||
    window.location.host
  lines.push(`${d.request_method} ${hesc(`http://${host}${d.request_target}`)}`)
  for (const h of d.request_headers) {
    const n = h.name.toLowerCase()
    if (n === 'host' || n === 'content-length' || n === 'transfer-encoding') continue
    lines.push(`${h.name}: ${hesc(h.value)}`)
  }
  if (body && body.text !== null && !body.truncated && body.text.length > 0) {
    if (body.text.includes('```')) {
      lines.push(`base64,${bytesToBase64(new TextEncoder().encode(body.text))};`)
    } else {
      const content = body.text.endsWith('\n') ? body.text.slice(0, -1) : body.text
      lines.push('```raw', ...content.split('\n'), '```')
    }
  } else if (body && body.len > 0) {
    lines.push(
      body.truncated
        ? `# body not embedded: truncated (${body.len} B on wire)`
        : binaryB64
          ? `base64,${binaryB64};`
          : `# body not embedded: binary (${body.len} B)`,
    )
  }
  return lines.join('\n') + '\n'
}
