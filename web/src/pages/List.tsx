import { useState, useEffect, useCallback } from 'react'
import { useSearchParams } from 'react-router'
import { listCaptures, getStats } from '../api'
import type { CaptureListItem, Stats } from '../api'
import Header from '../components/Header'
import CapturesTable from '../components/CapturesTable'
import SelectDropdown from '../components/SelectDropdown'
import KpiPanel from '../components/KpiPanel'
import TimeGraph from '../components/TimeGraph'
import { navBtn } from '../lib/navBtn'

const HTTP_METHODS = ['GET','POST','PUT','DELETE','PATCH','HEAD','OPTIONS','CONNECT','TRACE']
const METHOD_OPTIONS = [{ label: 'ALL', value: '' }, ...HTTP_METHODS.map(m => ({ label: m, value: m }))]

// End of the hour (YYYY-MM-DDTHH:00:00Z) as an inclusive upper bound.
function endOfHour(hour: string): string {
  const d = new Date(hour)
  d.setUTCMinutes(59, 59)
  return d.toISOString().replace(/\.\d{3}Z$/, 'Z')
}

export default function List({ auto, onAuto }: { auto: boolean; onAuto: (v: boolean) => void }) {
  const [sp,setSp]=useSearchParams()
  const [rows,setRows]=useState<CaptureListItem[]>([])
  const [count,setCount]=useState(0)
  const [stats,setStats]=useState<Stats|null>(null)
  const limit=20
  const offset=Number(sp.get('offset')||'0')
  const filterQuery=useCallback(():{[k:string]:any}=>{
    const q:{[k:string]:any}={}
    for (const k of ['method','status','path','client_addr','since','until','error']) {
      const v=sp.get(k); if(v!==null) q[k]=v
    }
    return q
  },[sp])
  const load=useCallback(async()=>{
    const q=filterQuery()
    const r=await listCaptures({...q,limit,offset})
    setRows(r.captures); setCount(r.count)
  },[filterQuery,offset])
  const loadStats=useCallback(async()=>{
    const s=await getStats(filterQuery())
    setStats(s)
  },[filterQuery])
  useEffect(()=>{load()},[load])
  useEffect(()=>{loadStats()},[loadStats])
  useEffect(()=>{
    if(!auto) return
    const t=setInterval(()=>{load();loadStats()},5000)
    return ()=>clearInterval(t)
  },[auto,load,loadStats])
  const set=(k:string,v:string)=>{
    const n=new URLSearchParams(sp)
    if(v==='')n.delete(k); else n.set(k,v)
    setSp(n,{replace:true})
  }
  const onHourClick=(hour:string)=>{
    const n=new URLSearchParams(sp)
    n.set('since',hour)
    n.set('until',endOfHour(hour))
    setSp(n,{replace:true})
  }
  const clearFilters=()=>{
    const n=new URLSearchParams(sp)
    for (const k of ['method','status','path','client_addr','since','until','error']) n.delete(k)
    n.set('offset','0')
    setSp(n,{replace:true})
  }
  const exportUrl=()=>{
    const p=new URLSearchParams()
    for (const k of ['method','status','path','client_addr','since','until','error']) {
      const v=sp.get(k); if(v!==null) p.set(k,v)
    }
    p.set('limit','500')
    return `/api/export/captures?${p.toString()}`
  }
  const totalPages=Math.max(1,Math.ceil(count/limit))
  const page=Math.min(Math.floor(offset/limit)+1,totalPages)
  const [pageInput,setPageInput]=useState(String(page))
  useEffect(()=>{setPageInput(String(page))},[page])
  const goToPage=(p:number)=>{
    const clamped=Math.max(1,Math.min(p,totalPages))
    const n=new URLSearchParams(sp)
    n.set('offset',String((clamped-1)*limit))
    setSp(n,{replace:true})
  }
  const jump=()=>{
    const p=Number(pageInput)
    if(Number.isInteger(p)&&p>=1) goToPage(p)
    else setPageInput(String(page))
  }
  return (<>
    <Header auto={auto} onAuto={onAuto} middle={<>
      <button disabled={page<=1} onClick={()=>goToPage(1)}
        className={navBtn} title="first page" aria-label="First page">{'<<'}</button>
      <button disabled={page<=1} onClick={()=>goToPage(page-1)}
        className={navBtn}>
        <span className="inline-flex items-center gap-1"><svg className="w-3 h-3" fill="none" viewBox="0 0 12 12" stroke="currentColor" strokeWidth={1.5} aria-hidden="true"><path d="M7 2L3 6l4 4" strokeLinecap="round" strokeLinejoin="round"/></svg>Prev</span>
      </button>
      <span className="text-sm text-[var(--text)] whitespace-nowrap">
        Page <input
          value={pageInput}
          onChange={e=>setPageInput(e.target.value.replace(/\D/g,''))}
          onKeyDown={e=>{if(e.key==='Enter')jump()}}
          className="w-12 border border-[var(--border)] rounded px-1.5 py-0.5 text-sm bg-transparent text-[var(--text)] text-center focus:outline-none focus:border-[var(--accent)]"
        /> of <span className="muted">{totalPages}</span>
      </span>
      <button disabled={page>=totalPages} onClick={()=>goToPage(page+1)}
        className={navBtn}>
        <span className="inline-flex items-center gap-1">Next<svg className="w-3 h-3" fill="none" viewBox="0 0 12 12" stroke="currentColor" strokeWidth={1.5} aria-hidden="true"><path d="M5 2l4 4-4 4" strokeLinecap="round" strokeLinejoin="round"/></svg></span>
      </button>
      <button disabled={page>=totalPages} onClick={()=>goToPage(totalPages)}
        className={navBtn} title="last page" aria-label="Last page">{'>>'}</button>
      <span className="muted text-sm whitespace-nowrap">{count} total</span>
    </>}/>
    <main className="px-4">
      <div className="flex flex-wrap gap-2 my-2">
        <SelectDropdown value={sp.get('method')??''} options={METHOD_OPTIONS} onChange={v=>set('method',v)} placeholder="Method" className="flex-1 min-w-[120px]"/>
        {(['status','path','client_addr','since','until','error'] as const).map(k=>(
          <input key={k} placeholder={k} value={sp.get(k)??''} onChange={e=>set(k,e.target.value)} className="flex-1 min-w-[8rem] border border-[var(--border)] rounded px-2 py-1 text-sm bg-transparent text-[var(--text)] placeholder-[var(--muted)]"/>
        ))}
        <button onClick={clearFilters}
          className="border border-[var(--border)] rounded px-2.5 py-1 text-sm bg-transparent text-[var(--text)] hover:border-[var(--accent)] transition-colors">
          Clear filters
        </button>
        <a href={exportUrl()} download
          className="border border-[var(--border)] rounded px-2.5 py-1 text-sm bg-transparent text-[var(--text)] hover:border-[var(--accent)] transition-colors">
          Export HAR
        </a>
      </div>
      {stats && (
        <div className="flex flex-col gap-3 mb-3">
          <KpiPanel byMethodStatus={stats.by_method_status} total={stats.total}/>
          <TimeGraph hourly={stats.hourly} onHourClick={onHourClick}/>
        </div>
      )}
      <CapturesTable captures={rows}/>
    </main>
  </>)
}
