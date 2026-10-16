import { useState } from 'react'
import { detectBodyType } from '../lib/detectBodyType'
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
  const banner = (view.truncated || view.text_truncated)
    ? <div className="bg-[color-mix(in_srgb,var(--s-5xx)_15%,transparent)] text-[var(--s-5xx)] px-2.5 py-1.5 rounded mb-2">body truncated</div> : null
  const meta = (
    <div className="muted text-sm mb-1">
      {view.len} B{view.truncated ? ' · truncated' : ''}
      {view.sha256 ? <> · sha256 {view.sha256}</> : ''}
    </div>
  )
  if (view.binary || view.text === null) {
    const more = view.len > HEX_PREVIEW_BYTES || view.truncated
    return (
      <>
        {meta}
        {banner}
        <div className="text-sm mb-1">binary · {view.len} B</div>
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
        {banner}
        {pretty === null && <div className="bg-[color-mix(in_srgb,var(--s-5xx)_15%,transparent)] text-[var(--s-5xx)] px-2.5 py-1.5 rounded mb-2">not valid JSON — showing raw</div>}
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
      {banner}
      <HL lang="markup" code={view.text as string} />
    </>
  )
  return (
    <>
      {meta}
      {banner}
      <pre className="whitespace-pre-wrap break-all">{view.text as string}</pre>
    </>
  )
}
