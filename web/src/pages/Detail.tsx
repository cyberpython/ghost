import { useEffect, useState } from 'react'
import { useParams } from 'react-router'
import { getCapture } from '../api'
import type { CaptureDetail } from '../api'
import BodyViewer from '../components/BodyViewer'
import MethodBadge from '../components/MethodBadge'
import StatusBadge from '../components/StatusBadge'
import { upstreamUrl } from '../lib/assembleUrl'
import { parseQuery, targetQuery } from '../lib/parseQuery'
export default function Detail(){
  const {id}=useParams() as {id:string}
  const [d,setD]=useState<CaptureDetail | null>(null)
  useEffect(()=>{ getCapture(Number(id)).then(setD).catch(console.error) },[id])
  if(!d) return <p className="muted">loading…</p>
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
  </>)
}
