import { useState } from 'react'
import { Routes, Route, Link } from 'react-router'
import List from './pages/List.tsx'
import Detail from './pages/Detail.tsx'
export default function App(){
  const [theme,setTheme]=useState<''|'light'>(()=>{
    const t = localStorage.getItem('ghost-theme')
    return t === 'light' ? 'light' : ''
  })
  const [auto,setAuto]=useState<boolean>(()=>localStorage.getItem('ghost-auto') !== '0')
  const isLight = theme === 'light'
  if (theme) document.documentElement.setAttribute('data-theme', theme)
  const toggle = ()=>{
    const v = isLight ? '' : 'light'
    setTheme(v as any)
    document.documentElement.setAttribute('data-theme', v)
    localStorage.setItem('ghost-theme', v)
  }
  return (<>
    <header className="flex items-center justify-between px-4 py-2.5">
      <div className="flex items-center gap-4">
        <strong>ghost</strong>
        <nav><Link to="/">list</Link></nav>
      </div>
      <div className="flex items-center gap-4">
        <label className="flex items-center gap-2 text-sm cursor-pointer select-none">
          <input type="checkbox" checked={auto} onChange={()=>setAuto(a=>{const n=!a;localStorage.setItem('ghost-auto',n?'1':'0');return n})}
            className="sr-only"/>
          <span style={{
            display: 'inline-block', width: 36, height: 20, borderRadius: 10,
            background: auto ? 'var(--s-2xx)' : 'var(--border)',
            position: 'relative', transition: 'background 0.2s', flexShrink: 0,
          }}>
            <span style={{
              display: 'inline-block', width: 16, height: 16, borderRadius: '50%',
              background: '#fff', position: 'absolute', top: 2, left: 2,
              transition: 'transform 0.2s',
              transform: auto ? 'translateX(16px)' : 'translateX(0)',
            }}/>
          </span>
          <span>Auto 5s</span>
        </label>
        <button onClick={toggle} className="p-1.5 rounded-full hover:bg-[var(--border)] transition-colors cursor-pointer" aria-label={isLight ? 'Switch to dark mode' : 'Switch to light mode'}>
          {isLight ? (
            <svg xmlns="http://www.w3.org/2000/svg" className="w-5 h-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z"/></svg>
          ) : (
            <svg xmlns="http://www.w3.org/2000/svg" className="w-5 h-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><circle cx="12" cy="12" r="5"/><line x1="12" y1="1" x2="12" y2="3"/><line x1="12" y1="21" x2="12" y2="23"/><line x1="4.22" y1="4.22" x2="5.64" y2="5.64"/><line x1="18.36" y1="18.36" x2="19.78" y2="19.78"/><line x1="1" y1="12" x2="3" y2="12"/><line x1="21" y1="12" x2="23" y2="12"/><line x1="4.22" y1="19.78" x2="5.64" y2="18.36"/><line x1="18.36" y1="5.64" x2="19.78" y2="4.22"/></svg>
          )}
        </button>
      </div>
    </header>
    <main className="px-4">
      <Routes>
        <Route path="/" element={<List auto={auto}/>} />
        <Route path="/captures/:id" element={<Detail/>} />
      </Routes>
    </main>
  </>)
}
