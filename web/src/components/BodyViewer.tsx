import { useState } from 'react'
import { detectBodyType } from '../lib/detectBodyType'
import { copyToClipboard } from '../lib/clipboard'
import { HL } from '../lib/highlight'
import { hexDump } from '../lib/hexdump'
import type { BodyView } from '../api'

export type BodyPart = 'request' | 'response'

// Hex preview cap in the API (`body_view`): beyond this, link to a download.
const HEX_PREVIEW_BYTES = 1024

export default function BodyViewer({ view, contentType, id, part }:
  { view: BodyView; contentType?: string; id: number; part: BodyPart }) {
  const t = detectBodyType(contentType, view.text ?? '', view.binary)
  const [tab, setTab] = useState<'formatted' | 'raw'>('formatted')

  // Copy affordance (FR-026a): Clipboard API first, execCommand fallback;
  // the label flips briefly (Copied! / Copy failed) on click.
  const [copied, setCopied] = useState<'body-ok' | 'body-err' | null>(null)
  const copyText = view.text ?? (view.hex_preview ? hexDump(view.hex_preview) : '')
  const copyNow = async () => {
    try {
      await copyToClipboard(copyText)
      setCopied('body-ok')
    } catch {
      setCopied('body-err')
    }
    setTimeout(() => setCopied(c => (c === 'body-ok' || c === 'body-err' ? null : c)), 1500)
  }
  const btnCls = 'border border-[var(--border)] rounded px-2 py-1 text-xs bg-transparent text-[var(--text)] hover:border-[var(--accent)] transition-colors disabled:opacity-40 disabled:cursor-not-allowed cursor-pointer'
  const copyGroup = (
    <div className="flex items-center gap-2 mb-2">
      <button type="button" onClick={copyNow} disabled={copyText.length === 0} className={btnCls}>
        {copied === 'body-ok' ? 'Copied!' : copied === 'body-err' ? 'Copy failed' : 'Copy Body'}
      </button>
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
    let pretty: string | null = null
    try { pretty = JSON.stringify(JSON.parse(raw), null, 2) } catch { pretty = null }
    return (
      <>
        {meta}
        {shaLine}
        {banner}
        {pretty === null && <div className="bg-[color-mix(in_srgb,var(--s-5xx)_15%,transparent)] text-[var(--s-5xx)] px-2.5 py-1.5 rounded mb-2">not valid JSON — showing raw</div>}
        {copyGroup}
        <div className="flex gap-2 mb-2">
          <button onClick={() => setTab('formatted')} className={tab === 'formatted' ? 'px-3 py-1 rounded bg-[var(--panel)] border border-[var(--border)]' : 'px-3 py-1 rounded border border-[var(--border)]'}>Formatted</button>
          <button onClick={() => setTab('raw')} className={tab === 'raw' ? 'px-3 py-1 rounded bg-[var(--panel)] border border-[var(--border)]' : 'px-3 py-1 rounded border border-[var(--border)]'}>Raw</button>
        </div>
        {pretty !== null && tab === 'formatted' ? <HL lang="json" code={pretty} /> : <HL lang="json" code={raw} />}
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
