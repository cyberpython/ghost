import { describe, it, expect, vi, afterEach } from 'vitest'
import type { BodyView, CaptureDetail } from '../api'
import { copyCurlCommand } from './curl'

const bv = (text: string | null): BodyView => ({
  text, text_truncated: false, binary: text === null,
  hex_preview: null, len: 0, sha256: null, truncated: false,
})

const detail = (over: Partial<CaptureDetail> = {}): CaptureDetail => ({
  id: 1, created_at: 't', total_ms: 5, client_addr: 'c',
  request_version: 'HTTP/1.1', request_method: 'POST', request_target: '/api?a=1',
  request_headers: [
    { name: 'host', value: '127.0.0.1:8080' },
    { name: 'content-length', value: '9' },
    { name: 'authorization', value: 'Be***23' },
    { name: 'x-test', value: 'abc' },
    { name: 'content-type', value: 'application/json' },
  ],
  request_body: bv('{"pw":"***"}'),
  // The upstream plays no role in the copied command; kept in the fixture to
  // prove it is never referenced.
  upstream_scheme: 'http', upstream_authority: 'up:9000', upstream_path: '/app/api',
  upstream_query: 'a=1',
  response_version: null, status_code: null, response_headers: [],
  response_body: bv(null), error: null,
  ...over,
})

afterEach(() => vi.unstubAllGlobals())

describe('copyCurlCommand', () => {
  it('replays through the proxy: client Host + stored target, ordered headers, body', () => {
    expect(copyCurlCommand(detail())).toBe(
      "curl -X POST 'http://127.0.0.1:8080/api?a=1' \\\n" +
      "  -H 'authorization: Be***23' \\\n" +
      "  -H 'x-test: abc' \\\n" +
      "  -H 'content-type: application/json' \\\n" +
      `  --data-binary '{"pw":"***"}'`,
    )
  })

  it('never references the upstream', () => {
    const cmd = copyCurlCommand(detail())
    expect(cmd).not.toContain('up:9000')
    expect(cmd).not.toContain('/app/')
  })

  it('plain GET without body: no -X, no --data', () => {
    const cmd = copyCurlCommand(detail({
      request_method: 'GET', request_target: '/x', request_body: bv(null),
    }))
    expect(cmd).toBe(
      "curl 'http://127.0.0.1:8080/x' \\\n" +
      "  -H 'authorization: Be***23' \\\n" +
      "  -H 'x-test: abc' \\\n" +
      "  -H 'content-type: application/json'",
    )
  })

  it('escapes embedded single quotes bash-style', () => {
    const cmd = copyCurlCommand(detail({
      request_method: 'GET', request_body: bv(null),
      request_headers: [{ name: 'host', value: 'h:1' }, { name: 'x-apos', value: "it's" }],
    }))
    expect(cmd).toContain("-H 'x-apos: it'\\''s'")
  })

  it('binary/truncated bodies become a comment, not data', () => {
    const cmd = copyCurlCommand(detail({
      request_body: { text: null, text_truncated: false, binary: true, hex_preview: null, len: 4096, sha256: null, truncated: true },
    }))
    expect(cmd).not.toContain('--data-binary')
    expect(cmd).toContain('# body not embedded: binary or truncated (4096 B stored)')
  })

  it('GET with a body keeps -X GET (curl would switch to POST otherwise)', () => {
    const cmd = copyCurlCommand(detail({
      request_method: 'GET', request_target: '/x',
    }))
    expect(cmd).toContain("-X GET")
    expect(cmd).toContain('--data-binary')
  })

  it('falls back to the viewer origin when the client omitted Host', () => {
    vi.stubGlobal('location', { host: 'viewer.example:9999' })
    const cmd = copyCurlCommand(detail({ request_headers: [{ name: 'x-test', value: 'y' }] }))
    expect(cmd).toContain("'http://viewer.example:9999/api?a=1'")
  })
})
