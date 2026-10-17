import type { CaptureDetail } from '../api'

// Shell-quote an argument as a single-quoted bash string ('...' with the
// classic `'\''` escape for embedded single quotes).
const shq = (s: string): string => `'${s.replaceAll("'", `'\\''`)}'`

// Build a bash `curl` that replays the stored request **through the proxy**:
// authority = the client's original `Host` header, path/query = the stored
// (display-masked) request target. Re-issuing the copied command therefore
// runs the exchange through the proxy again and produces a fresh capture.
// The upstream URL plays no role in the copied command.
//
// - Framing headers (content-length / transfer-encoding) are omitted: curl
//   derives framing from the data it sends (FR-005's rebuilt framing).
// - The recorded `Host` becomes the URL authority rather than a `-H Host`
//   argument (curl sets Host from the URL) — replayed byte-exact when
//   present; when the client omitted Host, we fall back to the viewer's own
//   origin (a rare HTTP/1.0-style client; the proxy injected the upstream
//   authority in that case, which we cannot know client-side).
// - Sensitive values (headers + query params) are stored masked and stay
//   visibly `***` in the copied command.
// - A text body is embedded as `--data-binary`; binary/truncated bodies get
//   a bash comment instead (they cannot be inlined losslessly).
export function copyCurlCommand(d: CaptureDetail): string {
  const body = d.request_body?.text ?? null
  const hasBody = !!body
  const hostHeader = d.request_headers.find(h => h.name.toLowerCase() === 'host')?.value
  const authority = hostHeader || window.location.host
  const lines: string[] = []
  const head = ['curl']
  // `-X` when the method is non-GET or when a body would make curl
  // silently switch to POST otherwise.
  if (d.request_method !== 'GET' || hasBody) head.push('-X', d.request_method)
  head.push(shq(`http://${authority}${d.request_target}`))
  lines.push(head.join(' '))
  for (const h of d.request_headers) {
    const n = h.name.toLowerCase()
    if (n === 'host' || n === 'content-length' || n === 'transfer-encoding') continue
    lines.push(`-H ${shq(`${h.name}: ${h.value}`)}`)
  }
  if (body) {
    lines.push(`--data-binary ${shq(body)}`)
  } else if (d.request_body && d.request_body.len > 0) {
    lines.push(`# body not embedded: binary or truncated (${d.request_body.len} B stored)`)
  }
  return lines.join(' \\\n  ')
}
