import { describe, it, expect, vi } from 'vitest'
import type { BodyView, CaptureDetail } from '../api'
import { copyHurlFile, bytesToBase64 } from './hurl'

const bv = (text: string | null): BodyView => ({
  text, text_truncated: false, binary: text === null,
  hex_preview: null, len: 0, sha256: null, truncated: false, decoded_encoding: null,
})

const detail = (over: Partial<CaptureDetail> = {}): CaptureDetail => ({
  id: 12, created_at: 't', total_ms: 5, client_addr: 'c',
  request_version: 'HTTP/1.1', request_method: 'POST', request_target: '/api?a=1',
  request_headers: [
    { name: 'host', value: '127.0.0.1:8080' },
    { name: 'content-length', value: '9' },
    { name: 'authorization', value: 'Be***23' },
    { name: 'content-type', value: 'application/json' },
  ],
  request_body: bv('{"pw":"***"}'),
  upstream_scheme: 'http', upstream_authority: 'up:9000', upstream_path: '/app/api',
  upstream_query: 'a=1',
  response_version: null, status_code: null, response_headers: [],
  response_body: bv(null), error: null,
  ...over,
})

describe('copyHurlFile', () => {
  it('golden: comment, proxy URL, headers (framing/host skipped), raw-fenced body', () => {
    expect(copyHurlFile(detail())).toBe(
      "POST http://127.0.0.1:8080/api?a=1\n" +
      'authorization: Be***23\n' +
      'content-type: application/json\n' +
      '```raw\n' +
      '{"pw":"***"}\n' +
      '```\n',
    )
  })

  it('templates are never evaluated: {{name}} passes through inside the raw fence', () => {
    const cmd = copyHurlFile(detail({ request_body: bv('{"name":"{{ name }}"}') }))
    expect(cmd).toContain('```raw')
    expect(cmd).toContain('{"name":"{{ name }}"}')
  })

  it('a stored trailing newline is preserved (not doubled)', () => {
    const cmd = copyHurlFile(detail({ request_body: bv('x\n') }))
    expect(cmd).toContain('```raw\nx\n```')
    expect(cmd).not.toContain('x\n\n```')
  })

  it('a body containing the fence itself falls back to base64', () => {
    const bytes = new TextEncoder().encode('a```b')
    const expected = `base64,${bytesToBase64(bytes)};`
    const cmd = copyHurlFile(detail({ request_body: bv('a```b') }))
    expect(cmd).toContain(expected)
    expect(cmd).not.toContain('```raw')
  })

  it('binary stored body (not truncated) embeds via the base64 parameter', () => {
    const cmd = copyHurlFile(detail({
      request_body: { text: null, text_truncated: false, binary: true, hex_preview: null, len: 3, sha256: null, truncated: false, decoded_encoding: null },
    }), 'QUJD')
    expect(cmd).toContain('base64,QUJD;')
    expect(cmd).not.toContain('```raw')
  })

  it('binary body without supplied bytes: safe fallback comment', () => {
    const cmd = copyHurlFile(detail({ request_body: { text: null, text_truncated: false, binary: true, hex_preview: null, len: 4096, sha256: null, truncated: false, decoded_encoding: null } }))
    expect(cmd).toContain('# body not embedded: binary (4096 B)')
  })

  it('truncated bodies: comment with on-wire length, no data', () => {
    const cmd = copyHurlFile(detail({
      request_body: { text: '{"pw":"***"', text_truncated: true, binary: false, hex_preview: null, len: 4096, sha256: null, truncated: true, decoded_encoding: null },
    }))
    expect(cmd).not.toContain('```raw')
    expect(cmd).not.toContain('base64,')
    expect(cmd).toContain('# body not embedded: truncated (4096 B on wire)')
  })

  it('header values escape backslash, double quote and hash; # in URL escaped', () => {
    const cmd = copyHurlFile(detail({
      request_method: 'GET', request_target: '/x#frag', request_body: bv(null),
      request_headers: [
        { name: 'host', value: 'h:1' },
        { name: 'x-token', value: 'BEEF #STEAK' },
        { name: 'x-q', value: 'say "hi" \\ ok' },
      ],
    }))
    expect(cmd).toContain('GET http://h:1/x\\#frag')
    expect(cmd).toContain('x-token: BEEF \\#STEAK')
    expect(cmd).toContain('x-q: say \\"hi\\" \\\\ ok')
  })

  it('GET without body: single method line, no fence; Host-less client falls back to viewer origin', () => {
    vi.stubGlobal('location', { host: 'viewer.example:9999' })
    const cmd = copyHurlFile(detail({
      request_method: 'GET', request_body: bv(null),
      request_headers: [{ name: 'x-test', value: 'y' }],
    }))
    vi.unstubAllGlobals()
    expect(cmd).toContain('GET http://viewer.example:9999/api?a=1')
    expect(cmd).not.toContain('```')
  })

  it('empty stored body: no body section at all', () => {
    const cmd = copyHurlFile(detail({
      request_method: 'GET', request_body: bv(''),
      request_headers: [{ name: 'host', value: 'h:1' }],
    }))
    expect(cmd).not.toContain('```')
    expect(cmd).not.toContain('# body not embedded')
  })

  it('view-decoded (gzip) bodies embed the RAW compressed bytes via base64', () => {
    const cmd = copyHurlFile(detail({
      request_body: { text: '{"ok":true}', text_truncated: false, binary: false, hex_preview: null, len: 40, sha256: null, truncated: false, decoded_encoding: 'gzip' },
    }), 'H4sIAAAAAAAC')
    expect(cmd).toContain('base64,H4sIAAAAAAAC;')
    expect(cmd).not.toContain('```raw')
  })

  it('view-decoded body without supplied bytes: safe fallback comment', () => {
    const cmd = copyHurlFile(detail({
      request_body: { text: '{"ok":true}', text_truncated: false, binary: false, hex_preview: null, len: 40, sha256: null, truncated: false, decoded_encoding: 'gzip' },
    }))
    expect(cmd).toContain('# body not embedded')
    expect(cmd).not.toContain('```raw')
  })
})
