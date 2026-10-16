import { useState, useEffect, useCallback } from 'react'
import { useSearchParams } from 'react-router'
import { listCaptures } from '../api'
import type { CaptureListItem } from '../api'
import CapturesTable from '../components/CapturesTable'
import SelectDropdown from '../components/SelectDropdown'

const HTTP_METHODS = ['GET','POST','PUT','DELETE','PATCH','HEAD','OPTIONS','CONNECT','TRACE']
const METHOD_OPTIONS = [{ label: 'ALL', value: '' }, ...HTTP_METHODS.map(m => ({ label: m, value: m }))]

export default function List({ auto }: { auto: boolean }) {
  const [sp,setSp]=useSearchParams()
  const [rows,setRows]=useState<CaptureListItem[]>([])
  const [count,setCount]=useState(0)
  const limit=20
  const offset=Number(sp.get('offset')||'0')
  const load=useCallback(async()=>{
    const q:{[k:string]:any}={}
    for (const k of ['method','status','path','client_addr','since','until','error']) {
      const v=sp.get(k); if(v!==null) q[k]=v
    }
    const r=await listCaptures({...q,limit,offset})
    setRows(r.captures); setCount(r.count)
  },[sp,offset])
  useEffect(()=>{load()},[load])
  useEffect(()=>{
    if(!auto) return
    const t=setInterval(load,5000)
    return ()=>clearInterval(t)
  },[auto,load])
  const set=(k:string,v:string)=>{
    const n=new URLSearchParams(sp)
    if(v==='')n.delete(k); else n.set(k,v)
    setSp(n,{replace:true})
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
    <div className="flex flex-wrap gap-2 my-2">
      <SelectDropdown value={sp.get('method')??''} options={METHOD_OPTIONS} onChange={v=>set('method',v)} placeholder="Method"/>
      {(['status','path','client_addr','since','until','error'] as const).map(k=>(
        <input key={k} placeholder={k} value={sp.get(k)??''} onChange={e=>set(k,e.target.value)} className="border border-[var(--border)] rounded px-2 py-1 text-sm bg-transparent text-[var(--text)] placeholder-[var(--muted)]"/>
      ))}
    </div>
    <CapturesTable captures={rows}/>
    <div className="flex flex-wrap items-center gap-2 my-2">
      <button disabled={page<=1} onClick={()=>goToPage(page-1)}
        className="border border-[var(--border)] rounded px-2.5 py-1 text-sm bg-transparent text-[var(--text)] hover:border-[var(--accent)] transition-colors disabled:opacity-40 disabled:cursor-not-allowed disabled:hover:border-[var(--border)]">
        <span className="inline-flex items-center gap-1"><svg className="w-3 h-3" fill="none" viewBox="0 0 12 12" stroke="currentColor" strokeWidth={1.5} aria-hidden="true"><path d="M7 2L3 6l4 4" strokeLinecap="round" strokeLinejoin="round"/></svg>First</span>
      </button>
      <button disabled={page<=1} onClick={()=>goToPage(page-1)}
        className="border border-[var(--border)] rounded px-2.5 py-1 text-sm bg-transparent text-[var(--text)] hover:border-[var(--accent)] transition-colors disabled:opacity-40 disabled:cursor-not-allowed disabled:hover:border-[var(--border)]">
        <span className="inline-flex items-center gap-1"><svg className="w-3 h-3" fill="none" viewBox="0 0 12 12" stroke="currentColor" strokeWidth={1.5} aria-hidden="true"><path d="M7 2L3 6l4 4" strokeLinecap="round" strokeLinejoin="round"/></svg>Prev</span>
      </button>
      <span className="text-sm text-[var(--text)]">
        Page <input
          value={pageInput}
          onChange={e=>setPageInput(e.target.value.replace(/\D/g,''))}
          onKeyDown={e=>{if(e.key==='Enter')jump()}}
          className="w-12 border border-[var(--border)] rounded px-1.5 py-0.5 text-sm bg-transparent text-[var(--text)] text-center focus:outline-none focus:border-[var(--accent)]"
        /> of <span className="muted">{totalPages}</span>
      </span>
      <button disabled={page>=totalPages} onClick={()=>goToPage(page+1)}
        className="border border-[var(--border)] rounded px-2.5 py-1 text-sm bg-transparent text-[var(--text)] hover:border-[var(--accent)] transition-colors disabled:opacity-40 disabled:cursor-not-allowed disabled:hover:border-[var(--border)]">
        <span className="inline-flex items-center gap-1">Next<svg className="w-3 h-3" fill="none" viewBox="0 0 12 12" stroke="currentColor" strokeWidth={1.5} aria-hidden="true"><path d="M5 2l4 4-4 4" strokeLinecap="round" strokeLinejoin="round"/></svg></span>
      </button>
      <button disabled={page>=totalPages} onClick={()=>goToPage(totalPages)}
        className="border border-[var(--border)] rounded px-2.5 py-1 text-sm bg-transparent text-[var(--text)] hover:border-[var(--accent)] transition-colors disabled:opacity-40 disabled:cursor-not-allowed disabled:hover:border-[var(--border)]">
        <span className="inline-flex items-center gap-1">Last<svg className="w-3 h-3" fill="none" viewBox="0 0 12 12" stroke="currentColor" strokeWidth={1.5} aria-hidden="true"><path d="M5 2l4 4-4 4" strokeLinecap="round" strokeLinejoin="round"/></svg></span>
      </button>
      <span className="muted text-sm">{count} total</span>
    </div>
  </>)
}
