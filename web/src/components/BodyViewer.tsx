import { useState } from 'react'
import { detectBodyType } from '../lib/detectBodyType'
import { copyToClipboard } from '../lib/clipboard'
import { HL } from '../lib/highlight'
import { hexDump } from '../lib/hexdump'
import type { BodyView } from '../api'
import { getCapture } from '../api'
import { copyCurlCommand } from '../lib/curl'
import { copyHurlFile, bytesToBase64 } from '../lib/hurl'
import JsonTree, { type Json } from './JsonTree'

export type BodyPart = 'request' | 'response'

// Hex preview cap in the API (`body_view`): beyond this, link to a download.
const HEX_PREVIEW_BYTES = 1024

export default function BodyViewer({ view, contentType, id, part }:
  { view: BodyView; contentType?: string; id: number; part: BodyPart }) {
  const t = detectBodyType(contentType, view.text ?? '', view.binary)
  const [tab, setTab] = useState<'tree' | 'raw'>('tree')
  // JSON parse (FR-026b), done once: it feeds the tree, Copy Formatted and
  // the not-valid-JSON banner. jsonOk distinguishes a parse failure from a
  // body that legitimately parses to null.
  let json: Json = null
  let jsonOk = false
  if (t === 'json' && view.text !== null) {
    try { json = JSON.parse(view.text) as Json; jsonOk = true } catch { jsonOk = false }
  }
  const pretty = jsonOk ? JSON.stringify(json, null, 2) : null

  // Copy affordance (FR-026a): Clipboard API first, execCommand fallback;
  // the label flips briefly (Copied! / Copy failed) on click.
  const [copied, setCopied] = useState<
    'raw-ok' | 'raw-err' | 'fmt-ok' | 'fmt-err' | 'curl-ok' | 'curl-err' | 'hurl-ok' | 'hurl-err' | null
  >(null)
  const copyText = view.text ?? (view.hex_preview ? hexDump(view.hex_preview) : '')
  const resetLater = (k: string) =>
    setTimeout(() => setCopied(c => (c?.startsWith(k) ? null : c)), 1500)
  const btnLabel = (k: 'raw' | 'fmt' | 'curl' | 'hurl', idle: string) =>
    copied === `${k}-ok` ? 'Copied!' : copied === `${k}-err` ? 'Copy failed' : idle
  const rawNow = async () => {
    try {
      await copyToClipboard(copyText)
      setCopied('raw-ok')
    } catch {
      setCopied('raw-err')
    }
    resetLater('raw')
  }
  const fmtNow = async () => {
    try {
      await copyToClipboard(pretty ?? '')
      setCopied('fmt-ok')
    } catch {
      setCopied('fmt-err')
    }
    resetLater('fmt')
  }
  const curlNow = async () => {
    try {
      const d = await getCapture(id)
      await copyToClipboard(copyCurlCommand(d))
      setCopied('curl-ok')
    } catch {
      setCopied('curl-err')
    }
    resetLater('curl')
  }
  const hurlNow = async () => {
    try {
      const d = await getCapture(id)
      let b64: string | null = null
      if (d.request_body.text === null && d.request_body.len > 0 && !d.request_body.truncated) {
        const r = await fetch(`/api/captures/${id}/body?part=request`)
        if (!r.ok) throw new Error('http ' + r.status)
        b64 = bytesToBase64(new Uint8Array(await r.arrayBuffer()))
      }
      await copyToClipboard(copyHurlFile(d, b64))
      setCopied('hurl-ok')
    } catch {
      setCopied('hurl-err')
    }
    resetLater('hurl')
  }
  const btnCls = 'border border-[var(--border)] rounded px-2 py-1 text-xs bg-transparent text-[var(--text)] hover:border-[var(--accent)] transition-colors disabled:opacity-40 disabled:cursor-not-allowed cursor-pointer'
  const copyGroup = (
    <div className="flex items-center gap-2 mb-2">
      <button type="button" onClick={rawNow} disabled={copyText.length === 0} className={btnCls}>
        {btnLabel('raw', 'Copy Raw')}
      </button>
      {jsonOk && (
        <button type="button" onClick={fmtNow} className={btnCls}>
          {btnLabel('fmt', 'Copy Formatted')}
        </button>
      )}
      {part === 'request' && (
        <>
          <button type="button" onClick={curlNow} className={btnCls}>
            {btnLabel('curl', 'Copy cURL')}
          </button>
          <button type="button" onClick={hurlNow} className={btnCls}>
            {btnLabel('hurl', 'Copy Hurl')}
          </button>
        </>
      )}
    </div>
  )
  const banner = (view.truncated || view.text_truncated)
    ? <div className="bg-[color-mix(in_srgb,var(--s-5xx)_15%,transparent)] text-[var(--s-5xx)] px-2.5 py-1.5 rounded mb-2">body truncated</div> : null
  const meta = (
    <div className="muted text-sm mb-1">
      {view.len} B{view.truncated ? ' · truncated' : ''}
    </div>
  )
  const shaLine = view.sha256 ? (
    <div className="muted text-xs font-mono break-all mb-1" title="SHA-256 of the original on-wire body, before masking/truncation — it does not verify the stored copy when masking changed it">
      sha256 {view.sha256}
    </div>
  ) : null
  if (view.binary || view.text === null) {
    const more = view.len > HEX_PREVIEW_BYTES || view.truncated
    return (
      <>
        {meta}
        {shaLine}
        {banner}
        <div className="text-sm mb-1">binary · {view.len} B</div>
        {copyGroup}
        <pre className="hexdump">{hexDump(view.hex_preview ?? '')}</pre>
        {more && (
          <a className="muted text-sm mt-1 block" href={`/api/captures/${id}/body?part=${part}`}
            download>download full body ({view.len} B)</a>
        )}
      </>
    )
  }
  if (t === 'json') {
    const raw = view.text as string
    return (
      <>
        {meta}
        {shaLine}
        {banner}
        {!jsonOk && <div className="bg-[color-mix(in_srgb,var(--s-5xx)_15%,transparent)] text-[var(--s-5xx)] px-2.5 py-1.5 rounded mb-2">not valid JSON — showing raw</div>}
        {copyGroup}
        <div className="flex gap-2 mb-2">
          {jsonOk && <button onClick={() => setTab('tree')} className={tab === 'tree' ? 'px-3 py-1 rounded bg-[var(--panel)] border border-[var(--border)]' : 'px-3 py-1 rounded border border-[var(--border)]'}>Tree</button>}
          <button onClick={() => setTab('raw')} className={tab === 'raw' ? 'px-3 py-1 rounded bg-[var(--panel)] border border-[var(--border)]' : 'px-3 py-1 rounded border border-[var(--border)]'}>Raw</button>
        </div>
        {jsonOk && tab === 'tree' ? <JsonTree value={json} /> : <HL lang="json" code={raw} />}
      </>
    )
  }
  if (t === 'xml' || t === 'html') return (
    <>
      {meta}
      {shaLine}
      {banner}
      {copyGroup}
      <HL lang="markup" code={view.text as string} />
    </>
  )
  return (
    <>
      {meta}
      {shaLine}
      {banner}
      {copyGroup}
      <pre className="whitespace-pre-wrap break-all">{view.text as string}</pre>
    </>
  )
}
