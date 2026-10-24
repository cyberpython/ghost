import { useEffect, useState } from 'react'
import { useParams, useNavigate } from 'react-router'
import { getCapture, getAdjacent } from '../api'
import type { CaptureDetail, AdjacentIds } from '../api'
import Header from '../components/Header'
import BodyViewer from '../components/BodyViewer'
import MethodBadge from '../components/MethodBadge'
import StatusBadge from '../components/StatusBadge'
import { upstreamUrl } from '../lib/assembleUrl'
import { parseQuery, targetQuery } from '../lib/parseQuery'
import { navBtn } from '../lib/navBtn'
export default function Detail({ auto, onAuto }: { auto: boolean; onAuto: (v: boolean) => void }){
  const {id}=useParams() as {id:string}
  const navigate=useNavigate()
  const [d,setD]=useState<CaptureDetail | null>(null)
  const [adj,setAdj]=useState<{for:string; a:AdjacentIds}|null>(null)
  // Cancellation guards: a late-resolving fetch from the previous capture must
  // not clobber the current one. adj is keyed by the id it was fetched for, so
  // Prev/Next never show the previous capture's neighbors while loading.
  useEffect(()=>{ let alive=true; getCapture(Number(id)).then(x=>{ if(alive) setD(x) }).catch(console.error); return ()=>{ alive=false } },[id])
  useEffect(()=>{ let alive=true; getAdjacent(Number(id)).then(a=>{ if(alive) setAdj({for:id,a}) }).catch(console.error); return ()=>{ alive=false } },[id])
  const cur=adj && adj.for===id ? adj.a : null
  // Same button styling/text as the list pager's Prev/Next (FR-039); the ends
  // of the sequence disable. Orientation is S/N: Prev = lower id (temporally
  // earlier), Next = higher id.
  const nav = cur && (
    <>
      <button disabled={cur.prev===null} onClick={()=>navigate(`/captures/${cur.prev}`)} className={navBtn} title="previous capture (lower id)">
        <span className="inline-flex items-center gap-1"><svg className="w-3 h-3" fill="none" viewBox="0 0 12 12" stroke="currentColor" strokeWidth={1.5} aria-hidden="true"><path d="M7 2L3 6l4 4" strokeLinecap="round" strokeLinejoin="round"/></svg>Prev</span>
      </button>
      <button disabled={cur.next===null} onClick={()=>navigate(`/captures/${cur.next}`)} className={navBtn} title="next capture (higher id)">
        <span className="inline-flex items-center gap-1">Next<svg className="w-3 h-3" fill="none" viewBox="0 0 12 12" stroke="currentColor" strokeWidth={1.5} aria-hidden="true"><path d="M5 2l4 4-4 4" strokeLinecap="round" strokeLinejoin="round"/></svg></span>
      </button>
    </>
  )
  if(!d) return (<>
    <Header auto={auto} onAuto={onAuto} middle={nav}/>
    <main className="px-4"><p className="muted">loading…</p></main>
  </>)
  const reqCt = d.request_headers.find(h=>h.name==='content-type')?.value
  const respCt = d.response_headers.find(h=>h.name==='content-type')?.value
  const params = parseQuery(targetQuery(d.request_target))
  const infoRows: {label: string; value: React.ReactNode}[] = [
    { label: 'Method', value: <MethodBadge m={d.request_method}/> },
    { label: 'Target', value: d.request_target },
    { label: 'Status', value: <StatusBadge code={d.status_code}/> },
    { label: 'Created', value: d.created_at },
    { label: 'Duration', value: `${d.total_ms} ms` },
    { label: 'Client address', value: d.client_addr },
    { label: 'Upstream', value: upstreamUrl(d) },
    { label: 'Request version', value: d.request_version },
    { label: 'Response version', value: d.response_version ?? '—' },
  ]
  return (<>
    <Header auto={auto} onAuto={onAuto} middle={nav}/>
    <main className="px-4">
    <div className="flex justify-end mb-2">
      <a href={`/api/export/captures/${d.id}`} download
        className="border border-[var(--border)] rounded px-2.5 py-1 text-sm bg-transparent text-[var(--text)] hover:border-[var(--accent)] transition-colors">
        Export HAR
      </a>
    </div>
    {d.error && <div className="bg-[color-mix(in_srgb,var(--s-5xx)_15%,transparent)] text-[var(--s-5xx)] px-2.5 py-1.5 rounded mb-2">error: {d.error}</div>}
    <table className="border-collapse w-full mb-4">
      <tbody>
        {infoRows.map(({label, value}) => (
          <tr key={label} className="border-b border-[var(--border)]">
            <td className="text-[var(--muted)] px-2.5 py-1.5 align-top whitespace-nowrap">{label}</td>
            <td className="px-2.5 py-1.5 break-all min-w-0">{value}</td>
          </tr>
        ))}
      </tbody>
    </table>
    <h3 className="text-lg font-semibold mb-2">Query parameters</h3>
    {params.length ? (
      <table className="border-collapse w-full table-fixed">
        <thead><tr><th className="text-left px-2.5 py-1.5 border-b border-[var(--border)] w-1/2">Name</th><th className="text-left px-2.5 py-1.5 border-b border-[var(--border)]">Value</th></tr></thead>
        <tbody>{params.map(([k, v], i) => <tr key={i} className="border-b border-[var(--border)]"><td className="px-2.5 py-1.5 break-all min-w-0">{k}</td><td className="px-2.5 py-1.5 break-all min-w-0">{v}</td></tr>)}</tbody>
      </table>
    ) : <p className="muted">—</p>}
    <div className="grid grid-cols-1 gap-6 min-[960px]:grid-cols-2">
      <div>
        <h3 className="text-lg font-semibold mb-2 mt-4">Request headers</h3>
        <table className="border-collapse w-full table-fixed">
          <thead><tr><th className="text-left px-2.5 py-1.5 border-b border-[var(--border)] w-1/3">Name</th><th className="text-left px-2.5 py-1.5 border-b border-[var(--border)]">Value</th></tr></thead>
          <tbody>{d.request_headers.map((h,i)=><tr key={i} className="border-b border-[var(--border)]"><td className="px-2.5 py-1.5 break-all min-w-0">{h.name}</td><td className="px-2.5 py-1.5 break-all min-w-0">{h.value}</td></tr>)}</tbody>
        </table>
        <h3 className="text-lg font-semibold mb-2 mt-4">Request body</h3>
        <BodyViewer view={d.request_body} contentType={reqCt} id={d.id} part="request"/>
      </div>
      <div>
        <h3 className="text-lg font-semibold mb-2 mt-4">Response headers</h3>
        <table className="border-collapse w-full table-fixed">
          <thead><tr><th className="text-left px-2.5 py-1.5 border-b border-[var(--border)] w-1/3">Name</th><th className="text-left px-2.5 py-1.5 border-b border-[var(--border)]">Value</th></tr></thead>
          <tbody>{d.response_headers.map((h,i)=><tr key={i} className="border-b border-[var(--border)]"><td className="px-2.5 py-1.5 break-all min-w-0">{h.name}</td><td className="px-2.5 py-1.5 break-all min-w-0">{h.value}</td></tr>)}</tbody>
        </table>
        <h3 className="text-lg font-semibold mb-2 mt-4">Response body</h3>
        <BodyViewer view={d.response_body} contentType={respCt} id={d.id} part="response"/>
      </div>
    </div>
    </main>
  </>)
}
